//! `vi_mode` — current vi keymap indicator.
//!
//! Shows `INSERT` / `NORMAL` / `VISUAL` / `OPER` (colour-coded) based on the
//! shell's current zsh-line-editor keymap. This is the *consumer* half of a
//! two-part feature: a later slice will land the zsh-side plumbing — a
//! `zle-keymap-select` widget that exports `$P10K_RS_VI_MODE` whenever the
//! keymap flips — so the binary picks up the new state on the next prompt
//! redraw. Today the segment simply reads whatever `$P10K_RS_VI_MODE` holds
//! and renders accordingly; users running ahead of the init script can set
//! it by hand to exercise the segment.
//!
//! Kept env-var-driven (rather than a `RenderCtx` field) on the same logic
//! as `virtualenv.rs`: it's a hot-path lookup for one segment, and threading
//! a snapshot field for it would mean every test fixture grows a column. The
//! `mode_label` helper is pulled out as a pure function so tests don't have
//! to touch process-global env state (`std::env::set_var` is `unsafe` since
//! 1.85 and would race across parallel test threads anyway).

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Vi keymap indicator segment.
///
/// Reads `$P10K_RS_VI_MODE` and emits a short uppercase label coloured by
/// the active keymap. Hidden when the var is unset or empty.
#[derive(Debug, Default)]
pub struct ViMode;

impl Segment for ViMode {
    fn name(&self) -> &'static str {
        "vi_mode"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        std::env::var("P10K_RS_VI_MODE")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call standalone — same pattern as `virtualenv.rs`.
        let raw = std::env::var("P10K_RS_VI_MODE").unwrap_or_default();
        let (state_tag, label) = mode_label(&raw);

        let default = match state_tag {
            "command" => Color::Named("yellow".into()),
            "visual" => Color::Named("magenta".into()),
            "operator" => Color::Named("cyan".into()),
            _ => Color::Named("green".into()),
        };
        let plain_len = u16::try_from(label.chars().count()).unwrap_or(u16::MAX);
        let fg = style::render_fg(ctx.config, self.name(), Some(state_tag), default);
        let text = format!("{fg}{label}{}", style::reset_fg());
        SegmentOutput {
            text,
            plain_len,
            state: Some(state_tag),
            icon: None,
        }
    }
}

/// Map a raw `$KEYMAP` value to a `(state_tag, label)` pair.
///
/// `state_tag` is the stable identifier used for TOML state-keyed colour
/// overrides (e.g. `[segments.vi_mode.state.command] fg = "red"`). `label`
/// is what the user sees in the prompt.
///
/// Unknown keymaps fall back to `("insert", "INSERT")` — that matches zsh's
/// default keymap when no `bindkey -v` / `bindkey -e` has run yet, so an
/// uninitialised or garbled value renders as the safest assumption.
pub(crate) fn mode_label(raw: &str) -> (&'static str, &'static str) {
    // `viins` falls through to the wildcard arm — both produce the insert
    // state, and clippy correctly flags an explicit arm as redundant.
    match raw {
        "vicmd" => ("command", "NORMAL"),
        "visual" | "visualline" => ("visual", "VISUAL"),
        "viopp" => ("operator", "OPER"),
        _ => ("insert", "INSERT"),
    }
}

#[cfg(test)]
mod tests {
    use super::mode_label;

    #[test]
    fn mode_label_insert() {
        assert_eq!(mode_label("viins"), ("insert", "INSERT"));
    }

    #[test]
    fn mode_label_command() {
        assert_eq!(mode_label("vicmd"), ("command", "NORMAL"));
    }

    #[test]
    fn mode_label_visual() {
        assert_eq!(mode_label("visual"), ("visual", "VISUAL"));
        assert_eq!(mode_label("visualline"), ("visual", "VISUAL"));
    }

    #[test]
    fn mode_label_unknown_defaults_to_insert() {
        assert_eq!(mode_label("garbage"), ("insert", "INSERT"));
    }
}
