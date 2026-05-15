//! `aws` — AWS profile segment.
//!
//! Shows `aws:<profile>` in yellow when one of the standard AWS environment
//! variables is set. Probes in order:
//!
//! 1. `$AWS_VAULT` — set by aws-vault when a vault session is active.
//! 2. `$AWS_PROFILE` — the standard aws-cli profile selector.
//! 3. `$AWS_DEFAULT_PROFILE` — the older aws-cli name; kept for back-compat.
//!
//! The value is a profile *name* (e.g. `prod-readonly`), not a path, so we
//! do not basename it. It is, however, an attacker-controlled string —
//! a malicious `$AWS_PROFILE` containing control bytes would otherwise let
//! a CR ride into the prompt line. Sanitisation happens before the value
//! lands in `text` (see `virtualenv.rs` for the same pattern).
//!
//! Yellow is upstream Powerlevel10k's nearest match for the orange-ish
//! default we don't have in the named-colour table.

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (amazon / aws cloud). Override via
/// `[segment.aws].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{f270}";

/// AWS profile segment.
///
/// Reads the first non-empty value from `$AWS_VAULT`, `$AWS_PROFILE`, then
/// `$AWS_DEFAULT_PROFILE`, and emits `aws:<profile>`. Hidden when none are
/// set or all are empty.
#[derive(Debug, Default)]
pub struct Aws;

impl Segment for Aws {
    fn name(&self) -> &'static str {
        "aws"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        current_aws_profile().is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same pattern as `vcs.rs` does for a
        // missing `ctx.git`.
        let Some(profile) = current_aws_profile() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
                background: None,
            };
        };

        let plain = format!("aws:{profile}");
        let icon = style::resolve_icon(ctx.config, self.name(), None, DEFAULT_ICON);
        let bg = style::render_bg(ctx.config, self.name(), None, Color::Named("yellow".into()));
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
            background: Some(Color::Named("yellow".into())),
        }
    }
}

/// Probe the AWS env vars in precedence order and return the first
/// non-empty, sanitised profile name.
///
/// Kept as a free function so the env-var read is in one place. Tests
/// exercise [`sanitize_profile`] directly to avoid mutating process-global
/// env state (`std::env::set_var` is `unsafe` since Rust 1.85, and parallel
/// test threads would race on it anyway).
fn current_aws_profile() -> Option<String> {
    for var in ["AWS_VAULT", "AWS_PROFILE", "AWS_DEFAULT_PROFILE"] {
        if let Ok(raw) = std::env::var(var) {
            if let Some(clean) = sanitize_profile(&raw) {
                return Some(clean);
            }
        }
    }
    None
}

/// Sanitise a raw profile string for terminal output. Returns `None` when
/// the input is empty or sanitises down to an empty string; otherwise the
/// cleaned value.
///
/// Profile names are user-set strings (env vars, aws-vault output) and so
/// must be treated as untrusted before rendering — see the module docs.
fn sanitize_profile(raw: &str) -> Option<String> {
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
    use super::sanitize_profile;

    #[test]
    fn sanitise_profile_strips_control_chars() {
        // C2 reproducer: a profile name containing `\r` would otherwise
        // let an attacker overwrite the start of the prompt line on render.
        assert_eq!(sanitize_profile("prod\rEVIL"), Some("prodEVIL".to_owned()));
    }

    #[test]
    fn sanitise_profile_empty_is_none() {
        assert_eq!(sanitize_profile(""), None);
    }

    #[test]
    fn sanitise_profile_passes_normal_name() {
        assert_eq!(
            sanitize_profile("prod-readonly"),
            Some("prod-readonly".to_owned())
        );
    }
}
