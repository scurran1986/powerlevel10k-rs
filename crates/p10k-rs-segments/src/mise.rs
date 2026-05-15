//! `mise` — cross-language version-manager segment (formerly known as `rtx`).
//!
//! `mise` is the dominant polyglot version manager and the most-requested
//! unfilled segment in the upstream Powerlevel10k tracker (issue #2212). It
//! manages tool versions across Node, Python, Ruby, Go, and friends from a
//! single `.tool-versions` / `.mise.toml` file.
//!
//! Detection here is intentionally minimal: if `$MISE_DATA_DIR` is set and
//! non-empty, mise is active in this shell. We additionally read
//! `$MISE_PROFILE` (and fall back to `$MISE_DEFAULT_TOOL_VERSIONS`) so the
//! segment can show the active profile name when one is in play — e.g.
//! `mise:production`. The shell-side render path reads each env var exactly
//! once per prompt; we never shell out to `mise current` because that would
//! re-introduce the latency tax this project exists to avoid.
//!
//! The profile name is attacker-controlled in the same sense `cwd` is — a
//! `.mise.toml` file or `MISE_PROFILE` value containing a CR byte would
//! otherwise let an attacker overwrite the prompt line. Sanitisation happens
//! before the value lands in `text` (see `nodenv.rs` / `pyenv.rs` for the
//! same pattern).
//!
//! Registration is done in `crates/p10k-rs-segments/src/lib.rs::build()` —
//! the orchestrator adds both `mise` and `rtx` (as an alias) match arms so
//! users on either name in their TOML config get the same segment.

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (mdi-tools — generic toolbox, the closest
/// stable codepoint for a cross-language version manager). Override via
/// `[segment.mise].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{f0a9b}";

/// `mise` cross-language version-manager segment.
///
/// Hidden when `$MISE_DATA_DIR` is unset or empty. When active renders
/// `mise` alone, or `mise:<profile>` when `$MISE_PROFILE` (or
/// `$MISE_DEFAULT_TOOL_VERSIONS` as a fallback) names a profile.
#[derive(Debug, Default)]
pub struct Mise;

impl Segment for Mise {
    fn name(&self) -> &'static str {
        "mise"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        mise_active()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same pattern as `nodenv.rs`.
        if !mise_active() {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
                background: None,
            };
        }

        let plain = match current_mise_profile() {
            Some(profile) => format!("mise:{profile}"),
            None => "mise".to_owned(),
        };

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

/// Return `true` when mise is active in the current shell.
///
/// mise exports `$MISE_DATA_DIR` on activation; an unset or empty value
/// means no shim is in play and the segment stays hidden. Reading the env
/// var happens once per prompt — no fork, no IPC.
fn mise_active() -> bool {
    std::env::var("MISE_DATA_DIR")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Read the active mise profile name, or `None` when no profile is set.
///
/// Precedence: `$MISE_PROFILE` first (the documented profile variable),
/// then `$MISE_DEFAULT_TOOL_VERSIONS` as a fallback (mise sets one or the
/// other depending on activation mode). Each var is read once; the first
/// non-empty value wins after sanitisation.
///
/// Kept as a free function so the env-var reads are in one place. Tests
/// exercise [`sanitise_profile`] directly to avoid mutating process-global
/// env state (`std::env::set_var` is `unsafe` since Rust 1.85, and
/// parallel test threads would race on it anyway).
fn current_mise_profile() -> Option<String> {
    if let Ok(raw) = std::env::var("MISE_PROFILE") {
        if let Some(clean) = sanitise_profile(&raw) {
            return Some(clean);
        }
    }
    if let Ok(raw) = std::env::var("MISE_DEFAULT_TOOL_VERSIONS") {
        if let Some(clean) = sanitise_profile(&raw) {
            return Some(clean);
        }
    }
    None
}

/// Sanitise a raw mise profile name for terminal output.
///
/// Returns `None` for empty input or input that sanitises to the empty
/// string (e.g. a value consisting entirely of control characters).
/// Otherwise returns the cleaned profile name — `production`, `dev`,
/// `staging`, etc. all pass through unchanged.
fn sanitise_profile(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let clean = sanitize_for_terminal(raw).into_owned();
    if clean.is_empty() {
        None
    } else {
        Some(clean)
    }
}

#[cfg(test)]
mod tests {
    use super::sanitise_profile;

    #[test]
    fn sanitise_profile_strips_control_chars() {
        // A `.mise.toml` profile name with a trailing CR would otherwise
        // let an attacker overwrite the start of the prompt line.
        assert_eq!(
            sanitise_profile("production\r"),
            Some("production".to_owned())
        );
    }

    #[test]
    fn sanitise_profile_empty_is_none() {
        assert_eq!(sanitise_profile(""), None);
    }

    #[test]
    fn sanitise_profile_passes_normal() {
        assert_eq!(
            sanitise_profile("production"),
            Some("production".to_owned())
        );
    }
}
