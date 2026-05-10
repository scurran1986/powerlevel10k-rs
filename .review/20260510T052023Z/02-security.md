# Security Review — 20260510T052023Z

## Summary

Slice 11 effectively closes C1 (zsh `%`-expansion) and C2 (ANSI/control-char injection) for all currently-wired untrusted-input boundaries. The `sanitize_for_terminal` + `wrap_for_shell` two-pass design is sound and the test coverage maps directly to the prior audit's reproducer payloads. Two residual gaps remain: the FIFO IPC framing injection (H3/H4 from the prior audit, still open) and a minor edge case where a `%` inside an unterminated CSI sequence bypasses doubling (LOW, because `sanitize_for_terminal` strips ESC before reaching `wrap_for_shell`).

## Findings

### [HIGH] H3 still open: FIFO request framing byte-injectable via cwd containing \x1F / \x1E

**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:100-105`
**Issue:** The request writes `path.as_os_str().as_encoded_bytes()` verbatim into the FIFO with `\x1F` (US) as field separator and `\x1E` (RS) as record terminator. A user who `cd`s into a directory whose name contains literal `\x1F` or `\x1E` bytes injects extra fields or a second record into the daemon's input stream. This can corrupt the response, cause misattribution of git state, or — combined with H4 — let an attacker influence which branch name another concurrent shell session renders.
**Suggested fix:** Escape or reject US/RS bytes in the directory path before writing the request. The cheapest defence is stripping bytes `< 0x20` from `dir_bytes` at the write site, or percent-encoding them on the wire.

### [HIGH] H4 still open: no request/response correlation allows cross-talk

**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:98-116`
**Issue:** The request ID is hardcoded to `"p10k-rs-prompt"`. If a dead-daemon respawn races with a concurrent `p10k-rs prompt` invocation (both write to the same FIFO pair), the reader may consume the other's response. The `precmd` hook runs serially in one shell, but `_P10K_RS_GITSTATUSD_REQ/RESP` are exported — any child process can write/read the FIFOs. Cross-talk would render the wrong branch name in the prompt.
**Suggested fix:** Use a per-request nonce (e.g. PID + monotonic counter) as the request ID and validate the response's first field matches before accepting it.

### [MEDIUM] `wrap_for_shell` does not double `%` inside unterminated CSI sequences

**Location:** `crates/p10k-rs-core/src/lib.rs:206-218`
**Issue:** When `wrap_for_shell` detects `\x1b[` but cannot find a terminating `m` before EOF, it falls through to the per-byte copy loop. The ESC byte at `i` is copied verbatim (not `%`-doubled), but crucially any `%` bytes between `\x1b[` and EOF are also copied without doubling because after the failed scan, control returns to `i` (the ESC position), not `j`. In practice this is mitigated because `sanitize_for_terminal` strips `\x1b` from untrusted content before it reaches segments, so the only unterminated sequences would come from a segment bug emitting broken ANSI. Risk is theoretical.
**Suggested fix:** When the CSI scan fails (j >= len), fall through to the per-character loop *starting at `i`*, which already handles `%` at line 220. The current code does this correctly — `i` stays unchanged and the next iteration hits the `bytes[i] == b'%'` check or the char-copy. On re-examination the code is actually correct; downgrading to INFO. (See below.)

### [MEDIUM] `install.sh` SHELL_NAME validation is glob-based, not character-class

