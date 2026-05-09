# Architecture & Other Review — 20260509T071500Z

## Summary

Slice 9's three ADR-0001 follow-ups are substantively closed: spike crate removed, gix stripped from the dep graph, GPL wiring landed via `THIRD-PARTY-LICENSES.md`. FIFO security hardening is solid. Two medium issues remain: `locate_binary` hardcodes `linux-x86_64` defeating multi-arch, and `bench/` scripts still embed a dev-machine absolute path. CHANGELOG has no slice-9 entry. Test coverage is adequate for shipped segments but zero for five placeholder crates and the `main.rs` glue.

## Findings

### [MEDIUM] locate_binary hardcodes x86_64 binary name
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:253`
**Issue:** `locate_binary()` probes `["gitstatusd", "gitstatusd-linux-x86_64"]` on PATH. On aarch64-linux or darwin hosts the arch-specific binary has a different suffix (`gitstatusd-linux-aarch64`, `gitstatusd-darwin-arm64`, etc.). ADR-0001 § Consequences lists four triples to support; only one is probed.
**Suggested fix:** Build the arch-specific name from `std::env::consts::{OS, ARCH}` at runtime, mapping to upstream's naming convention. Keep the generic `gitstatusd` as first probe.

### [MEDIUM] install.sh hardcodes dev-machine gitstatusd path
**Location:** `install.sh:126`
**Issue:** `GITSTATUSD_CANDIDATES` includes `$HOME/github/powerlevel10k/gitstatus/usrbin/gitstatusd-linux-x86_64`. This is the same class of dev-machine path that slice 9 deliberately removed from `locate_binary` (gitstatusd.rs:239 documents the removal). The install script re-introduces the pattern.
**Suggested fix:** Remove or gate behind `[[ -n "${P10K_RS_DEV:-}" ]]`. The two remaining candidates (`/opt/homebrew/bin`, `/usr/local/bin`) are legitimate system paths.

### [MEDIUM] bench scripts retain hardcoded /home/seaburdz path
**Location:** `bench/run_baseline.sh:54`, `bench/README.md:116`
**Issue:** `vendored=/home/seaburdz/github/powerlevel10k/...` won't resolve on any other machine. These are spike-era artifacts that should have been cleaned in slice 9 alongside the spike crate removal.
**Suggested fix:** Replace with a `$P10K_RS_GITSTATUSD_BIN` env-var probe or `$HOME/github/...` relative path with a guard.

### [MEDIUM] CHANGELOG missing slice 9 entry
**Location:** `CHANGELOG.md` (workspace-wide)
**Issue:** Every prior slice has a dedicated changelog section. Slice 9 (spike removal, gix strip, GPL wiring, FIFO security) has none. The `### Added` block at the bottom references "ADR-0001" but doesn't describe the slice-9 work.
**Suggested fix:** Add a `### Slice 9: triage — ADR-0001 follow-ups, FIFO security (de0072c)` section documenting: spike crate removal, gix dep strip, `THIRD-PARTY-LICENSES.md`, FIFO symlink/owner checks, `mktemp` unpredictable dirs, `safe_for_single_quote` path validation.

### [LOW] Vcs::is_fast returns false despite daemon being the default backend
**Location:** `crates/p10k-rs-segments/src/vcs.rs:28`
**Issue:** Comment says "Daemon backend in slice 5+ flips this to true" but slice 7 shipped the daemon as the default hot path. The method still returns `false`. While `is_fast` has no runtime effect today (no async dispatcher), it's stale documentation that will mislead the v0.2 daemon-mode implementer.
**Suggested fix:** Return `true`. The segment itself does no I/O; the git probe runs before segment render.

### [LOW] rust-toolchain.toml references gix in comment
**Location:** `rust-toolchain.toml:6`
**Issue:** Comment says "gix 0.66 transitives" as MSRV justification, but gix is no longer in the dep graph. Misleading for contributors checking MSRV rationale.
**Suggested fix:** Update comment to reference the actual MSRV driver (likely `home 0.5.12` or `clap`).

### [LOW] No tests for five placeholder crates
**Location:** `crates/p10k-rs-{config,wizard,ai,ipc,shell}/src/lib.rs`
**Issue:** These crates have zero `#[cfg(test)]` blocks. While they're stubs, `p10k-rs-shell` contains real logic (`FromStr`, `init_script` with `include_str!`) that is only smoke-tested indirectly through `install.sh`.
**Suggested fix:** Add at least one unit test per crate that exercises the public API surface (e.g., `Shell::from_str("zsh")` round-trips, `init_script(Shell::Zsh)` returns non-empty, etc.).

### [LOW] ADR-0001 follow-up "Update ARCHITECTURE.md § 2.4" still DEFERRED
**Location:** `docs/adr/0001-git-backend.md:105`
**Issue:** Two of four follow-ups are marked DONE. "Update ROADMAP.md" is documented as deferred (planning bundle outside repo) — acceptable. "Update ARCHITECTURE.md § 2.4" is also deferred with no tracking. Post-slice-9 is the natural time to close it.
**Suggested fix:** Either close it in the next maintenance slice or add a tracking comment with a target slice.

### [INFO] Dep graph is clean post-strip
**Location:** `Cargo.lock` (workspace-wide)
**Issue:** Zero `gix` references in `Cargo.lock`. The spike crate directory is gone. `workspace.members` in `Cargo.toml` lists only the eight production crates. The gix feature-strip follow-up is fully closed.

### [INFO] FIFO security is well-implemented
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:222-233`, `crates/p10k-rs-shell/shells/zsh/init.zsh:77-84,116`
**Issue:** `is_fifo` uses `symlink_metadata` (lstat, not stat) and checks owner UID. Shell side uses `mktemp -d` with random suffix, `chmod 0700`, and `umask 077 && mkfifo -m 0600`. Cleanup validates path pattern before `rm -rf`. This is sound defense-in-depth against co-tenant FIFO hijack.

## Things this review explicitly did NOT examine
- Rust idiomaticness, ownership patterns, error handling (lane 01)
- Unsafe blocks, command injection beyond FIFO (lane 02)
- Hot-path allocation counts, syscall budget (lane 03)
- Naming, comment quality, function length (lane 04)
- Module-level docs, README accuracy beyond ADR alignment (lane 05)

## Confidence
High. Read every `.rs` source file, all Cargo manifests, the init script, install script, ADR, changelog, and bench scaffolding. grep-verified spike removal and gix strip against the full tree. The three ADR-0001 follow-ups are verifiably closed; remaining findings are housekeeping.
