//! `root_indicator` — visible warning when the shell is running as root.
//!
//! Fires only when the effective UID is 0. Renders a single red lightning
//! bolt (`⚡`) so that a privileged shell is impossible to miss. Users who
//! never want this segment can simply drop `"root_indicator"` from
//! `[layout].left` (or `.right`) in their config.

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

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
        rustix::process::geteuid().as_raw() == 0
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let plain = "⚡";
        let plain_len: u16 = 1;
        // Hardcoded red bg + white fg default: root is the warning state and
        // the ribbon makes it impossible to miss. TOML can still override
        // via `[segments.root_indicator] fg = "..."` / `bg = "..."`.
        let bg = style::render_bg(ctx.config, self.name(), None, Color::Named("red".into()));
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("white".into()));
        let text = format!("{bg}{fg}{plain}{}{}", style::reset_fg(), style::reset_bg());
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: None,
            background: Some(Color::Named("red".into())),
        }
    }
}

#[cfg(test)]
mod tests {
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
            git: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
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
}
