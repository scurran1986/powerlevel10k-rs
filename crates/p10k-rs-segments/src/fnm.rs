//! `fnm` — Fast Node Manager version segment.
//!
//! Shows `fnm:<version>` in green when `$FNM_NODE_VERSION` is set and
//! non-empty. `fnm`'s shell integration exports this variable when the
//! `fnm env` hook resolves to a specific version (per-directory
//! `.node-version` / `.nvmrc`, `fnm use`, or a configured default), so
//! we read it directly rather than shelling out to `fnm current` on
//! every prompt — that would re-introduce the latency tax this project
//! exists to avoid.
//!
//! Upstream Powerlevel10k tracks fnm support in romkatv/powerlevel10k
//! #713. fnm also sets `$FNM_DIR` and `$FNM_MULTISHELL_PATH` when its
//! shell integration is active; we could fall back to reading the
//! `<FNM_MULTISHELL_PATH>/installation` symlink target when
//! `$FNM_VERSION_FILE_STRATEGY=local` keeps the version out of the
//! environment, but that's a syscall per prompt — deferred for MVP.
//!
//! The value is attacker-controlled in the same sense `cwd` is — a
//! `.node-version` file containing a CR byte would otherwise let an
//! attacker overwrite the prompt line. Sanitisation happens before the
//! value lands in `text` (see `dir.rs` and `virtualenv.rs` for the same
//! pattern).

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (mdi-nodejs). Override via
/// `[segment.fnm].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{f898}";

/// `fnm` (Fast Node Manager) Node.js version segment.
///
/// Reads `$FNM_NODE_VERSION` and emits `fnm:<version>`. Hidden when the
/// var is unset or empty.
#[derive(Debug, Default)]
pub struct Fnm;

impl Segment for Fnm {
    fn name(&self) -> &'static str {
        "fnm"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        current_fnm_version().is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same pattern as `virtualenv.rs` and
        // `vcs.rs`.
        let Some(version) = current_fnm_version() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
                background: None,
            };
        };

        let plain = format!("fnm:{version}");
        let icon = style::resolve_icon(ctx.config, self.name(), None, DEFAULT_ICON);
        let bg = style::render_bg(ctx.config, self.name(), None, Color::Named("green".into()));
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("black".into()));
        let text = format!(
            "{bg}{fg}{icon} {plain}{}{}",
            style::reset_fg(),
            style::reset_bg()
        );
        let plain_len = u16::try_from(plain.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2); // icon + space
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: Some(DEFAULT_ICON),
            background: Some(Color::Named("green".into())),
        }
    }
}

/// Read `$FNM_NODE_VERSION` and return the sanitised version string, or
/// `None` when unset / empty / sanitised-to-empty.
///
/// Kept as a free function so the env-var read is in one place. Tests
/// exercise [`sanitise_version`] directly to avoid mutating
/// process-global env state (`std::env::set_var` is `unsafe` since Rust
/// 1.85, and parallel test threads would race on it anyway).
fn current_fnm_version() -> Option<String> {
    let raw = std::env::var("FNM_NODE_VERSION").ok()?;
    sanitise_version(&raw)
}

/// Sanitise a raw `$FNM_NODE_VERSION` value for terminal output.
///
/// Returns `None` for empty input or input that sanitises to the empty
/// string (e.g. a value consisting entirely of control characters).
/// Otherwise returns the cleaned version string — `v20.10.0`, `system`,
/// `lts/iron`, etc. all pass through unchanged. fnm prefixes its
/// exported version with a leading `v` (e.g. `v20.10.0`); the
/// downstream consumer renders whatever fnm emits.
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
        assert_eq!(sanitise_version("v20.10.0\r"), Some("v20.10.0".to_owned()));
    }

    #[test]
    fn sanitise_version_empty_is_none() {
        assert_eq!(sanitise_version(""), None);
    }

    #[test]
    fn sanitise_version_passes_normal() {
        // fnm's shell integration exports the version with a leading
        // `v` (e.g. `v20.10.0`), matching `fnm current` output.
        assert_eq!(sanitise_version("v20.10.0"), Some("v20.10.0".to_owned()));
    }
}
