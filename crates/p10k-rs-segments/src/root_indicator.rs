//! `root_indicator` — visible warning when the shell is running as root.
//!
//! Fires only when the effective UID is 0. Renders a single red lightning
//! bolt (`⚡`) so that a privileged shell is impossible to miss. Users who
//! never want this segment can simply drop `"root_indicator"` from
//! `[layout].left` (or `.right`) in their config.

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (user-secret — reads as "privileged shell").
/// Override via `[segment.root_indicator].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{f2be}";

/// Root-indicator segment — lights up when EUID == 0.
#[derive(Debug, Default)]
pub struct RootIndicator;

impl Segment for RootIndicator {
    fn name(&self) -> &'static str {
        "root_indicator"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        // EUID (not RUID): reflects the privilege the process *currently
        // holds*, which is what the prompt is warning about. A setuid-root
        // binary that dropped privileges shouldn't keep flashing red.
        //
        // Non-Unix: there is no EUID and the elevation model is entirely
        // different (UAC on Windows). Always-disabled keeps the segment
        // additive on unix without claiming a meaningful "root" answer
        // on platforms where the concept doesn't translate. A future
        // Windows-aware admin probe would land here.
        #[cfg(unix)]
        {
            rustix::process::geteuid().as_raw() == 0
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let plain = "⚡";
        let icon = style::resolve_icon(ctx.config, self.name(), None, DEFAULT_ICON);
        // Lightning + icon + space; saturating_add to mirror the rest of the
        // codebase even though overflow is impossible from a literal `1`.
        let plain_len: u16 = 1u16.saturating_add(2);
        // Hardcoded red bg + white fg default: root is the warning state and
        // the ribbon makes it impossible to miss. TOML can still override
        // via `[segments.root_indicator] fg = "..."` / `bg = "..."`.
        let bg = style::render_bg(ctx.config, self.name(), None, Color::Named("red".into()));
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("white".into()));
        let text = format!(
            "{bg}{fg}{icon} {plain}{}{}",
            style::reset_fg(),
            style::reset_bg()
        );
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: Some(DEFAULT_ICON),
            background: Some(style::resolve_bg(
                ctx.config,
                self.name(),
                None,
                Color::Named("red".into()),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use p10k_rs_core::safety::SafeText;
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::RootIndicator;

    fn make_ctx<'a>(cfg: &'a Config, env: &'a EnvSnapshot) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: Path::new("/"),
            cwd_display: SafeText::default(),
            git: None,
            jj: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
            upcoming_command: "",
            shell_integration_active: false,
            sync_output: false,
        }
    }

    // We deliberately do NOT unit-test `enabled()`: changing the EUID is
    // `unsafe` and process-global (it would corrupt every other test
    // running in the same binary). A proper integration test would need a
    // dedicated `sudo`-wrapped runner, which we don't have today. The
    // renderer's contract is that `render` works regardless of `enabled`,
    // so we exercise the visual output unconditionally.
    #[test]
    fn render_emits_lightning_in_red() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = make_ctx(&cfg, &env);
        let out = RootIndicator.render(&ctx);
        assert!(out.text.contains("⚡"));
        // Powerline ribbon: red bg (`48;5;1`) + white fg (`38;5;7`). The bg
        // declaration must also surface on `SegmentOutput.background` so the
        // renderer can paint arrows in the matching colour.
        assert!(out.text.contains("\x1b[48;5;1m"));
        assert!(out.text.contains("\x1b[38;5;7m"));
        assert_eq!(
            out.background,
            Some(p10k_rs_core::style::Color::Named("red".into()))
        );
    }

    #[test]
    fn renders_with_default_icon() {
        // Same caveat as the test above — we exercise `render` unconditionally
        // because changing the EUID would be `unsafe` and process-global.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = make_ctx(&cfg, &env);
        let out = RootIndicator.render(&ctx);
        assert!(
            out.text.contains('\u{f2be}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{f2be}"));
    }
}
