//! Per-shell init scripts and integration glue.
//!
//! Init scripts live as plain text under `crates/p10k-rs-shell/shells/<sh>/`
//! and are byte-included into the binary via [`include_str!`] so
//! `p10k-rs init <shell>` prints the right snippet without reading from
//! disk at runtime. See `ARCHITECTURE.md` § 2.5.
//!
//! Today only zsh ships; fish and bash come in later phases.

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

/// Returned when the requested shell exists but its init script hasn't
/// landed yet.
#[derive(Debug, thiserror::Error)]
#[error("init script for {0:?} hasn't shipped yet")]
pub struct InitScriptUnimplemented(pub Shell);

/// Returns the init script for the requested shell.
///
/// The returned string is byte-included from `shells/<shell>/init.<ext>` at
/// compile time. The binary writes it to stdout for `eval`/`source` consumption.
///
/// # Errors
///
/// Returns [`InitScriptUnimplemented`] for shells whose init script hasn't
/// been written yet (today: only zsh).
pub fn init_script(shell: Shell) -> Result<&'static str, InitScriptUnimplemented> {
    match shell {
        Shell::Zsh => Ok(include_str!("../shells/zsh/init.zsh")),
        Shell::Fish | Shell::Bash => Err(InitScriptUnimplemented(shell)),
    }
}
