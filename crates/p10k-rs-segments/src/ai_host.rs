//! `ai_host` — AI coding-assistant host segment.
//!
//! Surfaces the active AI host (Claude Code, Aider, Cursor, …) as a
//! magenta band on the prompt so it's immediately obvious you're inside
//! an agent shell. Detection lives in `p10k-rs-ai`; this segment just
//! reads `ctx.host` and labels accordingly.
//!
//! Hidden when [`HostKind::None`].

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{HostKind, RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (cog). Override via `[segment.ai_host].icon`.
const DEFAULT_ICON: &str = "\u{f085}";

/// AI host segment.
///
/// Renders the host label (`claude-code`, `aider`, `cursor`) in white on
/// magenta. Hidden when no AI host is detected.
#[derive(Debug, Default)]
pub struct AiHost;

impl AiHost {
    /// Map a [`HostKind`] to the display label rendered next to the icon.
    ///
    /// `HostKind` is `#[non_exhaustive]`, so the wildcard arm is required
    /// for cross-crate matches. Unknown / `None` variants return `None`
    /// and the segment hides via [`Segment::enabled`].
    fn label(host: &HostKind) -> Option<&'static str> {
        match host {
            HostKind::ClaudeCode => Some("claude-code"),
            HostKind::Aider => Some("aider"),
            HostKind::Cursor => Some("cursor"),
            HostKind::Goose => Some("goose"),
            // `HostKind` is `#[non_exhaustive]` — covers both `None` and
            // any future variant cross-crate.
            _ => None,
        }
    }
}

impl Segment for AiHost {
    fn name(&self) -> &'static str {
        "ai_host"
    }

    fn enabled(&self, ctx: &RenderCtx<'_>) -> bool {
        Self::label(&ctx.host).is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let Some(label) = Self::label(&ctx.host) else {
            return SegmentOutput::default();
        };
        let icon = style::resolve_icon(ctx.config, self.name(), None, DEFAULT_ICON);
        let bg = style::render_bg(
            ctx.config,
            self.name(),
            None,
            Color::Named("magenta".into()),
        );
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("white".into()));
        let text = format!(
            "{bg}{fg}{icon} {label}{}{}",
            style::reset_fg(),
            style::reset_bg()
        );
        let plain_len = u16::try_from(label.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: Some(DEFAULT_ICON),
            background: Some(style::resolve_bg(
                ctx.config,
                self.name(),
                None,
                Color::Named("magenta".into()),
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

    use super::AiHost;

    fn ctx<'a>(cfg: &'a Config, env: &'a EnvSnapshot, host: HostKind) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host,
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

    #[test]
    fn name_is_ai_host() {
        assert_eq!(AiHost.name(), "ai_host");
    }

    #[test]
    fn hidden_when_host_is_none() {
        let cfg = Config::default();
        let env = EnvSnapshot::default();
        assert!(!AiHost.enabled(&ctx(&cfg, &env, HostKind::None)));
    }

    #[test]
    fn renders_claude_code_label() {
        let cfg = Config::default();
        let env = EnvSnapshot::default();
        let out = AiHost.render(&ctx(&cfg, &env, HostKind::ClaudeCode));
        assert!(out.text.contains("claude-code"), "label: {:?}", out.text);
        assert!(out.text.contains('\u{f085}'), "icon: {:?}", out.text);
    }

    #[test]
    fn renders_aider_label() {
        let cfg = Config::default();
        let env = EnvSnapshot::default();
        let out = AiHost.render(&ctx(&cfg, &env, HostKind::Aider));
        assert!(out.text.contains("aider"));
    }

    #[test]
    fn renders_cursor_label() {
        let cfg = Config::default();
        let env = EnvSnapshot::default();
        let out = AiHost.render(&ctx(&cfg, &env, HostKind::Cursor));
        assert!(out.text.contains("cursor"));
    }

    #[test]
    fn renders_goose_label() {
        let cfg = Config::default();
        let env = EnvSnapshot::default();
        let out = AiHost.render(&ctx(&cfg, &env, HostKind::Goose));
        assert!(out.text.contains("goose"));
    }
}
