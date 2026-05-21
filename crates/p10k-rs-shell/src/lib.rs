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
}

impl FromStr for Shell {
    type Err = UnsupportedShell;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "zsh" => Ok(Self::Zsh),
            "fish" => Ok(Self::Fish),
            "bash" => Ok(Self::Bash),
            other => Err(UnsupportedShell(other.to_owned())),
        }
    }
}

/// Returned when [`Shell::from_str`] gets a string we don't support.
#[derive(Debug, thiserror::Error)]
#[error("unknown shell '{0}': supported = zsh, fish, bash")]
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
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn every_shell_has_a_non_empty_script() {
        for shell in [Shell::Zsh, Shell::Bash, Shell::Fish] {
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
        for shell in [Shell::Zsh, Shell::Bash, Shell::Fish] {
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
        for shell in [Shell::Zsh, Shell::Bash, Shell::Fish] {
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
    }

    #[test]
    fn shell_parses_case_insensitively() {
        assert_eq!("ZSH".parse::<Shell>().unwrap(), Shell::Zsh);
        assert_eq!("Fish".parse::<Shell>().unwrap(), Shell::Fish);
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
}
