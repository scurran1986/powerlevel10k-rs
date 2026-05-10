//! `vcs` — version-control segment.
//!
//! Yellow branch name, with a trailing `*` when the working tree is dirty.
//! Disabled (skipped by the renderer) when not in a repo. ADR-0001's daemon
//! client will replace the producer behind [`RenderCtx::git`] later; this
//! segment doesn't change when that swap happens.

use std::fmt::Write;

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
            };
        };

        // Build the plain (display-width) version first; then wrap with
        // ANSI escapes. Format: `branch [+ahead] [-behind] [marker]`.
        // Marker is `!` if there are unmerged conflicts, else `*` if any
        // uncommitted change. Clean repos have no marker.
        let mut plain = String::with_capacity(git.branch.len() + 16);
        plain.push_str(&git.branch);
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

        let plain_len = u16::try_from(plain.chars().count()).unwrap_or(u16::MAX);

        // Color: yellow base. Override the marker red so dirty/conflict
        // pops without re-coloring the whole branch line. ANSI 33 yellow,
        // 31 red, 39 default-fg.
        let text = if marker.is_empty() {
            format!("\x1b[33m{plain}\x1b[39m")
        } else {
            // Split the marker off so we can red-paint just it.
            let split = plain.len() - marker.len();
            let head = &plain[..split];
            let tail = marker;
            format!("\x1b[33m{head}\x1b[31m{tail}\x1b[39m")
        };

        let state = if git.has_conflicts {
            "conflict"
        } else if git.dirty {
            "dirty"
        } else if git.ahead > 0 || git.behind > 0 {
            "diverged"
        } else {
            "clean"
        };
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
            ..Default::default()
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
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(out.text.contains("feat/x"));
        assert!(out.text.contains('*'));
        assert!(out.text.contains("\x1b[31m")); // marker in red
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
}
