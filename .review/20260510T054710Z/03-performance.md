# Performance Review — 20260510T054710Z

## Summary

Slice 12-b is type-system reshape of slice 11's allocating sanitiser
path. Allocation count, syscalls, and hot-path branches are byte-for-byte
unchanged: same two `from_utf8_lossy` + `sanitize_for_terminal`
allocations per untrusted field, same `Dir::render` cost. `&git.branch`
→ `git.branch.as_str()` is a codegen no-op. Net: zero regression, zero
gain. Prior findings carry at same severity. The two cheap wins the
brief asked about (`Cow<str>` from sanitise; single-pass lossy+sanitise)
become *more* attractive now — `SafeText` is the natural locus to absorb
the fix without touching every call site again.

## Findings

### [MEDIUM] `SafeText` constructors still allocate on the no-change hot path
**Location:** `crates/p10k-rs-core/src/safety.rs:87-89, 99-101`
(underlying sanitiser at `:42-55`)
**Issue:** `from_untrusted` calls `sanitize_for_terminal(s)`, which
unconditionally builds a fresh `String` via `chars()` + per-codepoint
push, even when the input has zero control bytes. The slice 12-b
commit message acknowledges this. Branch + commit + cwd = three
allocations per prompt where zero are needed in the overwhelming
no-control case. `chars()` also re-decodes UTF-8 ~4× slower than
`bytes().iter()` on ASCII input — which branch names and OIDs are
100% of the time.
**Suggested fix:** Re-rate stays MEDIUM; fix shape changes. Rather
than `Cow<str>` from `sanitize_for_terminal` (forces every consumer
to handle two arms), fold the fast path into `SafeText` itself: scan
`bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7F)` first; on
miss, `Self(s.to_owned())` — still one alloc, but no decode + no
per-char branch. On hit, fall through to the current walk for the C1
tail. Storing `Cow<'_, str>` inside `SafeText` is a bigger win but
forces a lifetime onto `GitState` — own-slice. Recommend the byte
fast-path now.

### [MEDIUM] `from_untrusted_bytes` is a two-pass, two-alloc pipeline
**Location:** `crates/p10k-rs-core/src/safety.rs:99-101`
**Issue:** `String::from_utf8_lossy(b)` is `Cow<str>` — free on valid
UTF-8, one alloc otherwise. Then `sanitize_for_terminal` unconditionally
allocates a second `String`. Worst case: two allocs + two passes, plus
materialising `\u{FFFD}` runs that the second pass re-copies. Hits twice
per prompt on warm gitstatusd path (branch + commit, `gitstatusd.rs:198-199`).
**Suggested fix:** Worth it; lands naturally in `SafeText`. Walk `b`
with a small UTF-8 decoder (chunk valid runs via `std::str::from_utf8`,
push `\u{FFFD}` once per invalid run, skip controls inline). Single
`String::with_capacity(b.len())`. Saves 2 allocs worst case, 1 in the
common (valid UTF-8) case — and the byte fast-path in finding 1 then
elides the remaining alloc when no controls are present.

### [INFO] `&git.branch` → `git.branch.as_str()` is a codegen no-op
**Location:** `crates/p10k-rs-segments/src/vcs.rs:46`
**Issue:** Pre-12-b: `&git.branch` (where `branch: String`) goes through
`Deref::deref` — a ptr+len load, zero instructions in release because
`String::deref` is trivially inlined. Post: `SafeText::as_str(&self) -> &self.0`
— same ptr+len load. LLVM emits identical assembly.
**Suggested fix:** None. Mentioned because the brief asked. Adding
`#[inline]` to `SafeText::as_str`/`len` would harmonise debug + opt-1
codegen with release; trivially worth it while in there.

### [LOW] `From<&str>` allocates on every test fixture and any future production constant
**Location:** `crates/p10k-rs-core/src/safety.rs:135-139`; consumers
`crates/p10k-rs-segments/src/vcs.rs:138, 153, 169, 185`
**Issue:** All `"main".into()` callers are tests today. Once a
production site reaches for a constant (e.g. a `"HEAD"` placeholder
or default fallback branch), every prompt pays a fresh sanitise walk
+ alloc on a 4-byte string. Per-call sub-µs; cumulative depends on
fire-rate.
**Suggested fix:** Keep `From<&str>` (test ergonomics are real); add
a `SafeText::from_static(s: &'static str) -> Self` that debug-asserts
no control bytes and skips sanitising. The `&'static` lifetime is the
safety knob. Alternative: skip and document — zero production callers
today; re-rate if one appears.

