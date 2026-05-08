//! `prompt_char` — the trailing chevron the user types after.
//!
//! Slice 3: green `❯` when the previous command exited 0, red `❯` when it
//! exited non-zero. The exit status arrives via `RenderCtx::last_status`,
//! which the binary fills from the `--last-status` CLI arg, which the zsh
//! init script captures from `$?` at the top of its `precmd` hook.
//!
//! Vi-mode variants land in later slices.

use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Trailing prompt character.
#[derive(Debug, Default)]
pub struct PromptChar;

impl Segment for PromptChar {
    fn name(&self) -> &'static str {
        "prompt_char"
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // 32 = ANSI green (success), 31 = red (failure). 39 = default-fg.
        let (text, state) = if ctx.last_status == 0 {
            ("\x1b[32m❯\x1b[39m".to_owned(), "ok")
        } else {
            ("\x1b[31m❯\x1b[39m".to_owned(), "error")
        };
        SegmentOutput {
            text,
            plain_len: 1,
            state: Some(state),
            icon: None,
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
        }
    }

    #[test]
    fn green_on_success() {
        let (cfg, env) = defaults();
        let ctx = make_ctx(&cfg, &env, Path::new("/"), 0);
        let out = PromptChar.render(&ctx);
        assert!(
            out.text.contains("\x1b[32m"),
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
            out.text.contains("\x1b[31m"),
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
        assert!(out.text.contains("\x1b[31m"));
    }
}
