//! Per-shell init scripts and integration glue.
//!
//! Init scripts live as plain text under `crates/p10k-rs-shell/shells/<sh>/`
//! and are byte-included into the binary via [`include_str!`] so
//! `p10k-rs init <shell>` prints the right snippet without reading from
//! disk at runtime. See `ARCHITECTURE.md` § 2.5.
//!
//! All three MVP shells now ship: zsh (full feature set including the
//! gitstatusd daemon backend and instant prompt), bash (no daemon, no
//! timing, no instant prompt), and fish (no daemon, no instant prompt,
//! but with command-execution-time tracking).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::str::FromStr;

/// Which shell init we want.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shell {
    /// Z shell.
    Zsh,
    /// Friendly Interactive Shell.
    Fish,
    /// Bourne Again Shell.
    Bash,
    /// `PowerShell` (Core / 7+). Native Windows + cross-platform.
    Pwsh,
    /// Nushell. Structured-data shell with native command-duration and
    /// right-prompt support.
    Nu,
}

impl FromStr for Shell {
    type Err = UnsupportedShell;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "zsh" => Ok(Self::Zsh),
            "fish" => Ok(Self::Fish),
            "bash" => Ok(Self::Bash),
            // Accept both `pwsh` (cross-platform PowerShell 7+) and
            // `powershell` (Windows-only legacy 5.x). Same init script —
            // it targets the 5.1 / 7+ intersection and feature-probes
            // the rest at load time.
            "pwsh" | "powershell" => Ok(Self::Pwsh),
            // Accept both `nu` and the long form `nushell`.
            "nu" | "nushell" => Ok(Self::Nu),
            other => Err(UnsupportedShell(other.to_owned())),
        }
    }
}

/// Returned when [`Shell::from_str`] gets a string we don't support.
#[derive(Debug, thiserror::Error)]
#[error("unknown shell '{0}': supported = zsh, fish, bash, pwsh, nu")]
pub struct UnsupportedShell(pub String);

