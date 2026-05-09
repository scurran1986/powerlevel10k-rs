# Architecture & Other Review — 20260509T055608Z

## Summary

The codebase broadly matches ADR-0001's daemon-client design: `p10k-rs-git`
implements a FIFO-based `gitstatusd` client with poll-driven timeouts.
However, ADR-0001's stated follow-ups (spike removal, GPL wiring, README
updates) remain unaddressed after 3 slices. A hardcoded developer-local
path in production code is the most immediately broken item. The
dependency graph is clean (no cycles, sane direction) but carries one
phantom edge and a workspace member ADR-0001 explicitly retired.

## Findings

### [HIGH] spike-gitstatus remains in workspace after ADR-0001 declared it discharged
**Location:** `Cargo.toml:26`, `docs/adr/0001-git-backend.md:61`
**Issue:** ADR-0001 states "The spike-gitstatus crate has discharged its
purpose. It will be removed from the workspace in a follow-up commit."
Three slices later (slices 5-7), it is still a workspace member,
compiled by CI, and pulls `gix`, `rayon`, `rustix`, and `bincode` into
the dependency graph. This inflates `cargo test --workspace` and
`cargo clippy --workspace` time and muddles the crate count (10 members
vs the 9 production crates the README implies).
**Suggested fix:** Remove `"crates/spike-gitstatus"` from `Cargo.toml`
`members`. The crate stays in git history per ADR-0001. The bench
harness (`bench/`) is independent and can remain.

### [HIGH] Hardcoded developer-local path in production binary
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:234-236`
**Issue:** `locate_binary()` probes
`/home/seaburdz/github/powerlevel10k/gitstatus/usrbin/gitstatusd-linux-x86_64`
as its second candidate. This is a dev-machine path baked into the
shipped binary — it will never resolve on any other machine, and if it
*does* exist on a multi-user system it could pick up a stale or
malicious binary. The spike has the same path
(`gitstatusd_baseline.rs:32`) but that crate doesn't ship.
**Suggested fix:** Remove the hardcoded vendored path. Replace with a
`$P10K_RS_ROOT`-relative probe (e.g.,
`<binary_dir>/../lib/gitstatusd-<arch>`) for the bundled binary in
release artifacts.

### [MEDIUM] GPL-3.0 obligations not wired into release process
**Location:** `docs/adr/0001-git-backend.md:106`, `README.md` (absent section)
**Issue:** ADR-0001 Follow-ups lists three concrete obligations:
(1) THIRD-PARTY-LICENSES section in README, (2) GPL license file
alongside bundled binary, (3) source-offer pointer to upstream tag.
None are implemented. The ADR says "don't ship v0.1 without them." No
CI check enforces their presence in release artifacts.
**Suggested fix:** Add a `THIRD-PARTY-LICENSES.md` to repo root. Add a
release-workflow step that copies `gitstatusd`'s GPL-3.0 text into the
tarball. Track with an issue so it doesn't slip further.

### [MEDIUM] No end-to-end test for gitstatusd FIFO backend
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs` (entire module)
**Issue:** The `Gitstatusd` backend has unit tests for `parse_response`
but zero integration tests that exercise the FIFO open-write-poll-read
path. The `read_until_with_deadline` function is tested only implicitly.
A real integration test (create temp FIFOs, spawn a mock daemon,
round-trip a request) would catch regressions in the poll loop and
deadline logic.
**Suggested fix:** Add an integration test in `crates/p10k-rs-git/tests/`
that creates a pair of FIFOs with `mkfifo`, spawns a thread writing a
canned response, and asserts `Gitstatusd::status()` returns the
expected `GitState`.

### [MEDIUM] Phantom dependency: p10k-rs-segments depends on p10k-rs-git but never imports it
**Location:** `crates/p10k-rs-segments/Cargo.toml:19`
**Issue:** `p10k-rs-segments` lists `p10k-rs-git` as a dependency, but
`grep -rn "p10k_rs_git" crates/p10k-rs-segments/src/` returns nothing.
The `vcs` segment reads `GitState` from `p10k-rs-core`, not from the
git crate. This phantom edge couples the segments crate to the git
crate's compile time for no reason.
**Suggested fix:** Remove `p10k-rs-git = { workspace = true }` from
`crates/p10k-rs-segments/Cargo.toml`.

### [MEDIUM] Stale README describes p10k-rs-git as "gix + rustix hot loop"
**Location:** `README.md:32`
**Issue:** The workspace layout table says `p10k-rs-git` is "gitstatus
replacement (gix + rustix hot loop)." Post-ADR-0001, it is a daemon
client. The description matches the pre-pivot architecture. Similarly,
the `p10k-rs-git/Cargo.toml:3` description says "gix for index/refs,
rustix for the hot walker."
**Suggested fix:** Update both descriptions to "gitstatusd daemon
client (ADR-0001)" or similar.

### [MEDIUM] p10k-rs-core claims I/O-free but depends on tracing and anstyle
**Location:** `crates/p10k-rs-core/Cargo.toml:18-20`, `crates/p10k-rs-core/src/lib.rs:1`
**Issue:** The module doc says "intentionally I/O-free." `tracing` can
perform I/O via its subscriber, and `anstyle` itself is pure, but the
contract is worth enforcing. Currently `tracing` is listed but grep
shows no `tracing::` usage in the crate — it is a phantom dependency
today. If a future contributor adds `tracing::info!()` calls, the
I/O-free claim breaks silently.
**Suggested fix:** Remove `tracing` from `p10k-rs-core` dependencies if
unused. If needed later, gate behind a feature flag with a doc comment
explaining the I/O boundary.

### [LOW] Stale TODO references pre-pivot architecture
**Location:** `crates/spike-gitstatus/src/hybrid.rs:267`, `crates/p10k-rs-core/src/lib.rs:281`
**Issue:** TODOs reference "the real index-entry comparison" and
replacing `Config` with a `p10k-rs-config` re-export. The first is
spike-only (moot if spike is removed). The second is a legitimate
placeholder that should be tracked.
**Suggested fix:** The core TODO is valid — convert to a tracked issue.
The spike TODOs disappear with the spike removal.

### [LOW] Bench fixtures have no CI story
**Location:** `.gitignore` (line: `bench/fixtures/repos/`), `bench/fixtures/repos/`
**Issue:** The linux kernel fixture is 8 GB and gitignored. CI runs
`cargo test --workspace` but has no bench job. The `fetch_fixtures.sh`
script exists but is never called from CI. This is fine for MVP but
means performance regressions are invisible until manual bench runs.
**Suggested fix:** INFO-level for now. Post-v0.1, add a scheduled CI
job that fetches a small fixture (ripgrep, not linux) and runs
`cargo bench` with a threshold check.

## Things this review explicitly did NOT examine
- Rust idiom quality (agent 01's lane)
- Security of FIFO handling and env var trust (agent 02's lane)
- Hot-path allocation and syscall counts (agent 03's lane)
- Naming and readability (agent 04's lane)
- Doc completeness and ADR prose quality (agent 05's lane)

## Confidence
**High.** All findings cite specific file:line locations, dependency
edges were verified via grep, and the ADR-0001 follow-up checklist is
explicit about what's missing. The only uncertainty is whether
`p10k-rs-segments`'s dependency on `p10k-rs-git` is intentional
forward-planning vs. accidental — but even if intentional, unused deps
should not be declared.
