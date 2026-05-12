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

/// Default Nerd Font v3 glyph (mdi `vim`). Override via
/// `[segment.vi_mode].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{f1058}";

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

        // Per-mode palette defaults match upstream P10K-classic: insert is
        // green/black (the calm default), normal is blue/white (the
        // "you're navigating" cue), visual is yellow/black (selection
        // highlight), replace is red/white (destructive). State-keyed TOML
        // overrides still win because we pass `state_tag` through the
        // `style::*` helpers — the default just shifts per state instead
        // of being constant blue/white.
        let (default_bg, default_fg) = default_palette_for(state_tag);

        let icon = style::resolve_icon(ctx.config, self.name(), Some(state_tag), DEFAULT_ICON);
        let plain_len = u16::try_from(label.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2); // icon + space
        let bg = style::render_bg(ctx.config, self.name(), Some(state_tag), default_bg.clone());
        let fg = style::render_fg(ctx.config, self.name(), Some(state_tag), default_fg);
        let text = format!(
            "{bg}{fg}{icon} {label}{}{}",
            style::reset_fg(),
            style::reset_bg()
        );
        SegmentOutput {
            text,
            plain_len,
            state: Some(state_tag),
            icon: Some(DEFAULT_ICON),
            background: Some(default_bg),
        }
    }
}

/// Per-mode default `(background, foreground)` pair.
///
/// Mirrors upstream Powerlevel10k's keymap palette:
/// - `insert` — green/black
/// - `command` (normal) — blue/white
/// - `visual` — yellow/black
/// - `replace` — red/white
/// - anything else falls back to the insert palette (matches
///   [`mode_label`]'s "unknown → insert" rule).
///
/// Pulled out as a free function so we can pick the default *before*
/// calling [`style::render_bg`] / [`style::render_fg`] — those helpers
/// take a single default and don't know about state defaults.
fn default_palette_for(state: &str) -> (Color, Color) {
    match state {
        "command" => (Color::Named("blue".into()), Color::Named("white".into())),
        "visual" => (Color::Named("yellow".into()), Color::Named("black".into())),
        "replace" => (Color::Named("red".into()), Color::Named("white".into())),
        // `insert`, `operator`, and unknown keymaps share the insert
        // palette — quiet green is the right "you're typing" default.
        _ => (Color::Named("green".into()), Color::Named("black".into())),
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
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::style::Color;
    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::{default_palette_for, mode_label, ViMode};

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

    #[test]
    fn render_emits_green_insert_ribbon_by_default() {
        // `render` reads `$P10K_RS_VI_MODE` directly, but we don't touch the
        // env here — `std::env::set_var` is `unsafe` since 1.85 and would
        // race across parallel test threads anyway (see the module docs).
        // With the var unset/empty, `mode_label("")` falls through to the
        // INSERT arm, which now defaults to green-on-black (P10K classic).
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = RenderCtx {
            config: &cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: Path::new("/"),
            git: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env: &env,
            upcoming_command: "",
        };
        let out = ViMode.render(&ctx);
        // Green bg (`48;5;2`) + black fg (`38;5;0`) — slice 34 restored
        // the per-mode palette differentiation.
        assert!(
            out.text.contains("\x1b[48;5;2m"),
            "green bg SGR missing: {:?}",
            out.text
        );
        assert!(
            out.text.contains("\x1b[38;5;0m"),
            "black fg SGR missing: {:?}",
            out.text
        );
        assert_eq!(out.background, Some(Color::Named("green".into())));
        assert_eq!(out.state, Some("insert"));
    }

    #[test]
    fn default_palette_matches_p10k_classic() {
        // Slice 34: per-mode default backgrounds. State-keyed TOML
        // overrides still win — these are just the defaults.
        assert_eq!(
            default_palette_for("insert"),
            (Color::Named("green".into()), Color::Named("black".into()))
        );
        assert_eq!(
            default_palette_for("command"),
            (Color::Named("blue".into()), Color::Named("white".into()))
        );
        assert_eq!(
            default_palette_for("visual"),
            (Color::Named("yellow".into()), Color::Named("black".into()))
        );
        assert_eq!(
            default_palette_for("replace"),
            (Color::Named("red".into()), Color::Named("white".into()))
        );
        // Operator and unknown share the insert palette (mirrors
        // `mode_label`'s "unknown → insert" fallback).
        assert_eq!(
            default_palette_for("operator"),
            (Color::Named("green".into()), Color::Named("black".into()))
        );
        assert_eq!(
            default_palette_for("garbage"),
            (Color::Named("green".into()), Color::Named("black".into()))
        );
    }

    #[test]
    fn state_keyed_toml_override_still_wins() {
        // Regression marker: per-mode defaults must not bypass the
        // state-keyed config path. If a user pins
        // `[segment.vi_mode.states.insert] background = "magenta"`,
        // the magenta wins over the new green default.
        let cfg = Config::from_toml(
            "schema_version = 1\n\
             [segment.vi_mode.states.insert]\n\
             background = \"magenta\"\n",
        )
        .expect("valid toml");
        let env = EnvSnapshot::default();
        let ctx = RenderCtx {
            config: &cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: Path::new("/"),
            git: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env: &env,
            upcoming_command: "",
        };
        let out = ViMode.render(&ctx);
        // magenta bg (`48;5;5`) — TOML override beat the green default.
        assert!(
            out.text.contains("\x1b[48;5;5m"),
            "magenta override missing: {:?}",
            out.text
        );
        // The default-fg path is still green's black (no fg override given).
        assert!(
            out.text.contains("\x1b[38;5;0m"),
            "black fg SGR missing: {:?}",
            out.text
        );
    }

    #[test]
    fn renders_with_default_icon() {
        // Same env caveat as the test above — we don't touch
        // `$P10K_RS_VI_MODE`; the unset/empty case falls into the INSERT
        // success path, which is exactly what we want to assert on.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = RenderCtx {
            config: &cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: Path::new("/"),
            git: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env: &env,
            upcoming_command: "",
        };
        let out = ViMode.render(&ctx);
        assert!(
            out.text.contains('\u{f1058}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{f1058}"));
    }
}