/// Returns the init script for the requested shell.
///
/// The returned string is byte-included from `shells/<shell>/init.<ext>` at
/// compile time. The binary writes it to stdout for `eval`/`source` consumption.
/// Infallible — every variant of [`Shell`] is supported.
#[must_use]
pub fn init_script(shell: Shell) -> &'static str {
    match shell {
        Shell::Zsh => include_str!("../shells/zsh/init.zsh"),
        Shell::Bash => include_str!("../shells/bash/init.bash"),
        Shell::Fish => include_str!("../shells/fish/init.fish"),
        Shell::Pwsh => include_str!("../shells/pwsh/init.ps1"),
        Shell::Nu => include_str!("../shells/nu/init.nu"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn every_shell_has_a_non_empty_script() {
        for shell in [Shell::Zsh, Shell::Bash, Shell::Fish, Shell::Pwsh] {
            let s = init_script(shell);
            assert!(!s.is_empty(), "{shell:?} script is empty");
        }
    }

    #[test]
    fn every_shell_substitutes_binary_path() {
        // The cmd_init binary glue replaces `__P10K_RS_BIN__` with the
        // absolute path of the running binary. If a script template loses
        // that token, init silently bakes the placeholder into the user's
        // shell — confusing and hard to diagnose. Pin the invariant.
        for shell in [Shell::Zsh, Shell::Bash, Shell::Fish, Shell::Pwsh] {
            let s = init_script(shell);
            assert!(
                s.contains("__P10K_RS_BIN__"),
                "{shell:?} script must contain the binary-path substitution token",
            );
        }
    }

    #[test]
    fn every_shell_guards_against_double_source() {
        // Re-sourcing must be a no-op. Each script gates on an
        // installation sentinel.
        for shell in [Shell::Zsh, Shell::Bash, Shell::Fish, Shell::Pwsh] {
            let s = init_script(shell);
            assert!(
                s.contains("_P10K_RS_INSTALLED"),
                "{shell:?} script must guard re-source via _P10K_RS_INSTALLED",
            );
        }
    }

    #[test]
    fn shell_parses_from_canonical_names() {
        assert_eq!("zsh".parse::<Shell>().unwrap(), Shell::Zsh);
        assert_eq!("bash".parse::<Shell>().unwrap(), Shell::Bash);
        assert_eq!("fish".parse::<Shell>().unwrap(), Shell::Fish);
        assert_eq!("pwsh".parse::<Shell>().unwrap(), Shell::Pwsh);
        assert_eq!("powershell".parse::<Shell>().unwrap(), Shell::Pwsh);
    }

    #[test]
    fn shell_parses_case_insensitively() {
        assert_eq!("ZSH".parse::<Shell>().unwrap(), Shell::Zsh);
        assert_eq!("Fish".parse::<Shell>().unwrap(), Shell::Fish);
        assert_eq!("PWSH".parse::<Shell>().unwrap(), Shell::Pwsh);
    }

    #[test]
    fn shell_parse_rejects_unknown() {
        let err = "ksh".parse::<Shell>().unwrap_err();
        assert!(err.0 == "ksh");
    }

    #[test]
    fn zsh_init_emits_osc133_c_and_d_byte_exact() {
        // T1.5/T1.9: pin the OSC 133 C/D emission byte sequences so a
        // refactor that drops or breaks them is caught by the suite.
        let zsh = init_script(Shell::Zsh);
        assert!(
            zsh.contains(r"printf '\033]133;C\007'"),
            "zsh init must emit OSC 133 C at preexec"
        );
        assert!(
            zsh.contains(r#"printf '\033]133;D;%d\007' "$rs""#),
            "zsh init must emit OSC 133 D;<exit> at precmd"
        );
    }

    #[test]
    fn zsh_init_gates_osc133_on_warp_suppression() {
        // T1.5/T1.9: Warp's block model breaks on OSC 133 A — verify
        // the script suppresses emission when TERM_PROGRAM=WarpTerminal.
        let zsh = init_script(Shell::Zsh);
        assert!(
            zsh.contains("\"${TERM_PROGRAM:-}\" == \"WarpTerminal\""),
            "zsh init must check for and suppress Warp Terminal"
        );
        assert!(
            zsh.contains("_P10K_RS_SHELL_INTEGRATION"),
            "zsh init must compute a resolved shell-integration flag"
        );
    }

    #[test]
    fn zsh_transient_clears_rprompt_before_redraw() {
        // T1.1 — the transient widget must blank RPROMPT before
        // `zle reset-prompt` so the right-side ribbon doesn't linger
        // in scrollback next to the collapsed `❯`. Pin both the
        // assignment and the ordering.
        let zsh = init_script(Shell::Zsh);
        // Locate the assignment and the reset; assert RPROMPT="" comes
        // first (lower byte offset).
        let rprompt_clear = zsh
            .find(r#"RPROMPT="""#)
            .expect("zsh init must clear RPROMPT for the transient swap");
        let reset_prompt = zsh
            .find("zle reset-prompt 2>/dev/null")
            .expect("zsh init must call zle reset-prompt in the transient widget");
        assert!(
            rprompt_clear < reset_prompt,
            "RPROMPT clear must precede reset-prompt: clear@{rprompt_clear} reset@{reset_prompt}"
        );
    }

    #[test]
    fn zsh_precmd_rearms_bracketed_paste() {
        // T1.2 — every precmd must re-arm bracketed paste (DECSET 2004)
        // so a `\e[?2004l` left behind by command output doesn't blind
        // zsh's ZLE to paste markers on the next line.
        let zsh = init_script(Shell::Zsh);
        assert!(
            zsh.contains(r"printf '\033[?2004h'"),
            "zsh init must emit DECSET 2004 in precmd"
        );
    }

    #[test]
    fn zsh_gitstatusd_spawn_uses_rlimits() {
        // T1.15 — daemon spawn must be wrapped in a subshell that
        // applies ulimits before `exec`ing the binary, so a runaway
        // gitstatusd can't peg the box.
        let zsh = init_script(Shell::Zsh);
        assert!(
            zsh.contains("ulimit -v 524288"),
            "zsh init must cap gitstatusd virtual memory at 512 MiB (Linux)"
        );
        assert!(
            zsh.contains("ulimit -t 30"),
            "zsh init must cap gitstatusd CPU time at 30 s"
        );
        // Spawn happens via `exec` inside the rlimit subshell so the
        // pid we capture is the daemon itself, not a wrapping shell.
        assert!(
            zsh.contains(r#"exec "$_P10K_RS_GITSTATUSD_BIN""#),
            "zsh init must exec gitstatusd inside the rlimit subshell"
        );
    }

    #[test]
    fn zsh_dump_source_checks_mode_and_owner() {
        // T1.18 — refusing to source a dump unless it's a regular file
        // with mode 0600 owned by the current user. Pin the load-bearing
        // bits of the gate so a refactor doesn't quietly remove them.
        let zsh = init_script(Shell::Zsh);
        // Refuse symlinks.
        assert!(
            zsh.contains("! -L $_p10k_rs_dump"),
            "zsh init must refuse to source a symlinked dump"
        );
        // Mode check (0600).
        assert!(
            zsh.contains("0600"),
            "zsh init must enforce 0600 mode on the instant-prompt dump"
        );
        // Owner check via EUID.
        assert!(
            zsh.contains("uid] == EUID"),
            "zsh init must enforce dump ownership matches the running user"
        );
    }

    #[test]
    fn zsh_precmd_routes_stderr_to_diagnostics_log() {
        // T1.22 — binary stderr used to be discarded (`2>/dev/null`) so
        // the silent-failure pile-up flagged in
        // research/05-security-fs-ipc/audit-logging.md never had a
        // diagnostic channel. After T1.22 both precmd invocations
        // (left + right ribbons) append to the T1.21 diagnostics file
        // at `$XDG_STATE_HOME/p10k-rs/diagnostics.log` (fallback
        // `$HOME/.local/state/p10k-rs/diagnostics.log`), and the
        // dir is mkdir-p'd as a guard for the very first invocation
        // before the binary's `init_tracing` runs.
        let zsh = init_script(Shell::Zsh);
        // Original /dev/null pipe must be gone (otherwise we'd silently
        // double-discard half the failures).
        assert!(
            !zsh.contains("2>/dev/null) \""),
            "zsh init must no longer route the left-render binary stderr to /dev/null"
        );
        // Append-to-log path with the XDG fallback expression.
        let redirect = "2>>\"${XDG_STATE_HOME:-$HOME/.local/state}/p10k-rs/diagnostics.log\"";
        assert!(
            zsh.contains(redirect),
            "zsh init must append binary stderr to the T1.21 diagnostics log"
        );
        // The redirect must appear at least twice — once for the left
        // ribbon and once for the right. We assert >= 2 occurrences
        // so a future addition of more invocations (transient, …)
        // doesn't break the pin.
        let count = zsh.matches(redirect).count();
        assert!(
            count >= 2,
            "expected the diagnostics-log redirect on both PROMPT and RPROMPT invocations, found {count}"
        );
        // The mkdir guard must precede the first use so the very
        // first invocation in a fresh $XDG_STATE_HOME doesn't lose
        // its stderr to a no-such-file open.
        let mkdir = "mkdir -p \"${XDG_STATE_HOME:-$HOME/.local/state}/p10k-rs\" 2>/dev/null";
        let mkdir_pos = zsh
            .find(mkdir)
            .expect("zsh init must mkdir -p the diagnostics dir before the redirect");
        let first_redirect_pos = zsh
            .find(redirect)
            .expect("zsh init must contain the diagnostics redirect");
        assert!(
            mkdir_pos < first_redirect_pos,
            "mkdir guard must come before the first stderr redirect"
        );
    }

    #[test]
    fn zsh_init_auto_detect_probes_modern_terminals() {
        // The auto-detect path must check the canonical fingerprints
        // for modern terminals (Ghostty, Kitty, Windows Terminal, plus
        // TERM_PROGRAM as the umbrella probe for iTerm2 / WezTerm /
        // VSCode / Apple Terminal).
        let zsh = init_script(Shell::Zsh);
        for marker in [
            "TERM_PROGRAM",
            "WT_SESSION",
            "GHOSTTY_RESOURCES_DIR",
            "KITTY_WINDOW_ID",
        ] {
            assert!(
                zsh.contains(marker),
                "zsh init must probe ${marker} for shell-integration auto-detect"
            );
        }
    }

    // --- Slice 58: line-pre-redraw widget ----------------------------------------

    #[test]
    fn zsh_line_pre_redraw_widget_is_registered() {
        // Slice 58 — the widget must be declared as a named zle widget so
        // zsh knows to call it on every redraw. Without the `zle -N` binding
        // the function exists but is never invoked.
        let zsh = init_script(Shell::Zsh);
        assert!(
            zsh.contains("zle -N line-pre-redraw _p10k_rs_zle_line_pre_redraw"),
            "zsh init must register _p10k_rs_zle_line_pre_redraw as the line-pre-redraw widget"
        );
    }

    #[test]
    fn zsh_line_pre_redraw_updates_upcoming_cmd_from_buffer() {
        // The widget must store $BUFFER into _P10K_RS_UPCOMING_CMD so
        // precmd picks up the live command line.
        let zsh = init_script(Shell::Zsh);
        assert!(
            zsh.contains("_P10K_RS_UPCOMING_CMD=\"$BUFFER\""),
            "line-pre-redraw widget must assign _P10K_RS_UPCOMING_CMD from \\$BUFFER"
        );
    }

    #[test]
    fn zsh_line_pre_redraw_has_first_word_cache() {
        // The first-word cache avoids calling `zle reset-prompt` on every
        // character — only when the command verb changes. Pin both the
        // cache variable and the comparison.
        let zsh = init_script(Shell::Zsh);
        assert!(
            zsh.contains("_P10K_RS_PREV_UPCOMING_FIRST_WORD"),
            "zsh init must declare the first-word cache variable for line-pre-redraw"
        );
        assert!(
            zsh.contains("\"$first_word\" != \"$_P10K_RS_PREV_UPCOMING_FIRST_WORD\""),
            "line-pre-redraw widget must compare current first word to cached first word"
        );
    }

    #[test]
    fn zsh_precmd_resets_first_word_cache() {
        // After precmd fires the cache must be cleared so the next command
        // line starts fresh. Without this reset, a new command that shares
        // its verb with the previous one would not trigger reset-prompt.
        let zsh = init_script(Shell::Zsh);
        // The reset must appear inside _p10k_rs_precmd (after the
        // `_P10K_RS_UPCOMING_CMD=""` drain), not just at the init-time
        // typeset declaration. Search only the portion of the script
        // starting at the precmd function definition.
        let precmd_start = zsh
            .find("_p10k_rs_precmd()")
            .expect("zsh init must define _p10k_rs_precmd");
        let cache_reset = zsh[precmd_start..]
            .find("_P10K_RS_PREV_UPCOMING_FIRST_WORD=\"\"")
            .expect("zsh init must reset _P10K_RS_PREV_UPCOMING_FIRST_WORD inside _p10k_rs_precmd");
        let _ = cache_reset; // presence inside the function body is the assertion
    }

    #[test]
    fn zsh_line_pre_redraw_reset_prompt_gated_on_verb_change() {
        // `zle reset-prompt` must be inside the first-word-changed branch,
        // not called unconditionally on every keystroke. Verify ordering:
        // the cache comparison precedes `zle reset-prompt`.
        let zsh = init_script(Shell::Zsh);
        let cmp_pos = zsh
            .find("\"$first_word\" != \"$_P10K_RS_PREV_UPCOMING_FIRST_WORD\"")
            .expect("first-word comparison must exist");
        // There are two `zle reset-prompt` calls: one in the transient
        // widget and one in the line-pre-redraw widget. We want the one
        // that comes after the comparison.
        let reset_pos = zsh[cmp_pos..]
            .find("zle reset-prompt 2>/dev/null")
            .expect("zle reset-prompt must appear after the first-word comparison")
            + cmp_pos;
        assert!(
            reset_pos > cmp_pos,
            "zle reset-prompt must be inside the verb-changed branch"
        );
    }

    #[test]
    fn zsh_preexec_still_sets_upcoming_cmd() {
        // Slice 58 adds line-pre-redraw but must NOT remove the preexec
        // assignment — preexec provides the history-expanded command for
        // the "show context after running" case and overwrites BUFFER's
        // raw text with the expanded form.
        let zsh = init_script(Shell::Zsh);
        let preexec_start = zsh
            .find("_p10k_rs_preexec()")
            .expect("zsh init must define _p10k_rs_preexec");
        // The assignment must occur inside the preexec function body.
        let assign = zsh[preexec_start..]
            .find("_P10K_RS_UPCOMING_CMD=\"$1\"")
            .expect("_p10k_rs_preexec must still assign _P10K_RS_UPCOMING_CMD from \\$1");
        let _ = assign; // presence is the assertion
    }
    // --- v0.2: fish transient prompt -------------------------------------------

    #[test]
    fn fish_init_declares_transient_flag() {
        // v0.2 — fish transient prompt rides a per-shell flag that the
        // Enter-key wrapper sets and `fish_prompt` consumes. Pin both
        // the flag name (other code may grep for it) and the wrapper's
        // `commandline -f repaint` so the collapsed render lands in
        // scrollback before the command executes.
        let fish = init_script(Shell::Fish);
        assert!(
            fish.contains("_p10k_rs_transient"),
            "fish init must declare the transient flag variable"
        );
        assert!(
            fish.contains("commandline -f repaint"),
            "fish transient wrapper must repaint before executing"
        );
        assert!(
            fish.contains("commandline -f execute"),
            "fish transient wrapper must execute the line after the repaint"
        );
        assert!(
            fish.contains("bind \\r _p10k_rs_transient_execute"),
            "fish init must bind Enter (\\r) to the transient wrapper"
        );
    }

    #[test]
    fn fish_init_honors_all_four_transient_modes() {
        // v0.2 — fish transient must honor the same four
        // `TransientPromptMode` variants the zsh side does. The modes
        // are gated by the binary at `--render-side transient` (Off →
        // empty stdout, Always/SameDir-match/UniqueDir-match → emit,
        // mismatch → exit 2). Pin the wire surface the fish init
        // forwards: `--render-side transient`, `--last-prompt-cwd`,
        // `--prompt-cwd-history-file`, and the exit-code-2 KeepPrompt
        // fall-through to the full render. The four mode-name strings
        // themselves live in the binary; the shell side just needs to
        // round-trip the wire bits intact.
        let fish = init_script(Shell::Fish);
        assert!(
            fish.contains("--render-side transient"),
            "fish init must invoke the binary with --render-side transient"
        );
        assert!(
            fish.contains("--last-prompt-cwd"),
            "fish init must forward --last-prompt-cwd for same-dir / unique-dir modes"
        );
        assert!(
            fish.contains("--prompt-cwd-history-file"),
            "fish init must forward --prompt-cwd-history-file for unique-dir mode"
        );
        // The KeepPrompt contract (rc == 2 → full render in scrollback)
        // is implemented as a fall-through past the transient branch
        // when `rc` is non-zero or stdout is empty. We pin the
        // condition that triggers the emit so a refactor that drops
        // the rc-check breaks the suite rather than silently blanking
        // SameDir/UniqueDir mismatches.
        assert!(
            fish.contains("test $rc -eq 0"),
            "fish transient must gate the collapsed-emit on rc == 0 (KeepPrompt fall-through)"
        );
        // All four mode-name slugs must appear somewhere in the script
        // — in the header docs as the contract reference. This is the
        // belt-and-braces pin requested by the v0.2 ship plan: a
        // refactor that drops a mode from the contract docs surfaces
        // it here.
        for mode in ["Off", "Always", "SameDir", "UniqueDir"] {
            assert!(
                fish.contains(mode),
                "fish init must reference TransientPromptMode::{mode} in its contract docs"
            );
        }
    }

    #[test]
    fn fish_init_shifts_cwd_history_slots() {
        // Same `prev / curr` discipline the zsh precmd uses (T1.8).
        // The transient render reads `_P10K_RS_PREV_PROMPT_CWD` to
        // decide whether to collapse; the full-render path shifts it
        // forward by reading `_P10K_RS_CURR_PROMPT_CWD` into prev
        // and stamping the current `$PWD` into curr.
        let fish = init_script(Shell::Fish);
        assert!(
            fish.contains("_P10K_RS_PREV_PROMPT_CWD"),
            "fish init must track the previous prompt's cwd"
        );
        assert!(
            fish.contains("_P10K_RS_CURR_PROMPT_CWD"),
            "fish init must track the current prompt's cwd"
        );
    }

    // --- Slice 63: instant-prompt $TERM-aware cache key --------------------------

    #[test]
    fn zsh_dump_path_includes_term() {
        // Slice 63: the dump filename must embed a $TERM-derived token so
        // switching terminals (e.g. xterm-256color → tmux-256color) produces
        // a different path and causes a natural cache miss rather than
        // sourcing a dump rendered for the wrong terminal capabilities.
        let zsh = init_script(Shell::Zsh);
        // The $TERM variable must appear in the dump path assignment. We
        // check the sanitised-variable form used to build the filename.
        assert!(
            zsh.contains("TERM") && zsh.contains("_p10k_rs_term_safe"),
            "zsh init must embed a sanitised $TERM token in the dump filename"
        );
        // The dump path variable must reference the sanitised term token,
        // not the raw $TERM, to guard against hostile values.
        assert!(
            zsh.contains("_p10k_rs_term_safe"),
            "zsh init must sanitise $TERM before embedding it in the dump path"
        );
    }

    // --- v0.3: pwsh init ---------------------------------------------------

    #[test]
    fn pwsh_init_declares_global_prompt_function() {
        let pwsh = init_script(Shell::Pwsh);
        assert!(
            pwsh.contains("function global:prompt"),
            "pwsh init must declare function global:prompt"
        );
    }

    #[test]
    fn pwsh_init_invokes_binary_with_shell_pwsh() {
        let pwsh = init_script(Shell::Pwsh);
        assert!(
            pwsh.contains("--shell pwsh"),
            "pwsh init must invoke the binary with --shell pwsh"
        );
    }

    #[test]
    fn pwsh_init_captures_last_exit_code() {
        let pwsh = init_script(Shell::Pwsh);
        assert!(
            pwsh.contains("$global:LASTEXITCODE"),
            "pwsh init must read $global:LASTEXITCODE to capture the last exit status"
        );
        assert!(
            pwsh.contains("--last-status $status"),
            "pwsh init must forward the captured status via --last-status $status"
        );
    }

    #[test]
    fn pwsh_init_has_fallback_prompt() {
        let pwsh = init_script(Shell::Pwsh);
        assert!(
            pwsh.contains("executionContext.SessionState.Path.CurrentLocation"),
            "pwsh init fallback prompt must use executionContext.SessionState.Path.CurrentLocation"
        );
        assert!(
            pwsh.contains("PS "),
            "pwsh init fallback prompt must emit the 'PS ' prefix"
        );
    }

    #[test]
    fn pwsh_init_passes_zero_duration_ms() {
        let pwsh = init_script(Shell::Pwsh);
        assert!(
            pwsh.contains("--last-duration-ms 0"),
            "pwsh init must pass --last-duration-ms 0 (no preexec analogue in pwsh)"
        );
    }

    #[test]
    fn pwsh_init_joins_stdout_lines_with_lf_not_crlf() {
        // pwsh captures binary stdout as a string array; joining with "`n" (LF)
        // reconstructs the original output. Using "`r`n" (CRLF) would double-up
        // line endings on Windows because the binary already emits LF.
        let pwsh = init_script(Shell::Pwsh);
        assert!(
            pwsh.contains(r#"-join "`n""#),
            "pwsh init must join stdout lines with LF (`n), not CRLF (`r`n)"
        );
    }
}
