//! `status` — last command exit code when non-zero.
//!
//! Hidden on success (the most common case). On failure shows `✘<code>`
//! painted white-on-red (P10K-classic palette). Visible alongside the red
//! `prompt_char` so the user sees both "the prompt knows it failed"
//! (chevron) and "what code did it exit with" (this segment) — useful for
//! `137` (OOM-kill), `130` (SIGINT), arbitrary user codes, etc.

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (x-mark — reads as "fail" since the segment
/// only renders on non-zero exit). Override via
/// `[segment.status].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{f00d}";

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
        // Slice 34: the segment only renders on non-zero exit, so the
        // state tag is always `"error"` — passing it through here lets
        // `[segment.status.states.error]` TOML overrides hit. Future "ok"
        // / "warning" indicators (when the segment learns to render on
        // success) would pick up their own state strings from this site.
        let state_tag = "error";
        let icon = style::resolve_icon(ctx.config, self.name(), Some(state_tag), DEFAULT_ICON);
        let plain_len = u16::try_from(plain.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2); // icon + space
        let bg = style::render_bg(
            ctx.config,
            self.name(),
            Some(state_tag),
            Color::Named("red".into()),
        );
        let fg = style::render_fg(
            ctx.config,
            self.name(),
            Some(state_tag),
            Color::Named("white".into()),
        );
        let text = format!(
            "{bg}{fg}{icon} {plain}{}{}",
            style::reset_fg(),
            style::reset_bg()
        );
        SegmentOutput {
            text,
            plain_len,
            state: Some("error"),
            icon: Some(DEFAULT_ICON),
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
            upcoming_command: "",
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

    #[test]
    fn renders_with_default_icon() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = make_ctx(&cfg, &env, 1);
        let out = Status.render(&ctx);
        assert!(
            out.text.contains('\u{f00d}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{f00d}"));
    }

    #[test]
    fn error_state_toml_override_fires() {
        // Slice 34: confirm `state = Some("error")` is actually plumbed
        // through the style helpers so per-state TOML overrides target it
        // cleanly. A user pinning the error background to magenta should
        // beat the red default.
        let cfg = Config::from_toml(
            "schema_version = 1\n\
             [segment.status.states.error]\n\
             background = \"magenta\"\n",
        )
        .expect("valid toml");
        let env = EnvSnapshot::default();
        let ctx = make_ctx(&cfg, &env, 1);
        let out = Status.render(&ctx);
        // magenta bg (`48;5;5`) — TOML override beat the red default.
        assert!(
            out.text.contains("\x1b[48;5;5m"),
            "magenta override missing: {:?}",
            out.text
        );
        assert_eq!(out.state, Some("error"));
    }
}
