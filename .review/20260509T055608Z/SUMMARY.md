# Review Swarm Summary — 20260509T055608Z

**HEAD reviewed:** `a96c7a132b91ebe29e221c1a800378a03b9d9026` (slice 7)
**Six agents:** rust-principles, security, performance, readability, documentation, architecture
**Aggregate verdict:** No CRITICAL findings. **12 HIGH** findings cluster around three themes: *dev-machine artefacts shipping in production code*, *FIFO security on multi-user hosts*, and *ADR-0001 follow-ups still open three slices later*.

---

## Top 3 cross-cutting themes (multi-reviewer consensus)

### 1. Hardcoded `/home/seaburdz/...` path in production binary
Flagged by **architecture (HIGH)**, **rust-principles (LOW)**, **security (LOW)**, **readability (LOW)** — same line: `crates/p10k-rs-git/src/gitstatusd.rs:234-236`.

Will fail to resolve on any other machine. On a multi-user system where another user happens to have the path, it could pick up a stale or hostile binary.
**Fix cost:** 5 minutes. Drop the line; rely on `$P10K_RS_GITSTATUSD_BIN` and `$PATH`.

### 2. ADR-0001 follow-ups never closed
Flagged by **architecture (HIGH × 2)** and **documentation (HIGH × 2)**:
- `crates/spike-gitstatus` still a workspace member after ADR said "removed in follow-up commit."
- `gix.features = ["status"]` still pinned in workspace deps.
- GPL-3.0 distribution obligations (license file, source-offer, THIRD-PARTY-LICENSES) not wired.
- README/CHANGELOG describe a project that did not happen ("pre-alpha, workspace skeleton only").

**Fix cost:** ~30-60 min of polish. Mostly removals + doc edits.

### 3. FIFO security on multi-user hosts
Flagged by **security (HIGH × 3)**:
- `_p10k_rs_start_daemon` uses `$$`-based predictable directory path → TOCTOU race for FIFO planting.
- `is_fifo` uses `metadata` (follows symlinks) → symlink-redirect bypass.
- `cmd_init` blocks single quotes but lets through `\n`, `\r`, NUL, and other control chars.

**Fix cost:** ~1-2 hours. `mktemp -d`, `symlink_metadata`, control-char allowlist.

---

## Severity rollup

| Severity | rust | security | perf | read | docs | arch | **TOTAL** |
|----------|-----:|---------:|-----:|-----:|-----:|-----:|----------:|
| CRITICAL |    0 |        0 |    0 |    0 |    0 |    0 |     **0** |
| HIGH     |    1 |        3 |    3 |    0 |    3 |    2 |    **12** |
| MEDIUM   |    4 |        3 |    4 |    3 |    3 |    4 |    **21** |
| LOW      |    2 |        2 |    3 |    4 |    1 |    2 |    **14** |
| INFO     |    1 |        2 |    1 |    1 |    1 |    0 |     **6** |

53 total findings. Quality is high for an MVP; everything HIGH is a known kind of problem with a known fix.

---

## All HIGH findings — one-line action items