### [HIGH] Process-spawn-per-prompt ceiling — unchanged, dominant warm-path term
**Location:** `crates/p10k-rs/src/main.rs` (entry); shell init scripts
**Issue:** Slice 12-b doesn't touch binary or shell-init. fork+exec+
dynamic-link+clap-parse ~1-2 ms native Linux, several × on WSL2. Sub-ms
gitstatusd RTT plus µs-level SafeText work means this remains the
realistic floor against MVP-SPEC § 0's < 5 ms warm budget. Re-rate
stays HIGH.
**Suggested fix:** Unchanged — bypass clap on the hot `prompt`
subcommand (peek `args_os().nth(1)`, hand-roll the six flags), bump
release profile to `lto = "fat"`. Post-MVP in-process daemon (MVP-SPEC
§ 1.5, v0.2) is the real fix. Slice 12-b was type-safety, not perf —
no surprise it doesn't move this needle.

### [LOW] `parse_branch_header` chains String → String → SafeText
**Location:** `crates/p10k-rs-git/src/lib.rs:103-105`
**Issue:** `parse_branch_header_raw(header)` returns an owned `String`
(via `to_owned()`); `SafeText::from_untrusted(&raw)` reallocates inside
`sanitize_for_terminal`. Intermediate `String` dropped immediately. Two
allocs where one would suffice. Real cost, but invisible — `ShellOut`
is the slow fallback and fork+exec dwarfs it.
**Suggested fix:** `parse_branch_header_raw` returns `&'a str`
borrowed from the input. `SafeText::from_untrusted(...)` becomes the
only allocator. Bundle with the byte-fast-path slice; not urgent.

### [INFO] No new syscalls on the prompt hot path
Slice 12-b is purely type-system surgery. No `read`/`write`/`open`/
`stat`/`getcwd` added or removed; the FIFO dance, the `poll(2)` deadline,
the `is_fifo` lstat — all unchanged at `gitstatusd.rs:71-118, 229-240`.
This is what kept the slice from regressing perf despite touching the
warm path.

### [INFO] `Default` for `SafeText` is zero-cost
**Location:** `crates/p10k-rs-core/src/safety.rs:78`
**Issue:** `String::default()` doesn't allocate (sentinel). The two
`SafeText::default()` values inside `GitState::default()` are free.
Backend returns `None` for "no repo" anyway; default rarely materialises
at runtime.

### [INFO] Slice-11 carryover findings unchanged
**Location:** `.review/20260510T052023Z/03-performance.md` findings 2,
4, 6, 7, 8 (`wrap_for_shell` two-pass + `memchr` rewrite, `Dir::render`
4× alloc, `is_control()` cost, capacity undersize, `default_layout`
Box churn). Slice 12-b touched none. `SafeText` constructor fast-path
is the natural locus for fixes 1, 2, 6 — strongest argument for doing
the maintenance slice next.

## Things this review explicitly did NOT examine

- Rust idioms / `Cow` ergonomics on the `SafeText` API surface (lane #01)
- Whether the strip-vs-replace posture is the right security choice (lane #02)
- Naming, comment quality, function length (lane #04)
- Doc / ADR alignment / RESUME.md staleness (lane #05)
- Architecture / slice-boundary cleanliness (lane #06)
- Live benchmarks — read-only constraint
- Whether `PartialEq<&str>` etc. impl footprint is right (architecture)

## Confidence

High. Slice 12-b is mechanically equivalent to slice 11 on the hot
path — same allocs, same syscalls, same branches. Codegen-equivalence
for `&String` → `as_str()` is standard Rust optimiser behaviour.
Re-rate verdicts anchored on slice-11 calibration (which was anchored
on `bench/results/`). Single-pass UTF-8-lossy + sanitise rewrite is a
known pattern; alloc-count estimates are conservative.
