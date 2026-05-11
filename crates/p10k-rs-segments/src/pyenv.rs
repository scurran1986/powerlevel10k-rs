//! `pyenv` — active pyenv Python version segment.
//!
//! Shows `py:<version>` in yellow when `$PYENV_VERSION` is set and non-empty.
//! `pyenv` exports this variable when a shim resolves a version (shell, local,
//! or global); we read it directly rather than shelling out to `pyenv version`
//! because the env var is the same source pyenv's own prompt integration uses
//! and avoids a fork on every prompt.
//!
//! The version string is attacker-controlled in the same sense as a venv path:
//! `$PYENV_VERSION` could in principle contain control bytes (a malicious
//! `.python-version` file, or a hostile parent process). Sanitisation happens
//! before the value lands in `text` (see `virtualenv.rs` and `dir.rs` for the
//! same pattern).

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// pyenv segment.
///
/// Reads `$PYENV_VERSION` and emits `py:<version>` (e.g. `py:3.12.1`,
/// `py:system`, `py:2.7.18/envs/legacy`). Hidden when the var is unset or
/// empty.
#[derive(Debug, Default)]
pub struct Pyenv;

impl Segment for Pyenv {
    fn name(&self) -> &'static str {
        "pyenv"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        current_pyenv_version().is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same pattern as `virtualenv.rs`.
        let Some(version) = current_pyenv_version() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
            };
        };

        let plain = format!("py:{version}");
        let plain_len = u16::try_from(plain.chars().count()).unwrap_or(u16::MAX);
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("yellow".into()));
        let text = format!("{fg}{plain}{}", style::reset_fg());
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: None,
        }
    }
}

/// Read `$PYENV_VERSION` and return the sanitised version string, or `None`
/// when unset / empty / scrubbed-to-empty.
///
/// Kept as a free function so the env-var read is in one place. Tests
/// exercise [`sanitise_version`] directly to avoid mutating process-global
/// env state (`std::env::set_var` is `unsafe` since Rust 1.85, and parallel
/// test threads would race on it anyway).
fn current_pyenv_version() -> Option<String> {
    let raw = std::env::var("PYENV_VERSION").ok()?;
    sanitise_version(&raw)
}

/// Sanitise a raw `$PYENV_VERSION` value for terminal output.
///
/// Returns `None` for an empty input, or when the sanitiser strips the input
/// down to nothing. Otherwise returns the cleaned version string suitable for
/// landing in the prompt.
fn sanitise_version(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let clean = sanitize_for_terminal(raw);
    if clean.is_empty() {
        None
    } else {
        Some(clean)
    }
}

#[cfg(test)]
mod tests {
    use super::sanitise_version;

    #[test]
    fn sanitise_version_strips_control_chars() {
        assert_eq!(sanitise_version("3.12.1\r"), Some("3.12.1".to_owned()));
    }

    #[test]
    fn sanitise_version_empty_is_none() {
        assert_eq!(sanitise_version(""), None);
    }

    #[test]
    fn sanitise_version_passes_normal() {
        assert_eq!(sanitise_version("3.12.1"), Some("3.12.1".to_owned()));
    }

    #[test]
    fn sanitise_version_passes_system_keyword() {
        assert_eq!(sanitise_version("system"), Some("system".to_owned()));
    }
}
