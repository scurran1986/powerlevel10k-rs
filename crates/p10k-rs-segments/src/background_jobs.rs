//! `background_jobs` — count of suspended/running background jobs.
//!
//! Hidden when the shell has no jobs (the common case). When at least one
//! job exists, shows `⚙N` in cyan to match upstream p10k's default colour.
//!
//! Job count arrives via [`RenderCtx::jobs`], which the binary fills from
//! the `--jobs` CLI arg. The zsh init script captures it via `$#jobstates`
//! at prompt-render time.

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Background-jobs segment.
#[derive(Debug, Default)]
pub struct BackgroundJobs;

impl Segment for BackgroundJobs {
    fn name(&self) -> &'static str {
        "background_jobs"
    }

    fn enabled(&self, ctx: &RenderCtx<'_>) -> bool {
        ctx.jobs > 0
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let count = ctx.jobs;
        let plain = format!("⚙{count}");
        let plain_len = u16::try_from(plain.chars().count()).unwrap_or(u16::MAX);
        let bg = style::render_bg(ctx.config, self.name(), None, Color::Named("cyan".into()));
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("black".into()));
        let text = format!("{bg}{fg}{plain}{}{}", style::reset_fg(), style::reset_bg());
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: None,
            background: Some(Color::Named("cyan".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::style::Color;
    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::BackgroundJobs;

    fn make_ctx<'a>(cfg: &'a Config, env: &'a EnvSnapshot, jobs: u32) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: Path::new("/"),
            git: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs,
            now: SystemTime::UNIX_EPOCH,
            env,
        }
    }

    #[test]
    fn hidden_when_no_jobs() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = make_ctx(&cfg, &env, 0);
        assert!(!BackgroundJobs.enabled(&ctx));
    }

    #[test]
    fn shown_when_one_job() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = make_ctx(&cfg, &env, 1);
        assert!(BackgroundJobs.enabled(&ctx));
        let out = BackgroundJobs.render(&ctx);
        assert!(out.text.contains("⚙1"));
        // Powerline ribbon: cyan bg + black fg, with both resets present.
        assert!(out.text.contains("\x1b[48;5;6m"));
        assert!(out.text.contains("\x1b[38;5;0m"));
        assert!(out.text.contains("\x1b[49m"));
        assert_eq!(out.background, Some(Color::Named("cyan".into())));
    }

    #[test]
    fn shown_with_many_jobs() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = make_ctx(&cfg, &env, 5);
        let out = BackgroundJobs.render(&ctx);
        assert!(out.text.contains("⚙5"));
    }
}
