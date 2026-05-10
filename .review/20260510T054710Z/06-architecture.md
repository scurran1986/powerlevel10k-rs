# Architecture & Other Review — 20260510T054710Z

## Summary

Slice 12-b lands the `SafeText` newtype recommended by the prior swarm
([20260510T052023Z/06 MEDIUM "sanitisation gate at the producer leaks
responsibility"](../20260510T052023Z/06-architecture.md)). The
implementation is right-shaped: `core::safety::SafeText`, no
`assume_safe` escape hatch, byte-stream constructor, three traits the
renderer actually consumes (`AsRef<str>`, `Display`,
`PartialEq<&str>`), and a clean migration of `GitState::branch` and
`GitState::commit`. ADR-0001 follow-ups remain closed; dep graph stays
clean. Two architectural concerns: the migration is **partial** —
`SegmentOutput::text`, `Prompt::left/right`, and `RenderCtx::cwd` are
still raw `String`/`&Path`, so the "compile-error if you forget"
property only holds for the git fields — and the producer→render
integration test the prior swarm asked for is still missing. CHANGELOG
also has no slice-12-b entry yet.

## Findings

### [HIGH] CHANGELOG drift returns one slice later
**Location:** `CHANGELOG.md:69-73` (last entry: slice 12-a).
**Issue:** Slice 12-a explicitly backfilled the prior HIGH ("CHANGELOG
drift unfixed across three slices"). Slice 12-b is the very next
commit and again has no entry — same shape of drift, same artefact.
The slice that closes a HIGH from the previous swarm by adding a
type-system invariant is exactly the one users grepping CHANGELOG for
"safety" or "sanitis" will want to find.
**Suggested fix:** Add a "Slice 12-b" entry before the next merge:
note the `SafeText` newtype, that it migrates `GitState::branch` /
`GitState::commit`, that `RenderCtx::cwd` and `SegmentOutput::text`
are deferred, and the test count delta (53 → 60).

### [HIGH] Migration is half-done; the type guarantee is field-local, not pipeline-wide
**Location:** `crates/p10k-rs-core/src/lib.rs:130`
(`SegmentOutput::text: String`); `crates/p10k-rs-core/src/lib.rs:148-150`
(`Prompt::left/right: String`); `crates/p10k-rs-core/src/lib.rs:106`
(`RenderCtx::cwd: &Path`).
**Issue:** The slice-11 swarm asked for "forgetting to sanitise is a
compile error." `SafeText` delivers that property only for the two
fields it landed on. `Dir::render` still receives `&Path` and calls
`sanitize_for_terminal` by hand at `dir.rs:26`; if a future segment
forgets that one line, C2 reopens for cwd. `SegmentOutput::text` is a
plain `String` carrying ANSI-decorated output — the renderer at
`lib.rs:174` (`left.push_str(&out.text)`) trusts segments to have
sanitised the underlying user data before formatting. That's the same
producer-discipline pattern the newtype was meant to retire, just
moved one level up. The commit message acknowledges the cwd deferral
as "fits more naturally with a wider RenderCtx audit"; that audit
needs to land before slice 13 doubles the number of producer fields.
**Suggested fix:** Two-step. (a) `RenderCtx::cwd: SafeText` (built
once in the binary glue from `cwd.display().to_string()` →
`SafeText::from`); delete the inline call at `dir.rs:26`. (b) Decide
the `SegmentOutput::text` story explicitly: either accept that
"already-sanitised plus ANSI" lives outside the newtype's invariant
(document why), or introduce a sibling `StyledSafeText` that wraps
SGR-bearing output and accepts only `SafeText` payloads in its
constructors. Pick one, write it down in `safety.rs`'s module doc.

### [MEDIUM] No producer→render integration test — second swarm running, still open
**Location:** workspace-wide; closest pointers
`crates/p10k-rs-segments/src/dir.rs:65-130`,
`crates/p10k-rs-core/src/lib.rs:248-323`.
**Issue:** Prior swarm flagged this as MEDIUM. Slice 12-b adds 7 unit
tests for `SafeText` itself but no test that drives `render_prompt`
end-to-end with a hostile `RenderCtx { cwd: "…\rEVIL", git: Some(&
GitState { branch: SafeText::from("%n@%m\x1b]0;evil\x07"), …}) }` and
asserts the final wire string contains `%%n@%%m`, no `\x1b`, no `\x07`,
no bare `%`. The composition (`Dir` sanitises + `Vcs` consumes
`SafeText` + `wrap_for_shell` doubles) is now load-bearing across
three crates and verified only by the two `/tmp/p10ktest` reproducers
in the commit message. No `tests/` integration directory exists in
any crate.
**Suggested fix:** Add `crates/p10k-rs-segments/tests/render_pipeline.rs`
that builds a `Vec<Box<dyn Segment>>` of `[Dir, Vcs]`, a `RenderCtx`
with both attack vectors loaded, calls `render_prompt`, and pins the
output. Cheaper than re-running ad-hoc `/tmp/` repros every slice.

### [MEDIUM] `SafeText` placement is right; surface scales for slice-13 — with one exception
**Location:** `crates/p10k-rs-core/src/safety.rs:78-121`.
**Issue:** Same answer as the slice-11 question: `core` owns the
I/O-free contract, `git` and `segments` both consume — placement is
correct and `p10k-rs-config` / `p10k-rs-ai` will reuse it without
inverting any deps. For slice-13 segments the surface mostly fits:
`hostname`, `user`, `kubecontext`, `aws_profile`, `virtualenv` are all
single-line untrusted strings — `SafeText::from_untrusted(&s)` is a
one-liner per producer. The exception is `time`/`now`: it's
formatted from a trusted `SystemTime` via `strftime`-style, so making
the segment's output `SafeText` would force a no-op sanitise pass on
known-safe bytes. Cost is negligible (one allocation, no controls
present), but worth documenting that segments which produce text from
trusted-only inputs still pay the toll. Acceptable; alternative is a
`SafeText::trusted(&str)` constructor for literals only, and that's a
foothold for the escape hatch the design deliberately rejected.
**Suggested fix:** None. Document in `safety.rs`'s module doc that
all-trusted-source text still goes through the sanitiser by design,
because the alternative escape hatch costs more than the no-op
allocation.

### [MEDIUM] `SafeText` doesn't enforce its invariant against re-entry from `String`
**Location:** `crates/p10k-rs-core/src/safety.rs:78-79`.
**Issue:** The struct derives `Clone`, `PartialEq`, `Hash`, `Eq` and
exposes `as_str(&self) -> &str` — exactly the surface a consumer
needs. Good. But `serde::Deserialize` is not implemented, and slice 12
is supposed to be the TOML-config slice (per the prior swarm and
`p10k-rs-config` references at `lib.rs:13`). When config grows
`POWERLEVEL9K_VCS_BRANCH_PREFIX = "on "` (or any user-typeable
string), the deserialiser will produce a `String`. If a future
`Config` field is `SafeText`, a hand-rolled `Deserialize` impl that
calls `from_untrusted` is correct; if someone reaches for
`#[serde(transparent)]` or implements `From<String>` later for
ergonomics, the invariant evaporates.
**Suggested fix:** Add a `#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for SafeText` that calls `from_untrusted`
on the deserialised `String`. Do it now, before
`p10k-rs-config` lands a `pub struct VcsConfig { prefix: String }` and
the type-system invariant has to chase the config schema retroactively.

### [LOW] Dep graph stays clean — no phantom imports
**Location:** workspace-wide (verified by grep on
`p10k_rs_core::safety` and `SafeText`).
**Issue:** All six hits live in code paths that genuinely consume the
type: `core/src/safety.rs` (defines), `core/src/lib.rs:27` (uses in
`GitState`), `git/src/lib.rs:25` + `git/src/gitstatusd.rs:36` (both
parser sites), `segments/src/dir.rs:8` (still uses the function, not
the newtype). No `pub use` re-exports, no test-only leaks,
`segments/Cargo.toml` correctly lists `p10k-rs-config` as a workspace
dep without pulling `p10k-rs-git`. Boundary clean.
**Suggested fix:** None.

### [LOW] ADR-0001 follow-ups still all closed, again
**Location:** `docs/adr/0001-git-backend.md:103-108`.
**Issue:** Re-verified post-slice-12-b. Spike crate absent, `gix`
absent from `[workspace.dependencies]` (manifest comments at
`Cargo.toml` confirm the strip), `THIRD-PARTY-LICENSES.md` present,
`ARCHITECTURE.md § 2.4` still DEFERRED (planning bundle outside
repo, not a regression). No slice-12-b regression.
**Suggested fix:** None.

### [LOW] Test discipline — inline placement is correct, count claim is honest-ish
**Location:** `crates/p10k-rs-core/src/safety.rs:164-275`.
**Issue:** Commit message says "60 tests pass (was 53)". `grep -c
'#[test]'` across `crates/` totals 58 marked tests; the doc-test in
`SafeText`'s rustdoc and the `sanitize_for_terminal` doctest add 2,
giving 60 once `cargo test` collects both. Within rounding. The 7
new `safety` tests are unit-inline next to `SafeText` — correct
boundary for pure-function tests on private internals
(`safety_text_default_is_empty` exercises `Default`, etc.). What's
missing is the integration layer (see [MEDIUM] above), not the
location of these tests.
**Suggested fix:** None on existing tests.

### [LOW] Reverse-direction `PartialEq` asymmetry is documented; consider tests
**Location:** `crates/p10k-rs-core/src/safety.rs:141-162`.
**Issue:** The comment is explicit: `assert_eq!(safe, "main")` works,
`assert_eq!("main", safe)` does not. That's a deliberate trade-off
(small foreign-impl footprint) and the test at `lib.rs:127-147`
(`vcs::tests::renders_branch_clean`) demonstrates the LHS form
working. Risk: future contributors will write the reversed form,
get a confusing compile error, and either flip the assertion or add
the missing impl without re-reading the comment. Low impact, but
flagging.
**Suggested fix:** Add a `compile_fail` doctest on `SafeText`
showing that `"main" == SafeText::from("main")` does not compile, so
the asymmetry is part of the documented contract not just an
overheard rationale.

### [INFO] Producer-discipline asymmetry is now load-bearing, not incidental
**Location:** commit message "Deferred: `RenderCtx::cwd` migration to
SafeText"; `crates/p10k-rs-segments/src/dir.rs:23-26`.
**Issue:** The commit explicitly defers the cwd migration as
"fits more naturally with a wider `RenderCtx` audit." Reading the
diff, that is defensible — `cwd` is a `&Path`, not a `String`, so
the migration is non-trivial. But the asymmetry is now a documented
architectural feature: git fields enforce the invariant via type,
cwd enforces it via call-site discipline at exactly one site. If
slice 13 adds e.g. `RenderCtx::hostname: &str` from `gethostname()`
without migrating cwd in the same slice, the asymmetry becomes a
pattern. Worth tracking explicitly.
**Suggested fix:** Note in synthesis that the wider `RenderCtx`
audit (cwd → `SafeText`, plus any other untrusted fields slice 13
adds) is a single coherent slice; resist the temptation to spread it
across two.

### [INFO] No regression on the slice-11 defence-in-depth tests
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:286-369`,
`crates/p10k-rs-git/src/lib.rs:172-185`.
**Issue:** All slice-11 sanitisation tests still pass after the
migration to `SafeText`. The byte-equality assertions
(`assert_eq!(s.branch, "main]0;TARS-OWNED")`) ride the new
`PartialEq<&str>` impl and didn't need rewriting — that ergonomic
choice in the newtype paid for itself immediately. Good signal that
the `SafeText` surface is sized right.
**Suggested fix:** None.

## Things this review explicitly did NOT examine

- Idiomatic Rust on `SafeText`'s trait set / lifetimes (lane 01).
- Whether sanitiser still meets latency budget on hot path (lane 03).
- Doc-comment grammar / ADR cross-refs in narrative prose (lane 05).
- IPC/FIFO H3/H4 — still deferred from slice 11; lanes 02/03 own.
- Wizard, ai, ipc placeholder crates — unchanged in slice 12-b.
- The `RenderCtx`-wide audit's actual implementation (work not yet
  in HEAD; reviewable when it lands).

## Confidence

High on the core verdict (`SafeText` placement, dep graph, ADR
status, test count, CHANGELOG drift): read every changed file in the
slice, the surrounding callers in `dir.rs` / `vcs.rs`, the prior swarm
findings, ADR, manifests, and CHANGELOG without compilation. Medium
on the "migration is half-done" framing — partly a values call about
how aggressively to enforce a typeclass invariant before slice 13's
producers exist to test against.
