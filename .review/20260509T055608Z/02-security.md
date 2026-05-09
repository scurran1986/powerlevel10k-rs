# Security Review — 20260509T055608Z

## Summary

The codebase has a solid security posture for a prompt tool: `#![forbid(unsafe_code)]` on all production crates, `Command::new("git")` with explicit `.arg()` (no shell interpolation), and a single-quote guard on `cmd_init` path injection. Three issues warrant attention: a TOCTOU race on FIFO creation exploitable on multi-user hosts, `is_fifo` following symlinks which defeats its purpose, and incomplete shell-escaping that blocks single quotes but passes control characters through.

## Findings

### [HIGH] FIFO directory TOCTOU race — no exclusive creation
**Location:** `crates/p10k-rs-shell/shells/zsh/init.zsh:61-64`
**Issue:** `_p10k_rs_start_daemon` creates `$XDG_RUNTIME_DIR/p10k-rs-$$` with `mkdir -p`, then conditionally runs `mkfifo` only if the path is not already a pipe (`[[ -p "$req" ]] || mkfifo`). On a multi-user host an attacker who knows the victim's PID can pre-create the directory and plant a symlink or their own FIFO at the `req`/`resp` paths between the `mkdir` and `mkfifo`. This lets the attacker read every gitstatusd request (leaking repo paths) or inject crafted responses. The `$$` is predictable; `/proc` exposes it.
**Suggested fix:** Use `mktemp -d` for unpredictable names, or create the directory with `install -d -m 0700` and verify ownership before proceeding. Set umask 077 before `mkfifo`. On Linux, prefer `$XDG_RUNTIME_DIR` (already uid-private on systemd hosts) and fail hard if it is world-writable.

### [HIGH] `is_fifo` follows symlinks — symlink-to-regular-file bypass
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:216-220`
**Issue:** `std::fs::metadata(p)` follows symlinks. An attacker who replaces a FIFO with a symlink to `/dev/null` or a regular file can cause `is_fifo` to return false (triggering fallback) or — if pointed at another FIFO — hijack the IPC channel. The check should use `symlink_metadata` (lstat) to inspect the link target without following it, rejecting symlinks outright.
**Suggested fix:** Replace `std::fs::metadata(p)` with `std::fs::symlink_metadata(p)` so symlinks are detected and rejected. Additionally, verify the file's owner matches the current UID.

### [HIGH] `cmd_init` single-quote check insufficient — control chars pass through
**Location:** `crates/p10k-rs/src/main.rs:168-172`
**Issue:** The guard rejects `'` in the exe path but allows newlines (`\n`), carriage returns (`\r`), NUL bytes, backticks, `$()`, and ANSI escape sequences. A path like `/tmp/p10k\n$(evil)/p10k-rs` would break out of the single-quoted literal context in some shell edge cases, or at minimum corrupt the init script. While single-quoted strings in POSIX shells don't interpret `$` or backticks, a newline inside a single-quoted string is legal but can confuse line-oriented parsers downstream, and NUL terminates C strings early.
**Suggested fix:** Allowlist the path to printable ASCII (or printable UTF-8) excluding shell metacharacters. Reject any byte < 0x20 or 0x7F. Example: `if exe_str.bytes().any(|b| b < 0x20 || b == 0x7F) { bail!(...) }`.

### [MEDIUM] Env-controlled FIFO paths trusted without validation
**Location:** `crates/p10k-rs/src/main.rs:194-201`
**Issue:** `_P10K_RS_GITSTATUSD_REQ` and `_P10K_RS_GITSTATUSD_RESP` are read from the environment with no validation beyond `is_fifo`. A hostile parent process can set these to attacker-controlled FIFOs, causing `p10k-rs prompt` to send cwd paths to the attacker and accept fabricated git state. The `is_fifo` check confirms the target is a FIFO but not that it is owned by the current user or lives under an expected directory.
**Suggested fix:** Verify the FIFO's owner UID matches `getuid()` and the parent directory is not world-writable. Consider checking that the path prefix matches `$XDG_RUNTIME_DIR/p10k-rs-*`.

