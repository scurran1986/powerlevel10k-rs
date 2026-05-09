# Security Review — 20260509T071500Z

## Summary

Slice 9's FIFO hardening is well-executed: `mktemp -d`, `umask 077`, `mkfifo -m 0600`, `symlink_metadata` (lstat), and UID ownership checks are all correct and materially close the previous FIFO hijack surface. The remaining findings are MEDIUM-severity TOCTOU gaps inherent in check-then-open patterns and a predictable tempfile name in the instant-prompt dump path. No hardcoded secrets. All crates carry `#![forbid(unsafe_code)]`. `deny.toml` is strict and well-configured. `cargo audit` could not be run (no toolchain in review sandbox); the deny.toml advisory config is sound.

## Findings

### [MEDIUM] TOCTOU between `is_fifo()` and `OpenOptions::open()` in `Gitstatusd::status()`
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:71-76` (check) vs `:95,113` (open)
**Issue:** `is_fifo()` validates symlink-safety and UID ownership via `symlink_metadata`, but the actual `open()` calls on lines 95 and 113 happen later. Between the check and the open, an attacker with write access to the parent directory could swap the FIFO for a symlink (or replace it with an attacker-owned FIFO). The `mktemp -d` with `0700` permissions on the parent directory makes this unexploitable in practice — only the owning UID can modify the directory — so the real-world risk is low. If `XDG_RUNTIME_DIR` or `TMPDIR` were ever set to a shared sticky-bit directory without the `mktemp` wrapper, this would become exploitable.
**Suggested fix:** Open first with `O_NOFOLLOW | O_NONBLOCK`, then `fstat` the resulting fd to verify FIFO type and UID. This collapses the TOCTOU window to zero. Alternatively, document the safety invariant that the parent directory must be mode 0700 and owned by the user.

### [MEDIUM] Instant-prompt dump uses predictable `.tmp` extension
**Location:** `crates/p10k-rs/src/main.rs:209`
**Issue:** `path.with_extension("tmp")` produces a predictable tempfile name (e.g. `dump-sean.tmp`). On a multi-user system where `XDG_CACHE_HOME` resolves to a shared or world-writable location, an attacker could pre-create a symlink at this path to redirect the write. The `rename` on line 211 would then atomically place the file at the attacker's chosen destination. The default cache path (`~/.cache/p10k-rs/`) is user-owned, limiting real-world risk.
**Suggested fix:** Use `tempfile::NamedTempFile` or `mkstemp`-equivalent in the same parent directory, then `persist()` (rename). This gives an unpredictable name and `O_EXCL` semantics.

### [MEDIUM] Dump directory created with default permissions
**Location:** `crates/p10k-rs/src/main.rs:204`
**Issue:** `create_dir_all(parent)` creates the cache directory with the process umask (often 022), making it world-readable. The dump file contains the rendered PROMPT string, which includes the current working directory — a minor information leak on shared hosts.
**Suggested fix:** After `create_dir_all`, explicitly `std::fs::set_permissions(parent, Permissions::from_mode(0o700))` or use `DirBuilder::new().mode(0o700).recursive(true).create(parent)`.

### [MEDIUM] `locate_binary()` trusts `$PATH` for `gitstatusd` without signature or hash check
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:244-262`
**Issue:** The daemon binary is discovered by walking `$PATH`. Any binary named `gitstatusd` or `gitstatusd-linux-x86_64` in any `$PATH` directory will be executed by the shell init script with the user's full privileges. If an attacker can write to any early `$PATH` directory, they achieve arbitrary code execution. This is standard Unix `$PATH` trust and not unique to this project, but the install script's symlink approach (line 133 of `install.sh`) partially mitigates by placing a known-good symlink early in `$PATH`.
**Suggested fix:** Document that users should verify `$PATH` ordering. Optionally, store a SHA-256 hash of the known-good binary at install time and verify it in `locate_binary()`.

### [LOW] `install.sh` uses `ln -sfn` to symlink gitstatusd without verifying target integrity
**Location:** `install.sh:133`
**Issue:** The install script symlinks whichever first-found candidate from `GITSTATUSD_CANDIDATES` to `~/.cargo/bin/gitstatusd` without verifying the file's provenance. The candidate list includes a user-local path (`$HOME/github/powerlevel10k/...`), which is fine for single-user dev, but could be manipulated on a shared filesystem.
**Suggested fix:** Low priority. Verify the candidate is owned by the current user and not world-writable before symlinking.

### [LOW] `_p10k_rs_stop_daemon` pattern-match for rm -rf is permissive
**Location:** `crates/p10k-rs-shell/shells/zsh/init.zsh:116`
**Issue:** The guard `"$_P10K_RS_FIFO_DIR" == */p10k-rs.*` is a reasonable defense against corrupted variables, but the glob `*/p10k-rs.*` would match paths like `/tmp/evil/p10k-rs.XXXXXXXX/../../important`. In practice the `mktemp -d` output won't contain `..` segments, and `-d` (is-a-directory) provides a second gate.
**Suggested fix:** Add a check that the path contains no `..` component: `[[ "$_P10K_RS_FIFO_DIR" != *..* ]]`.

### [INFO] `#![forbid(unsafe_code)]` is set on all crates
**Location:** Workspace-wide (all 10 crate roots)
**Issue:** This is a positive finding. Every crate in the workspace carries `#![forbid(unsafe_code)]`, which is the strongest compile-time guarantee against memory-safety issues in first-party code.

### [INFO] No hardcoded secrets detected
**Location:** Workspace-wide
**Issue:** Grep for `api_key`, `password`, `secret`, `token`, and `credential` across all source, TOML, and shell files found no hardcoded secrets. Environment variable names used (`_P10K_RS_GITSTATUSD_REQ`, etc.) are IPC paths, not secrets.

## Things this review explicitly did NOT examine
- Performance characteristics (lane 03)
- Code style, naming, readability (lanes 01, 04)
- Documentation accuracy (lane 05)
- Architecture conformance to ADR-0001 (lane 06)
- `bench/fixtures/` vendored code (ripgrep, linux kernel sources)
- Compiled artifacts in `target/`
- `cargo audit` results (toolchain not available in review sandbox; `deny.toml` advisory policy is correctly configured with `yanked = "deny"` and empty ignore list)

## Confidence
**High.** The security surface is small (CLI tool, no network listeners, no auth, no user-facing input beyond shell env vars and filesystem paths). The FIFO hardening in slice 9 is well-designed. The remaining TOCTOU findings are defense-in-depth improvements, not exploitable under the current `mktemp -d` + `0700` parent directory setup.