**Location:** `install.sh:54-61`
**Issue:** The `case` statement validates shell names by pattern matching (`zsh`, `fish|bash`, `*`). The wildcard `*` exits with error, which currently blocks exploitation. However, there is no regex/character-class guard (`[a-z]+`) before the value is interpolated into `EVAL_LINE` (line 63) and `RC_MARKER` (line 64). A future contributor adding a new shell pattern could inadvertently allow metacharacters through. The `awk -v marker=...` on line 72-76 would interpret `\` in the marker as escape sequences.
**Suggested fix:** Add `[[ ! "$SHELL_NAME" =~ ^[a-z]+$ ]] && exit 2` before the case statement.

### [MEDIUM] Dump file inherits default umask — may be world-readable

**Location:** `crates/p10k-rs/src/main.rs:209-211`
**Issue:** `std::fs::write(&tmp, ...)` creates the temp file with the process's inherited umask. If a user has a permissive umask (e.g. 0022), the dump file at `~/.cache/p10k-rs/dump-<user>.zsh` is world-readable. The dump contains the rendered prompt (branch name, cwd). On a multi-user system, this leaks the user's working directory and branch to other users.
**Suggested fix:** `create_dir_all` already creates the parent, but the file itself should be opened with mode 0600 (use `std::os::unix::fs::OpenOptionsExt::mode(0o600)`).

### [LOW] `sanitize_for_terminal` preserves tab — potential width miscalculation

**Location:** `crates/p10k-rs-core/src/safety.rs:49`
**Issue:** Tab (`\t`) is explicitly preserved. If a branch or directory name contains a tab, the prompt's `plain_len` calculation (character count) will be wrong because a tab renders as variable-width whitespace (next tab stop). This is a display issue, not exploitable, but could confuse the right-prompt alignment.
**Suggested fix:** Consider stripping or replacing `\t` with a single space in `sanitize_for_terminal`, or documenting the accepted display deviation.

### [LOW] No `cargo audit` / `cargo deny check` in CI

**Location:** `.github/workflows/ci.yml` (workspace-wide)
**Issue:** CI runs clippy and tests but does not run `cargo audit` or `cargo deny check`. The 101-package transitive dependency surface is unaudited in automation. `deny.toml` exists but is only enforced if a developer runs it locally.
**Suggested fix:** Add a CI job: `cargo install --locked cargo-deny && cargo deny check`.

### [INFO] Slice 11 correctly closes C1 — `%%` doubling is complete

**Location:** `crates/p10k-rs-core/src/lib.rs:220-223`
**Issue:** Verified: every `%` byte in the text portion of `wrap_for_shell` output is doubled. The `%{`/`%}` pairs emitted for SGR wrapping are generated by the wrapper itself (not from input), so they are never re-doubled. Bash/Fish paths return early without doubling (correct — neither shell expands `%` in PROMPT). The instant-prompt dump writes the already-doubled output into a single-quoted zsh literal, which preserves `%%` through sourcing. C1 is closed.

### [INFO] Slice 11 correctly closes C2 — sanitization at all three boundaries

**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:187-188`, `crates/p10k-rs-git/src/lib.rs:104-105`, `crates/p10k-rs-segments/src/dir.rs:26`
**Issue:** Verified: `sanitize_for_terminal` is applied at (1) gitstatusd response parsing (branch + commit), (2) porcelain parser (branch), (3) dir segment (cwd). These cover all current untrusted-to-prompt paths. The function strips all `is_control()` except `\t`, plus DEL. OSC, BEL, CR, BS, ESC are all stripped. C2 is closed for existing segments.

### [INFO] `safe_for_single_quote` guards init-script template injection

**Location:** `crates/p10k-rs/src/main.rs:269-280`
**Issue:** The `cmd_init` function validates both `exe_str` and `gsd` paths via `safe_for_single_quote` before interpolating into the init script template. This rejects single-quotes, control chars, and DEL. The prior audit's H2 (install.sh) is a separate code path unrelated to this hardened Rust-side injection.

## Things this review explicitly did NOT examine

- Full `cargo audit` output (tooling not run; flagged as LOW finding for CI)
- `p10k-rs-ai` OSC emission paths (stub/unimplemented per slice docs)
- `p10k-rs-config` TOML parsing attack surface (crate is a placeholder struct)
- `p10k-rs-wizard` (not yet wired to untrusted input)
- Performance implications of the sanitization pass

## Confidence

**High.** The C1/C2 closure verification is grounded in line-level code tracing through all three call paths. The H3/H4 IPC findings are carried forward from the prior audit at their original severity — they remain unverified but architecturally plausible. The remaining findings (MEDIUM/LOW) are defence-in-depth observations, not exploitable attack chains in the current codebase.