| # | Severity / area | Finding | Action |
|---|---|---|---|
| 1 | HIGH (rust) | `Backend::status` returns `Option` — conflates "not a repo" with errors | Convert to `Result<Option<GitState>, BackendError>` |
| 2 | HIGH (security) | FIFO TOCTOU via predictable `p10k-rs-$$` dir | `mktemp -d`, `umask 077`, `mkfifo -m 0600` |
| 3 | HIGH (security) | `is_fifo` follows symlinks | Use `symlink_metadata` |
| 4 | HIGH (security) | `cmd_init` quote check passes control chars | Reject bytes < 0x20 and 0x7F |
| 5 | HIGH (perf) | `tracing-subscriber` init on silent path | Lazy-init only when `RUST_LOG` set |
| 6 | HIGH (perf) | Process-spawn-per-prompt is the real latency ceiling | Track in CI; `lto = "fat"`; consider hand-rolled parser for `prompt` subcommand |
| 7 | HIGH (perf) | `wrap_for_shell` two-pass scan + per-char boundary check | Single-pass via `memchr`; pre-size with escape count |
| 8 | HIGH (docs) | CHANGELOG.md frozen at bootstrap; 7 slices unmentioned | Backfill `[Unreleased]` per slice |
| 9 | HIGH (docs) | README.md says "pre-alpha skeleton only"; planning links 404 | Rewrite status; copy planning into `docs/` or replace with ADR-0001 link |
| 10 | HIGH (docs) | ADR-0001 follow-ups still open (spike removal, gix feature strip) | Close or annotate each |
| 11 | HIGH (arch) | `crates/spike-gitstatus` still in workspace | `git rm -r crates/spike-gitstatus`; drop from `Cargo.toml` members |
| 12 | HIGH (arch) | Hardcoded `/home/seaburdz/...` path in `gitstatusd.rs:234-236` | Delete the line |

---

## Suggested slice 9: "Triage"

Bundle these into one cleanup slice. Order by cost:

1. **Quick paper cuts (≤ 10 min total):**
   - Delete hardcoded dev path (HIGH #12)
   - Strip phantom `p10k-rs-git` dep from `p10k-rs-segments`
   - Remove unused `tracing` from `p10k-rs-core` deps
   - Lazy-init tracing in `main.rs` (HIGH #5)

2. **ADR-0001 follow-ups (~30 min):**
   - Remove `crates/spike-gitstatus` from workspace (HIGH #11)
   - Strip `gix.features = ["status"]` from workspace pin
   - Add `THIRD-PARTY-LICENSES.md` (GPL-3.0 attribution + source-offer)

3. **Doc refresh (~30 min):**
   - README status + planning link fix (HIGH #9)
   - CHANGELOG `[Unreleased]` backfill for slices 1-8 (HIGH #8)
   - ADR-0001 follow-up status updates (HIGH #10)
   - `docs/adr/README.md` index title alignment

4. **Security fixes (~1 hour):**
   - `mktemp -d` for FIFO dir (HIGH #2)
   - `umask 077` before `mkfifo` + `mkfifo -m 0600` (HIGH #2)
   - `symlink_metadata` in `is_fifo` (HIGH #3)
   - Control-char allowlist in `cmd_init` (HIGH #4)

5. **Defer to later slices** (each is its own design discussion):
   - `Backend::status` → `Result` (HIGH #1) — affects API; do alongside the next consumer
   - Process-spawn ceiling instrumentation (HIGH #6) — bench-infra work
   - `wrap_for_shell` single-pass with `memchr` (HIGH #7) — micro-opt; measure first

---

## Cross-cutting MEDIUMs worth flagging

- **Duplicate `Shell`/`ColorMode`/`HostKind` enums** across two crates each (rust + readability). One canonical owner per type, others depend or re-export.
- **`Dir::render` reads `$HOME` directly** instead of via `EnvSnapshot` (rust + perf). Architecture violation.
- **Three near-identical `RenderCtx` test helpers** copy-pasted across segment files (readability + future-fragility). Extract to `testutil`.
- **Slice-number comments stale** workspace-wide (readability + docs). Rip out forecasted slice numbers; keep concrete language.
- **Raw ANSI escapes bypass `style.rs`** (readability). The module doc is currently a lie.

---

## What this review explicitly didn't catch

- No actual `cargo audit` — security agent flagged this; not run because cargo wasn't on the agents' PATH.
- No bench numbers — performance findings are mechanically certain from code reading; exact ms wins would need a microbench.
- No multi-shell concurrency test — the FIFO TOCTOU finding is from code reading, not an actual race demonstration.

---

## Confidence

**High** that the HIGH findings are real. **Medium-high** that the suggested slice 9 plan covers the right priorities — Sean owns final triage. The review took 6 parallel agents ~2 minutes wall-clock; methodology validated for ongoing use per `.review/REVIEW-SWARM.md`.
