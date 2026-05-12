//! `prompt_char` — the trailing chevron the user types after.
//!
//! Green `❯` when the previous command exited 0, red `❯` when it exited
//! non-zero. The exit status arrives via `RenderCtx::last_status`, which the
//! binary fills from the `--last-status` CLI arg, which the zsh init script
//! captures from `$?` at the top of its `precmd` hook.
//!
//! Vi-mode variants will land later.

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Trailing prompt character.
#[derive(Debug, Default)]
pub struct PromptChar;

impl Segment for PromptChar {
    fn name(&self) -> &'static str {
        "prompt_char"
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let state = if ctx.last_status == 0 { "ok" } else { "error" };
        let default = if state == "ok" {
            Color::Named("green".into())
        } else {
            Color::Named("red".into())
        };
        let fg = style::render_fg(ctx.config, self.name(), Some(state), default);
        let text = format!("{fg}❯{}", style::reset_fg());
        SegmentOutput {
            text,
            plain_len: 1,
            state: Some(state),
            icon: None,
            // No background — `prompt_char` lives on line 2 and renders
            // inline without a powerline ribbon block.
            background: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::PromptChar;

    fn defaults() -> (Config, EnvSnapshot) {
        (Config::default(), EnvSnapshot::default())
    }

    fn make_ctx<'a>(
        cfg: &'a Config,
        env: &'a EnvSnapshot,
        cwd: &'a Path,
        last_status: i32,
    ) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd,
            git: None,
            last_status,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
            upcoming_command: "",
        }
    }

    #[test]
    fn green_on_success() {
        let (cfg, env) = defaults();
        let ctx = make_ctx(&cfg, &env, Path::new("/"), 0);
        let out = PromptChar.render(&ctx);
        assert!(
            out.text.contains("\x1b[38;5;2m"),
            "expected green: {:?}",
            out.text
        );
        assert_eq!(out.state, Some("ok"));
    }

    #[test]
    fn red_on_failure() {
        let (cfg, env) = defaults();
        let ctx = make_ctx(&cfg, &env, Path::new("/"), 1);
        let out = PromptChar.render(&ctx);
        assert!(
            out.text.contains("\x1b[38;5;1m"),
            "expected red: {:?}",
            out.text
        );
        assert_eq!(out.state, Some("error"));
    }

    #[test]
    fn red_on_signal_kill() {
        // Shells encode signals as 128+N. Treat anything non-zero as failure.
        let (cfg, env) = defaults();
        let ctx = make_ctx(&cfg, &env, Path::new("/"), 130);
        let out = PromptChar.render(&ctx);
        assert!(out.text.contains("\x1b[38;5;1m"));
    }
}
