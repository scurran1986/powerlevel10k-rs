//! `vcs` — version-control segment.
//!
//! Branch name painted black-on-green (P10K-classic palette), with a trailing
//! `*` when the working tree is dirty. The dirty marker stays hardcoded red
//! per the project's load-bearing test pin — it's a subsegment, not the
//! segment-level fg. Disabled (skipped by the renderer) when not in a repo.
//! ADR-0001's daemon client will replace the producer behind
//! [`RenderCtx::git`] later; this segment doesn't change when that swap
//! happens.

use std::fmt::Write;

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default icon glyph for the vcs segment. Nerd Font v3 git glyph.
/// Overridable via `[segment.vcs].icon = "..."` in the user's TOML config.
const DEFAULT_ICON: &str = "\u{f1d3}";

/// Version-control segment: shows current branch + dirty marker.
#[derive(Debug, Default)]
pub struct Vcs;

impl Segment for Vcs {
    fn name(&self) -> &'static str {
        "vcs"
    }

    fn enabled(&self, ctx: &RenderCtx<'_>) -> bool {
        ctx.git.is_some()
    }

    fn is_fast(&self) -> bool {
        // Shell-out backend spawns `git` — definitely not "fast".
        // The daemon backend will flip this to true.
        false
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let Some(git) = ctx.git else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
                background: None,
            };
        };

        // Build the plain (display-width) version first; then wrap with
        // ANSI escapes. Format: `branch [+ahead] [-behind] [marker]`.
        // Marker is `!` if there are unmerged conflicts, else `*` if any
        // uncommitted change. Clean repos have no marker.
        let mut plain = String::with_capacity(git.branch.len() + 16);
        plain.push_str(git.branch.as_str());
        if git.ahead > 0 {
            let _ = write!(plain, " +{}", git.ahead);
        }
        if git.behind > 0 {
            let _ = write!(plain, " -{}", git.behind);
        }
        let marker = if git.has_conflicts {
            "!"
        } else if git.dirty {
            "*"
        } else {
            ""
        };
        if !marker.is_empty() {
            plain.push(' ');
            plain.push_str(marker);
        }

        // Compute state first so we can pass it to the style resolver below.
        let state = if git.has_conflicts {
            "conflict"
        } else if git.dirty {
            "dirty"
        } else if git.ahead > 0 || git.behind > 0 {
            "diverged"
        } else {
            "clean"
        };

        // Resolve the icon glyph through the state-aware precedence chain:
        // state-keyed override → segment-level override → Nerd Font default.
        // Painted inside the head_fg colour band so it tracks the branch
        // colour (and the per-state foreground override).
        let icon = style::resolve_icon(ctx.config, self.name(), Some(state), DEFAULT_ICON);

        // Head colour goes through the config-aware style resolver; default
        // is black-on-green (P10K-classic palette) when no override is
        // configured. Marker stays hardcoded red — it's a subsegment, not
        // the segment-level fg, and threading it through config would
        // conflict with the single per-state fg field. Future slice can
        // add separate marker control.
        let bg = style::render_bg(
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
        // Prepend `icon + space` inside the bg/head_fg colour band.
        let text = if marker.is_empty() {
            format!("{bg}{head_fg}{icon} {plain}{reset_fg}{reset_bg}")
        } else {
            // Split the marker off so we can red-paint just it. The marker
            // stays inside the green bg band; only the fg flips to red.
            let split = plain.len() - marker.len();
            let head = &plain[..split];
            let tail = marker;
            format!("{bg}{head_fg}{icon} {head}\x1b[31m{tail}{reset_fg}{reset_bg}")
        };

        // plain_len accounts for the icon glyph (1 display cell) + 1 space.
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

    use p10k_rs_core::{Config, EnvSnapshot, GitState, HostKind, RenderCtx, Segment, Shell};

    use super::Vcs;

    fn ctx_with_git<'a>(
        cfg: &'a Config,
        env: &'a EnvSnapshot,
        cwd: &'a Path,
        git: Option<&'a GitState>,
    ) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd,
            git,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
        }
    }

    #[test]
    fn disabled_when_not_in_repo() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), None);
        assert!(!Vcs.enabled(&ctx));
    }

    #[test]
    fn renders_branch_clean() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "main".into(),
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(out.text.contains("main"));
        assert!(!out.text.contains('*'));
        // Slice 28A: P10K-classic palette is black-on-green.
        assert!(
            out.text.contains("\x1b[38;5;0m"),
            "black fg: {:?}",
            out.text
        );
        assert!(
            out.text.contains("\x1b[48;5;2m"),
            "green bg: {:?}",
            out.text
        );
        assert!(out.text.contains('\u{f1d3}'));
        assert_eq!(out.state, Some("clean"));
        assert_eq!(
            out.background,
            Some(p10k_rs_core::style::Color::Named("green".into()))
        );
    }

    #[test]
    fn renders_dirty_marker() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "feat/x".into(),
            dirty: true,
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(out.text.contains("feat/x"));
        assert!(out.text.contains('*'));
        assert!(out.text.contains("\x1b[31m")); // marker in red
        assert!(out.text.contains('\u{f1d3}'));
        assert_eq!(out.state, Some("dirty"));
    }

    #[test]
    fn renders_ahead_and_behind() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "main".into(),
            ahead: 3,
            behind: 1,
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(out.text.contains("+3"));
        assert!(out.text.contains("-1"));
        assert_eq!(out.state, Some("diverged"));
    }

    #[test]
    fn renders_conflict_takes_priority_over_dirty() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "main".into(),
            dirty: true,
            has_conflicts: true,
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(out.text.contains('!'));
        assert!(!out.text.contains('*'));
        assert_eq!(out.state, Some("conflict"));
    }

    #[test]
    fn state_keyed_icon_override_fires_on_dirty() {
        // Slice 24: per-state icon override. The `dirty` state must
        // resolve `[segment.vcs.states.dirty].icon` over the
        // segment-level default.
        let cfg = p10k_rs_core::Config::from_toml(
            "schema_version = 1\n\
             [segment.vcs.states.dirty]\n\
             icon = \"!!\"\n",
        )
        .expect("fixture parses");
        let env = EnvSnapshot::default();
        let dirty = GitState {
            branch: "feat/x".into(),
            dirty: true,
            ..Default::default()
        };
        let clean = GitState {
            branch: "main".into(),
            ..Default::default()
        };

        let out_dirty = Vcs.render(&ctx_with_git(&cfg, &env, Path::new("/"), Some(&dirty)));
        assert!(
            out_dirty.text.contains("!!"),
            "state-keyed icon override missing: {:?}",
            out_dirty.text
        );
        assert!(
            !out_dirty.text.contains('\u{f1d3}'),
            "default vcs icon should be replaced for dirty state: {:?}",
            out_dirty.text
        );

        // Clean state still gets the Nerd Font default since no
        // segment-level fallback was configured.
        let out_clean = Vcs.render(&ctx_with_git(&cfg, &env, Path::new("/"), Some(&clean)));
        assert!(
            out_clean.text.contains('\u{f1d3}'),
            "clean state must still render the default icon: {:?}",
            out_clean.text
        );
    }
}
