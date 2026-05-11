//! `nodenv` — Node.js version-manager segment.
//!
//! Shows `node:<version>` in green when `$NODENV_VERSION` is set and
//! non-empty. `nodenv` exports this variable when a shim resolves to a
//! specific version (per-directory `.node-version`, `nodenv shell`, or a
//! global default), so we read it directly rather than shelling out to
//! `nodenv version-name` on every prompt — that would re-introduce the
//! latency tax this project exists to avoid.
//!
//! The value is attacker-controlled in the same sense `cwd` is — a
//! `.node-version` file containing a CR byte would otherwise let an
//! attacker overwrite the prompt line. Sanitisation happens before the
//! value lands in `text` (see `dir.rs` and `virtualenv.rs` for the same
//! pattern).

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// `nodenv` Node.js version segment.
///
/// Reads `$NODENV_VERSION` and emits `node:<version>`. Hidden when the
/// var is unset or empty.
#[derive(Debug, Default)]
pub struct Nodenv;

impl Segment for Nodenv {
    fn name(&self) -> &'static str {
        "nodenv"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        current_nodenv_version().is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same pattern as `virtualenv.rs` and
        // `vcs.rs`.
        let Some(version) = current_nodenv_version() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
            };
        };

        let plain = format!("node:{version}");
        let plain_len = u16::try_from(plain.chars().count()).unwrap_or(u16::MAX);
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("green".into()));
        let text = format!("{fg}{plain}{}", style::reset_fg());
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: None,
        }
    }
}

/// Read `$NODENV_VERSION` and return the sanitised version string, or
/// `None` when unset / empty / sanitised-to-empty.
///
/// Kept as a free function so the env-var read is in one place. Tests
/// exercise [`sanitise_version`] directly to avoid mutating
/// process-global env state (`std::env::set_var` is `unsafe` since Rust
/// 1.85, and parallel test threads would race on it anyway).
fn current_nodenv_version() -> Option<String> {
    let raw = std::env::var("NODENV_VERSION").ok()?;
    sanitise_version(&raw)
}

/// Sanitise a raw `$NODENV_VERSION` value for terminal output.
///
/// Returns `None` for empty input or input that sanitises to the empty
/// string (e.g. a value consisting entirely of control characters).
/// Otherwise returns the cleaned version string — `20.10.0`, `system`,
/// `lts/iron`, etc. all pass through unchanged.
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
        // A `.node-version` file with a trailing CR would otherwise let
        // an attacker overwrite the start of the prompt line.
        assert_eq!(sanitise_version("20.10.0\r"), Some("20.10.0".to_owned()));
    }

    #[test]
    fn sanitise_version_empty_is_none() {
        assert_eq!(sanitise_version(""), None);
    }

    #[test]
    fn sanitise_version_passes_normal() {
        assert_eq!(sanitise_version("20.10.0"), Some("20.10.0".to_owned()));
    }

    #[test]
    fn sanitise_version_passes_lts_alias() {
        assert_eq!(sanitise_version("lts/iron"), Some("lts/iron".to_owned()));
    }
}
