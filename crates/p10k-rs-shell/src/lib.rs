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
}
