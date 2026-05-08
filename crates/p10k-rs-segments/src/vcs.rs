//! `vcs` — version-control segment.
//!
//! Slice 4: yellow branch name, with a trailing `*` when the working tree is
//! dirty. Disabled (skipped by the renderer) when not in a repo. ADR-0001's
//! daemon client lands in slice 5+ and replaces the producer behind
//! [`RenderCtx::git`]; this segment doesn't change when that swap happens.

use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

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
        // Shell-out backend (slice 4) spawns `git` — definitely not "fast".
        // Daemon backend in slice 5+ flips this to true.
        false
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let Some(git) = ctx.git else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
            };
        };
        let dirty = if git.dirty { "*" } else { "" };
        let plain = format!("{}{}", git.branch, dirty);
        let plain_len = u16::try_from(plain.chars().count()).unwrap_or(u16::MAX);
        // 33 = yellow.
        let text = format!("\x1b[33m{plain}\x1b[39m");
        let state = if git.dirty { "dirty" } else { "clean" };
        SegmentOutput {
            text,
            plain_len,
            state: Some(state),
            icon: None,
        }
    }
}

#[cfg(test)]
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
            dirty: false,
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(out.text.contains("main"));
        assert!(!out.text.contains('*'));
        assert!(out.text.contains("\x1b[33m"));
        assert_eq!(out.state, Some("clean"));
    }

    #[test]
    fn renders_dirty_marker() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "feat/x".into(),
            dirty: true,
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(out.text.contains("feat/x*"));
        assert_eq!(out.state, Some("dirty"));
    }
}
