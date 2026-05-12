//! `status` — last command exit code when non-zero.
//!
//! Hidden on success (the most common case). On failure shows `✘<code>`
//! painted white-on-red (P10K-classic palette). Visible alongside the red
//! `prompt_char` so the user sees both "the prompt knows it failed"
//! (chevron) and "what code did it exit with" (this segment) — useful for
//! `137` (OOM-kill), `130` (SIGINT), arbitrary user codes, etc.

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Status (exit-code) segment.
#[derive(Debug, Default)]
pub struct Status;

impl Segment for Status {
    fn name(&self) -> &'static str {
        "status"
    }

    fn enabled(&self, ctx: &RenderCtx<'_>) -> bool {
        ctx.last_status != 0
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let code = ctx.last_status;
        let plain = format!("✘{code}");
        let plain_len = u16::try_from(plain.chars().count()).unwrap_or(u16::MAX);
        let bg = style::render_bg(
            ctx.config,
            self.name(),
            Some("error"),
            Color::Named("red".into()),
        );
        let fg = style::render_fg(
            ctx.config,
            self.name(),
            Some("error"),
            Color::Named("white".into()),
        );
        let text = format!("{bg}{fg}{plain}{}{}", style::reset_fg(), style::reset_bg());
        SegmentOutput {
            text,
            plain_len,
            state: Some("error"),
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

    use super::Status;

    fn make_ctx<'a>(cfg: &'a Config, env: &'a EnvSnapshot, last_status: i32) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: Path::new("/"),
            git: None,
            last_status,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
        }
    }

    #[test]
    fn hidden_on_success() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = make_ctx(&cfg, &env, 0);
        assert!(!Status.enabled(&ctx));
    }

    #[test]
    fn shown_on_failure_with_code() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = make_ctx(&cfg, &env, 1);
        assert!(Status.enabled(&ctx));
        let out = Status.render(&ctx);
        assert!(out.text.contains("✘1"));
        // Slice 28A: P10K-classic white-on-red.
        assert!(
            out.text.contains("\x1b[38;5;7m"),
            "white fg: {:?}",
            out.text
        );
        assert!(out.text.contains("\x1b[48;5;1m"), "red bg: {:?}", out.text);
        assert_eq!(out.state, Some("error"));
        assert_eq!(
            out.background,
            Some(p10k_rs_core::style::Color::Named("red".into()))
        );
    }

    #[test]
    fn shown_on_signal_kill() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = make_ctx(&cfg, &env, 137);
        let out = Status.render(&ctx);
        assert!(out.text.contains("✘137"));
    }
}
