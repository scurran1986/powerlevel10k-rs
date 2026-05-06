//! Configuration wizard TUI for `p10k-rs`.
//!
//! Drives the 30-screen single-key flow described in
//! `06-wizard-and-presets.md`. Built on `crossterm` (no `ratatui` for the
//! basic flow — most screens are a header, a list, and a footer; the extra
//! widget tree doesn't earn its keep until we add live previews). Output is
//! a fully-validated [`p10k_rs_config::Config`] written atomically by the
//! caller.
//!
//! Implementation lands in the wizard phase per `ROADMAP.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use thiserror::Error;

/// Errors raised by the wizard.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WizardError {
    /// User aborted before completing the flow (Ctrl-C, Esc, etc.).
    #[error("wizard cancelled by user")]
    Cancelled,
    /// Terminal does not meet capability minimums (size, color, key input).
    #[error("terminal does not meet capability minimums: {0}")]
    UnsupportedTerminal(String),
    /// I/O failure during the flow.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Run the wizard interactively against the current terminal.
///
/// Returns a fully-validated [`p10k_rs_config::Config`]. The caller is
/// responsible for writing it to disk atomically.
///
/// # Errors
///
/// See [`WizardError`].
///
/// # Panics
///
/// Currently unimplemented — lands in the wizard phase.
pub fn run() -> Result<p10k_rs_config::Config, WizardError> {
    unimplemented!("the wizard lands in its own roadmap phase")
}
