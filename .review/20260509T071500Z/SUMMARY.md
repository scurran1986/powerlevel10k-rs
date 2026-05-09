# Review Swarm Summary — slice 9 (HEAD `de0072c`)

**Aggregate:** No CRITICAL. **2 HIGH** (both documentation drift). ~14 MEDIUM, ~10 LOW.
Slice 9's HIGH findings from the previous swarm verified closed: spike removed, gix-feature stripped, GPL wiring landed, FIFO hardening sound (lstat + UID check + mktemp + 0700/0600 perms), lazy `init_tracing` win.

## All HIGH

| # | Reviewer | Finding | Action |
|---|---|---|---|
| 1 | docs | README + CONTRIBUTING claim MSRV 1.84, code pins 1.88 — fresh contributor fails to build | Update both to 1.88 |
| 2 | docs+arch | CHANGELOG has no slice-9 entry (spike removal, FIFO hardening, THIRD-PARTY-LICENSES, ADR closure all unrecorded) | Backfill |

## Cross-cutting MEDIUM themes

- **Type duplication** (`Shell`, `Config`, `ColorMode`) across crates — unchanged from slice-7 review. Drift risk grows.
- **`Dir` bypasses `EnvSnapshot`** for `$HOME` — architecture violation, untestable without unsafe env mutation.
- **`Backend::status` returns owned `GitState`** — should return `Result<Option<&GitState>>` with cached borrow on the hot path; affects allocations.
- **19 stale slice-number comments** across 7 files — actively misleading.
- **Triplicated `RenderCtx` test builders** in 3+ segment files — fragile to `RenderCtx` changes.
- **`cmd_prompt` does 6 things in 46 lines** — split.
- **`is_fifo`/`open` TOCTOU window** — small, unexploitable under 0700 parent dir, but the right fix is `openat(O_NOFOLLOW)` + post-open `fstat`.
- **Predictable `.tmp` filename** in instant-prompt dump write — a co-tenant could pre-create the path.
- **`locate_binary` hardcodes `x86_64`** — breaks aarch64 / Apple Silicon. Multi-arch was an ADR-0001 expectation.
- **`install.sh:126` re-introduces the dev-machine path** that slice 9 removed from the binary. Different blast radius (install-time, Sean-machine) but conceptually undoes the fix.
- **`THIRD-PARTY-LICENSES.md` asserts a v1.5.4 pin nothing enforces** — install.sh doesn't check the binary version.
- **Process-spawn ceiling** (perf #02) — unchanged from slice-7, still real.
- **`wrap_for_shell` two-pass scan** (perf #03) — unchanged.
- **Bench scripts have `/home/seaburdz/...` absolute paths** — same class of issue.

## Per-area summary
- **Rust:** 0H 3M 2L — strong fundamentals, type-duplication overdue.
- **Security:** 0H 3M 2L — slice 9 hardening confirmed correct.
- **Performance:** 0H 6M 2L — slice 9 lazy-tracing win confirmed; old HIGHs persist.
- **Readability:** 0H 3M 4L — stale slice comments are the noise floor.
- **Documentation:** 2H 3M 1L — MSRV drift + CHANGELOG drift dominate.
- **Architecture:** 0H 4M 4L — ADR-0001 follow-ups verified closed; install.sh + multi-arch are next.

## Suggested slice 11: doc + multi-arch hygiene

Cheap, visible:
1. **MSRV doc fix** (5 min) — README + CONTRIBUTING → 1.88.
2. **CHANGELOG slice-9 + slice-10 entries** (10 min).
3. **Strip stale slice-number comments** workspace-wide (30 min) — replace forecasts with facts.
4. **install.sh: drop the dev-machine `$HOME/github/powerlevel10k/...` candidate** (5 min) — keep only generic candidates (`/opt/homebrew`, `/usr/local`, `$PATH`).
5. **`locate_binary` multi-arch** (15 min) — probe `gitstatusd-linux-aarch64` etc. via `uname -m`.
6. **Extract `RenderCtx` test builder** to `p10k-rs-core::testutil` (30 min) — paid back across 4+ segments.

Total: ~95 min. Defer the bigger ones (type-dedup, `Backend` Result, perf micro-opts) — they need a coherent design pass.

## What this review confirmed
- Slice 9's hardening is correct (FIFO perms, mktemp, lstat+UID, single-quote escape).
- ADR-0001 § Follow-ups all closed except the planning-bundle DEFERRED.
- Lazy `init_tracing` win is real.

## Confidence
High on all 6 lanes. The 2 HIGHs are concrete drift items with mechanical fixes.
