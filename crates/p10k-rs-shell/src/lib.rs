//! Per-shell init scripts and integration glue.
//!
//! Init scripts live as plain text under `crates/p10k-rs-shell/shells/<sh>/`
//! and are byte-included into the binary via [`include_str!`] so
//! `p10k-rs init <shell>` prints the right snippet without reading from
//! disk at runtime. See `ARCHITECTURE.md` § 2.5.
//!
//! The actual scripts ship in the foundation phase (zsh first, then fish,
//! then bash). This crate is currently the public surface and a stable
//! [`Shell`] enum the binary dispatches on.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Which shell init we want.
///
/// Re-exports [`p10k_rs_core::Shell`] indirectly: the shell crate
/// authoritatively owns the per-shell behaviour, but the type itself is
/// shared so the binary can take it as a CLI value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shell {
    /// Z shell.
    Zsh,
    /// Friendly Interactive Shell.
    Fish,
    /// Bourne Again Shell.
    Bash,
}

/// Returns the init script for the requested shell.
///
/// The returned `&'static str` is byte-included from
/// `shells/<shell>/init.<ext>` at compile time. The binary writes it to
/// stdout for `eval`/`source` consumption.
///
/// # Panics
///
/// Currently unimplemented — the per-shell scripts land in the foundation
/// phase. See `ROADMAP.md`.
#[must_use]
pub fn init_script(_shell: Shell) -> &'static str {
    unimplemented!("init scripts ship with the foundation-phase shell support")
}