### [MEDIUM] `install.sh` marker-line injection
**Location:** `install.sh:69-77`
**Issue:** The awk-based uninstall deletes the marker line and the line after it. If a user's `.zshrc` happens to contain the exact marker string (unlikely but possible via copy-paste from docs), the uninstaller will delete an unrelated line. More importantly, the `grep -qF "$RC_MARKER"` idempotence check means an attacker who can append to `.zshrc` can insert the marker to prevent future installs from adding the eval line, or craft a second marker to delete arbitrary adjacent lines during uninstall.
**Suggested fix:** Use a more unique marker (include a version hash or UUID). During uninstall, match both the marker and the eval line pattern together rather than blindly deleting "marker + next line."

### [MEDIUM] No `umask` before `mkfifo` — world-readable FIFOs possible
**Location:** `crates/p10k-rs-shell/shells/zsh/init.zsh:63-64`
**Issue:** `mkfifo` inherits the shell's current umask. If the user has a permissive umask (e.g., `0000` or `0002`), the FIFOs are created group- or world-readable/writable, allowing other users on the same host to read requests or inject responses.
**Suggested fix:** Set `umask 077` (or use `mkfifo -m 0600`) before creating the FIFOs. Restore the original umask afterward.

### [LOW] Hardcoded vendored binary path
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:234-236`
**Issue:** `locate_binary` contains a hardcoded absolute path `/home/seaburdz/github/powerlevel10k/gitstatus/usrbin/gitstatusd-linux-x86_64`. This is a dev-machine artifact that should not ship. On a multi-user system, if another user creates this path, the binary would execute an attacker-controlled daemon. The `is_file()` check follows symlinks.
**Suggested fix:** Remove the hardcoded path before release. Rely only on `$P10K_RS_GITSTATUSD_BIN` and `$PATH` lookup.

### [LOW] `rm -rf` on daemon stop without path validation
**Location:** `crates/p10k-rs-shell/shells/zsh/init.zsh:93-95`
**Issue:** `_p10k_rs_stop_daemon` runs `rm -rf -- "$_P10K_RS_FIFO_DIR"`. If this variable is corrupted or overwritten (e.g., by a misbehaving plugin setting it to `/`), the recursive delete could be destructive. The `[[ -d ... ]]` guard does not confirm the path is under the expected runtime directory.
**Suggested fix:** Validate that `$_P10K_RS_FIFO_DIR` matches the expected pattern (`*/p10k-rs-*`) before `rm -rf`.

### [INFO] No `unsafe` blocks — good
**Location:** workspace-wide
**Issue:** All nine production crates carry `#![forbid(unsafe_code)]`. The spike crate does not, but it is not compiled into the release binary. No `unsafe` blocks were found anywhere in the workspace.
**Suggested fix:** No action needed. Consider adding `#![forbid(unsafe_code)]` to spike crates as a hygiene measure.

### [INFO] Dependency audit could not run
**Location:** workspace-wide
**Issue:** `cargo audit` was unavailable in the review environment (`cargo` not on `PATH`). Dependency CVE scanning was not performed.
**Suggested fix:** Run `cargo audit` in CI on every commit. Ensure the Cargo.lock is committed.

## Things this review explicitly did NOT examine
- Rust idiom quality, ownership patterns, or trait design (reviewer 01)
- Performance characteristics or allocation patterns (reviewer 03)
- Code readability, naming, or comment quality (reviewer 04)
- Documentation completeness or ADR alignment (reviewer 05)
- Architecture decisions or test discipline (reviewer 06)
- The `spike-gitstatus` crate (not shipped in production binary)
- The `bench/fixtures/` directory (vendored third-party test data)

## Confidence
**Medium-high.** All production source files were reviewed. The FIFO TOCTOU and symlink-follow findings are concrete and exploitable on multi-user systems. The control-char escaping gap is lower-confidence — single-quoted shells are resilient, but defense-in-depth warrants the fix. Dependency audit gap lowers overall confidence.
