//! `jj` — Jujutsu VCS segment.
//!
//! Mirror of [`crate::vcs::Vcs`] for Jujutsu repositories. Shows the
//! primary bookmark (or a shortened change-id when no bookmark is set),
//! with a trailing `*` when the working copy has uncommitted changes
//! and `!` when in conflict. Disabled (skipped by the renderer) when
//! not inside a `.jj/` working copy. Producer lives in `p10k-rs-jj`.
//!
//! Visual band matches `vcs`: black-on-green so a user with both
//! `vcs` and `jj` in their layout reads the prompt as "a VCS segment"
//! regardless of which backend is active. Disambiguation is the icon
//! glyph (`f02ac` — the jujutsu / monkey wrench mark, MDI's
//! `mdi-source-pull` standing in for a dedicated jj glyph).

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default icon glyph for the `jj` segment. Nerd Font v3 `dev-git`
/// glyph — the same one `vcs` uses — keeps the visual band consistent
/// when both a jj repo and a git repo share a working tree (jj's git
/// compat backend is one such case). Override via
/// `[segment.jj].icon = "..."` if you want to disambiguate at the
/// glyph level in your TOML.
const DEFAULT_ICON: &str = "\u{e702}";

/// Length of the abbreviated change-id we render when no bookmark
/// is set. Matches jj's own `change_id.short()` length, which is
/// what the producer sends us — slicing it would be redundant work,
/// but we cap the prefix here so a future producer change can't
/// blow out the prompt width.
const CHANGE_ID_PREFIX_LEN: usize = 8;

/// Jujutsu segment: shows current bookmark / change-id + dirty marker.
#[derive(Debug, Default)]
pub struct Jj;

impl Segment for Jj {
    fn name(&self) -> &'static str {
        "jj"
    }

    fn enabled(&self, ctx: &RenderCtx<'_>) -> bool {
        ctx.jj.is_some()
    }

    fn is_fast(&self) -> bool {
        // Shell-out backend spawns `jj` twice (log + status). Definitely
        // not "fast"; a future daemon analogue would flip this to true.
        false
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let Some(jj) = ctx.jj else {
            return SegmentOutput::default();
        };

        // Label: prefer the bookmark name (jj's branch analogue). Fall
        // back to a short change-id so the prompt doesn't go blank on a
        // freshly-created change.
        let label = if jj.bookmark.is_empty() {
            jj.change_id
                .as_str()
                .chars()
                .take(CHANGE_ID_PREFIX_LEN)
                .collect::<String>()
        } else {
            jj.bookmark.as_str().to_owned()
        };

        let mut plain = String::with_capacity(label.len() + 8);
        plain.push_str(&label);
        // Dirty marker — same shape as `vcs.rs`, painted red over the
        // segment's head_fg band so it reads as an alarm subsegment.
        let dirty_marker_offset = if jj.dirty {
            plain.push(' ');
            let off = plain.len();
            plain.push('*');
            Some(off)
        } else {
            None
        };
        if jj.conflicts {
            plain.push_str(" !");
        }

        // State resolution mirrors the vcs segment's precedence chain.
        let state = if jj.conflicts {
            "conflict"
        } else if jj.dirty {
            "dirty"
        } else if jj.divergent {
            "diverged"
        } else {
            "clean"
        };

        let icon = style::resolve_icon(ctx.config, self.name(), Some(state), DEFAULT_ICON);
        let bg_sgr = style::render_bg(
            ctx.config,
            self.name(),
            Some(state),
            Color::Named("green".into()),
        );
        let head_fg = style::render_fg(
            ctx.config,
            self.name(),
            Some(state),
            Color::Named("black".into()),
        );
        let reset_fg = style::reset_fg();
        let reset_bg = style::reset_bg();
        let red = "\x1b[31m";

        let mut text = format!("{bg_sgr}{head_fg}{icon} ");
        if let Some(off) = dirty_marker_offset {
            text.push_str(&plain[..off]);
            text.push_str(red);
            text.push('*');
            text.push_str(&head_fg);
            text.push_str(&plain[off + '*'.len_utf8()..]);
        } else {
            text.push_str(&plain);
        }
        text.push_str(reset_fg);
        text.push_str(reset_bg);

        // plain_len: icon (1 cell) + 1 space + visible chars.
        let plain_len = u16::try_from(plain.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2);

        SegmentOutput {
            text,
            plain_len,
            state: Some(state),
            icon: Some(DEFAULT_ICON),
            background: Some(Color::Named("green".into())),
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::{Config, EnvSnapshot, HostKind, JjState, RenderCtx, Segment, Shell};

    use super::Jj;

    fn ctx_with_jj<'a>(
        cfg: &'a Config,
        env: &'a EnvSnapshot,
        cwd: &'a Path,
        jj: Option<&'a JjState>,
    ) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd,
            git: None,
            jj,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
            upcoming_command: "",
        }
    }

    #[test]
    fn name_is_jj() {
        assert_eq!(Jj.name(), "jj");
    }

    #[test]
    fn disabled_when_not_in_jj_repo() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = ctx_with_jj(&cfg, &env, Path::new("/"), None);
        assert!(!Jj.enabled(&ctx));
    }

    #[test]
    fn renders_bookmark_clean() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let jj = JjState {
            bookmark: "main".into(),
            ..Default::default()
        };
        let ctx = ctx_with_jj(&cfg, &env, Path::new("/"), Some(&jj));
        let out = Jj.render(&ctx);
        assert!(
            out.text.contains("main"),
            "bookmark missing: {:?}",
            out.text
        );
        assert!(
            !out.text.contains('*'),
            "no dirty `*` on clean: {:?}",
            out.text
        );
        assert_eq!(out.state, Some("clean"));
    }

    #[test]
    fn renders_dirty_bookmark_with_marker() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let jj = JjState {
            bookmark: "feat/widget".into(),
            dirty: true,
            ..Default::default()
        };
        let ctx = ctx_with_jj(&cfg, &env, Path::new("/"), Some(&jj));
        let out = Jj.render(&ctx);
        assert!(
            out.text.contains("feat/widget"),
            "bookmark missing: {:?}",
            out.text
        );
        assert!(out.text.contains('*'), "dirty `*` missing: {:?}", out.text);
        assert!(
            out.text.contains("\x1b[31m"),
            "red SGR around `*` missing: {:?}",
            out.text
        );
        assert_eq!(out.state, Some("dirty"));
    }

    #[test]
    fn renders_change_id_when_no_bookmark() {
        // No bookmark → fall back to the short change-id.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let jj = JjState {
            change_id: "abcdef12".into(),
            ..Default::default()
        };
        let ctx = ctx_with_jj(&cfg, &env, Path::new("/"), Some(&jj));
        let out = Jj.render(&ctx);
        assert!(
            out.text.contains("abcdef12"),
            "change-id missing: {:?}",
            out.text
        );
    }

    #[test]
    fn renders_conflict_marker() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let jj = JjState {
            bookmark: "main".into(),
            conflicts: true,
            ..Default::default()
        };
        let ctx = ctx_with_jj(&cfg, &env, Path::new("/"), Some(&jj));
        let out = Jj.render(&ctx);
        assert!(
            out.text.contains('!'),
            "conflict `!` missing: {:?}",
            out.text
        );
        assert_eq!(out.state, Some("conflict"));
    }
}
