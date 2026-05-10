# Performance Review — 20260510T052023Z

## Summary

Slice 11 buys correctness against `%`-expansion and ANSI injection, paid
in allocations and an extra branch on the binary's warmest path: each
prompt now allocates one `String` per untrusted field. Costs are µs,
not ms — none of this jeopardises MVP-SPEC § 0's < 5 ms warm budget. The
two prior-swarm flags (`wrap_for_shell` two-pass loop, process-spawn
ceiling) are slightly worse, not better. No re-rate; prior rankings hold.

## Findings

### [MEDIUM] `sanitize_for_terminal` always allocates, including the no-control hot case
**Location:** `crates/p10k-rs-core/src/safety.rs:42-55`; called from
`crates/p10k-rs-git/src/gitstatusd.rs:187-188`,
`crates/p10k-rs-git/src/lib.rs:103-106`,
`crates/p10k-rs-segments/src/dir.rs:26`
**Issue:** Every call returns a fresh `String` with capacity
`s.len()`. The overwhelmingly common case — branch / commit / cwd contains
no control bytes — pays a heap allocation and a per-`char` decode + branch
+ push to copy the input verbatim. Three call sites = three allocations
per prompt where zero were needed before slice 11. The `chars()` iterator
also re-decodes UTF-8, ~4× slower than `as_bytes().iter()` on
ASCII-dominated input.
**Suggested fix:** Fast-path the common case. Scan with
`s.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7F)` first (linear,
no alloc, vectorisable, picks up every ASCII control — the only kind a
git ref or POSIX path realistically carries). On hit, fall through to the
current `chars()` walk for the C1 unicode tail. Returning `Cow<'_, str>`
keeps call sites ergonomic. Saves three allocations per warm prompt.

### [MEDIUM] `wrap_for_shell` byte loop now does two scalar branches per byte; `memchr` rewrite gets more attractive
**Location:** `crates/p10k-rs-core/src/lib.rs:195-231`
**Issue:** Prior review (`.review/20260509T071500Z/03-performance.md` §
HIGH 2) called for replacing the two-pass byte walk with a single
`memchr`-driven pass. Slice 11 added a third condition (`bytes[i] ==
b'%'`, line 220-224) inside the per-byte loop. The fast-out short-circuit
(line 199) now requires *both* `\x1b` and `%` absence; every prompt the
binary emits contains at least one SGR escape, so the short-circuit fires
on no real prompt. Cost per warm prompt: inner loop ~60 iterations on a
typical prompt, each with up to three byte comparisons plus a
`next_char_boundary` call.
**Suggested fix:** Single `memchr2(b'\x1b', b'%', ...)` pass. Pre-size
once: `s.len() + 4 * occurrences`. `memchr` is already a transitive dep.
Same severity as before — slightly more attractive now because the extra
branch raises the constant factor on every byte.

### [HIGH] Process-spawn-per-prompt ceiling — unchanged, still the dominant warm-path term
**Location:** `crates/p10k-rs/src/main.rs:87-89`;
`crates/p10k-rs-shell/shells/zsh/init.zsh:147` (per prior review)
**Issue:** Slice 11 didn't touch this. Re-rating per dispatch: still HIGH.
fork+exec+dynamic-link+clap-parse is ~1-2 ms native Linux, several × that
on WSL2. Sub-ms gitstatusd RTT means this term sets the realistic floor
against MVP-SPEC § 2's "< 5 ms" target. The slice-11 sanitise + `%`
doubling add ~µs each — they don't move this needle, and they don't help.
**Suggested fix:** As prior review: bypass clap for the `prompt`
subcommand (peek `args_os().nth(1)`, hand-roll the six flags), bump
release profile to `lto = "fat"`. Both additive, both ship without an
architectural change. Long-term: the post-MVP daemon (deferred to v0.2 in
MVP-SPEC § 1.5) is the real fix.

### [MEDIUM] `Dir::render` now allocates four times for the cwd path
**Location:** `crates/p10k-rs-segments/src/dir.rs:23-39`
**Issue:** Pre-slice-11: one `format!` for the styled output. Post:
`ctx.cwd.display().to_string()` (1) → `sanitize_for_terminal(&...)` (2) →
`home_collapse(&raw, ...)` returns either a fresh `path.to_owned()`
(line 59) or a `format!("~{rest}")` (line 56) (3) →
`format!("\x1b[34m{collapsed}\x1b[39m")` (4). Four heap allocations in
the cwd path alone. The "no home prefix" branch unconditionally clones a
string we already own.
**Suggested fix:** Return `Cow<'_, str>` from `home_collapse`. Combine
sanitise + collapse + style into a single `String` build: write
`\x1b[34m`, walk pushing safe chars (with home-prefix substitution
folded in), write `\x1b[39m`. Drops three of four allocations and a
UTF-8 re-decode pass.

