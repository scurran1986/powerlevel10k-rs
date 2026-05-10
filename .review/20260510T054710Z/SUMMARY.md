# Review Swarm Summary — slice 12-b (HEAD `1c8f80b`)

**Aggregate:** **0 CRITICAL.** Slice 12-b's `SafeText` newtype landed
correctly and security-lane verified it closes the "future-segment
re-introduces unsanitised text" gap from the last swarm. **4 HIGHs**
remain: 1 doc/process repeat, 1 stale-state, 1 half-done migration
critique, 1 perf carryover.

## All HIGH

| # | Reviewer | Finding | Action |
|---|---|---|---|
| 1 | docs + arch | CHANGELOG has no 12-b entry — same pattern as the prior swarm closed for slices 9/10/11. Recurrence after one slice. | Backfill in next commit; commit-discipline going forward (CHANGELOG entry in the slice's own commit). |
| 2 | docs | `RESUME.md` still says "post-slice-11" with HEAD `7438a37`. Stale by one slice. | Update post-12-b. |
| 3 | arch | **Migration is half-done.** SafeText's "compile-error if you forget" property holds for `GitState::branch`/`commit`. `RenderCtx::cwd` (`&Path`), `SegmentOutput::text` (`String`), and `Prompt::left`/`right` (`String`) still rely on producer-discipline (`Dir::render` calls sanitize directly). | Phase 2 migration: split `RenderCtx` into `cwd: &Path` + `cwd_display: SafeText`. `SegmentOutput::text` is harder — must contain SGR escapes the segment itself emitted; partial migration possible at best. |
| 4 | perf | **Process-spawn ceiling** unchanged HIGH — 1.5 MB binary fork+exec is the dominant warm-path term against the < 5 ms budget. Recurring from slice 7. | Bench-driven; deferred to a perf-focused slice. |

## Cross-cutting themes (≥ 2 reviewers)

1. **CHANGELOG drift, again** (docs + arch). Same finding the prior swarm
   raised. The fix that landed in 12-a (backfill all of 9/10/11) didn't
   fix the *process* — 12-b's own commit didn't include its entry.
   Discipline issue, not code.
2. **`SafeText` allocates on the no-change path** (rust + perf). The
   fix locus moves into `SafeText`'s constructor: a byte-fast-path that
   returns the input string borrow when no chars need stripping
   (`Cow<'_, str>`-shaped). Both lanes flag MEDIUM, both more attractive
   post-12-b than pre.
3. **`From<&str>` is implicitly lossy** (rust + perf + docs touched it).
   Violates the `From` "info-preserving" convention; needs at least a
   doc warning. Optionally a `from_static` for compile-time-known-safe
   strings to skip the allocation.
4. **Naming nit** (read): `SafeText` vs `SanitisedText` — the rest of
   the codebase says "sanitize" (`sanitize_for_terminal`,
   `from_untrusted`); the type would pair lexically as `SanitisedText`.
   MEDIUM but bikeshed-class.
5. **`wrap_for_shell` still 3-job** (read + perf-adjacent). Slice-11 swarm
   flagged. Not addressed in 12-b. Stays MEDIUM.

## Severity distribution

- **CRITICAL:** 0.
- **HIGH:** 4 (1 process-spawn ceiling, 1 CHANGELOG drift, 1 RESUME
  stale, 1 half-done migration).
- **MEDIUM:** ~14 (Cow opt, reverse PartialEq, Borrow<str>, naming,
  wrap_for_shell, ADR-0002 sketch, init.zsh body markers, …).
- **LOW:** ~8.
- **INFO:** ~6.

## Per-area headlines

- **01 Rust principles:** 0H 4M. Highest: `From<&str>` lossy convention,
  `from_untrusted` Cow opt, reverse `PartialEq`, missing `Borrow<str>`.
- **02 Security:** **PASS.** SafeText closes the future-segment gap
  definitively; private inner `String` + `#![forbid(unsafe_code)]` +
  every constructor goes through `sanitize_for_terminal` = no bypass
  without a breaking API change. Cwd deferral safe today.
- **03 Performance:** allocation-neutral on hot path. Sanitiser
  no-op-allocates-anyway HIGH from prior swarm restated as MEDIUM with
  a clearer fix locus inside SafeText. Process-spawn ceiling HIGH
  unchanged.
- **04 Readability:** 0H 2M. SafeText naming question; wrap_for_shell
  still 3-job (slice-11 finding restated). 12-a's stale-comment fix
  held for `*.rs` (zero hits) but the doc lane found 5 surviving
  markers in `init.zsh` body that 12-a's sweep missed.
- **05 Documentation:** 2H. CHANGELOG missing 12-b, RESUME.md stale.
  SafeText doc block called out as the new bar.
- **06 Architecture:** 2H 2M. CHANGELOG drift; half-done migration.
  SafeText placement in core is correct. ADR-0002 to record the
  producer→render type-system contract (deferred).

## Suggested slice 13 menu

| | Slice | Effort | What |
|---|---|---|---|
| **13-a** | doc/process cleanup | ~30 min | CHANGELOG 12-b entry, RESUME.md → post-12-b, strip 5 init.zsh body markers, optional ADR-0002. Closes 2 of 4 HIGHs. |
| **13-b** | finish SafeText migration | ~½ day | `RenderCtx`: split `cwd: &Path` + `cwd_display: SafeText`. `Dir::render` becomes a SafeText producer. Closes the half-done HIGH from architecture lane. |
| **13-c** | Cow + Borrow + reverse PartialEq | ~1 hr | Sanitiser returns `Cow<'_, str>`; `SafeText` exposes a fast-path borrow. Adds `Borrow<str>`. Adds reverse `PartialEq` (fixes test footgun). Closes ~6 MEDIUMs. |
| **13** | Sean's UI (still owed: a/b/c/d × i-v × preset/toml) | 1 slice if "preset switcher" path | Custom theme switching. Different feature work entirely. |

**TARS lean:** **13-a + 13-c bundled** (1.5 hrs total, closes 2 HIGHs +
several MEDIUMs in one tidy commit). Then either **13** (UI) or **13-b**
(finish migration) — pick by user-value vs. architectural cleanliness.

## What this review confirmed

- Slice 12-b's `SafeText` is correct, type-safe, and closes the
  recurrence-prevention HIGH from the last swarm.
- 12-a's stale-comment cleanup landed (zero `Slice N` markers in `*.rs`).
- The data-plane safety pipeline (boundary sanitisation + zsh `%`
  doubling + SafeText invariant) is now the project's most-reviewed
  surface.

## Confidence

**High** on five lanes. **Medium** on the documentation lane's
"5 surviving markers in init.zsh body" finding — needs a quick
verification grep before fix (the 12-a agent's verification grep was
`*.rs`-scoped, so this could be real or a misread of the body's
prose).