### [MEDIUM] `gitstatusd::parse_response` stacks utf8-lossy + sanitize, two extra allocations
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:182-200`
**Issue:** `sanitize_for_terminal(&String::from_utf8_lossy(fields[i]))`
(line 188) materialises a `Cow<'_, str>` (free if valid UTF-8, one alloc
otherwise) and *then* hands it to `sanitize_for_terminal`, which
unconditionally allocates a second `String`. Branch + commit = up to four
allocations per prompt where slice 10 had two. Combined with the
`Vec<&[u8]>` allocation (still open from prior), `parse_response` is now
the heaviest allocator on the gitstatusd-success path.
**Suggested fix:** Fold sanitisation into the lossy decode in one pass:
walk `fields[i]` byte-by-byte, decode UTF-8 manually, skip control bytes
inline, push `\u{FFFD}` for invalid sequences. Single `String` per
untrusted field. If too much surgery for one slice: take ownership —
`sanitize_for_terminal(s: Cow<'_, str>) -> String` and pass the lossy
`Cow` directly so the no-control valid-UTF-8 case skips one alloc.

### [LOW] `is_control()` per-codepoint slower than a byte-table for ASCII-dominated inputs
**Location:** `crates/p10k-rs-core/src/safety.rs:49-53`
**Issue:** `char::is_control()` is correct but generic — dispatches
through Unicode tables. Branch names and POSIX paths are >99% ASCII; a
single-range comparison
(`c < '\u{20}' || c == '\u{7F}' || ('\u{80}'..='\u{9F}').contains(&c)`)
beats it for the common case and auto-vectorises.
**Suggested fix:** If the fast-path from finding 1 lands, this collapses
into the slow-path scanner. Otherwise inline the three-range check.
Sub-µs improvement; file under "while you're in there."

### [LOW] `String::with_capacity(s.len() + 16)` undersizes when `%` doubling fires
**Location:** `crates/p10k-rs-core/src/lib.rs:203`
**Issue:** Capacity hint is `s.len() + 16`. Each SGR adds 4 bytes (`%{`
+ `%}`); each `%` adds 1. A typical prompt has ~6 SGRs → +24, already
over the +16 reserve, plus any `%` in the cwd. Realloc fires on most
prompts now.
**Suggested fix:** `s.len() + 4 * bytes.iter().filter(|&&b| b == 0x1B
|| b == b'%').count()`. The `memchr` rewrite gets the count for free.

### [LOW] `default_layout` rebuilds 5 boxes per prompt — unchanged
**Location:** `crates/p10k-rs-segments/src/lib.rs:65-73`
**Issue:** Carryover from prior MEDIUM. Five `Box<dyn Segment>` per warm
prompt for ZSTs. Slice 11 didn't touch it. Mentioned because slice-11
allocations make the cumulative overhead more visible.
**Suggested fix:** No change from prior recommendation. Bundle with the
allocation-reduction maintenance slice.

### [INFO] Cold-cache `ShellOut`: sanitise cost invisible against fork+exec
**Location:** `crates/p10k-rs-git/src/lib.rs:103-106`
**Issue:** `ShellOut` pays fork+exec of `git status` (~10-30 ms native).
The added sanitise on the parsed branch is correctness; its µs is
invisible against spawn cost. Brief asked about cold-cache; no action.

### [INFO] Carryover findings unchanged
**Location:** `.review/20260509T071500Z/03-performance.md` (findings 3,
6, 7, 8, 9, 11): `read_until_with_deadline` rescan, `Vec<&[u8]>`
materialisation, FIFO open/close, `kill -0` polling, `getcwd` syscall,
dump tempfile churn. Slice 11 didn't touch any. Listed for synthesis
carry-forward; queue for a maintenance slice.

### [INFO] Slice 11 introduces no new syscalls
`sanitize_for_terminal` and `%` doubling are pure in-process work. No
`stat`/`read`/`write`/`open` added. Costs are allocator pressure and
CPU branches, not kernel boundaries — which is what otherwise inflates
p99 prompt latency under load.

## Things this review explicitly did NOT examine

- Rust idioms / `Cow` API ergonomics (lane #01)
- Whether strip-vs-replace is the right security posture (lane #02)
- Comment quality / function length (lane #04)
- Doc / ADR alignment (lane #05)
- Architecture / slice-boundary cleanliness (lane #06)
- Live benchmarks — read-only constraint

## Confidence

Medium-high. Slice-11 alloc/branch additions are mechanically clear from
the diff; severity carries the prior review's calibration (anchored on
`bench/results/`). Absolute µs are estimates from standard fork+exec and
allocator costs, not measured here; relative ranking is robust.
Process-spawn re-rate rests on unchanged code at the cited lines.
