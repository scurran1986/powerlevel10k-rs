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
        // ANSI escapes. Upstream P10K order:
        //   `<branch> ⇡<ahead> ⇣<behind> *<dirty> !<conflicts> \
        //    +<staged> ~<unstaged> ?<untracked>`
        // The bare `*` dirty marker still renders, but only when no
        // `+ ~ ?` count indicators are present — those counts are
        // themselves a dirty signal, so the bare `*` becomes redundant.
        // The `*` marker stays hardcoded red (load-bearing test pin);
        // every other indicator inherits the segment's fg/bg band.
        let any_change_counts = git.staged > 0 || git.unstaged > 0 || git.untracked > 0;
        let show_dirty_marker = git.dirty && !any_change_counts;

        let mut plain = String::with_capacity(git.branch.len() + 32);
        plain.push_str(git.branch.as_str());
        if git.ahead > 0 {
            let _ = write!(plain, " \u{21e1}{}", git.ahead);
        }
        if git.behind > 0 {
            let _ = write!(plain, " \u{21e3}{}", git.behind);
        }
        // Track byte offset of the red-painted `*` marker so the ANSI
        // wrapper can split the plain string at that exact point.
        let dirty_marker_offset = if show_dirty_marker {
            plain.push(' ');
            let off = plain.len();
            plain.push('*');
            Some(off)
        } else {
            None
        };
        // Conflicts marker `!` lives in the segment's fg band (no red
        // override — the `dirty *` is the only hardcoded-red subsegment).
        if git.has_conflicts {
            plain.push_str(" !");
        }
        if git.staged > 0 {
            let _ = write!(plain, " +{}", git.staged);
        }
        if git.unstaged > 0 {
            let _ = write!(plain, " ~{}", git.unstaged);
        }
        if git.untracked > 0 {
            let _ = write!(plain, " ?{}", git.untracked);
        }
        // Slice 45: in-progress repo action (merge / rebase / cherry-pick /
        // revert / bisect) surfaces in upper case, painted red so it reads
        // as an alarm-state indicator over the segment's head_fg band. We
        // track the byte offset so the ANSI wrapper can splice in a red
        // SGR exactly around the action label and resume head_fg for any
        // trailing stash indicator.
        let action_marker = if !git.action.as_str().is_empty() {
            plain.push(' ');
            let start = plain.len();
            for ch in git.action.as_str().chars() {
                for upper in ch.to_uppercase() {
                    plain.push(upper);
                }
            }
            Some((start, plain.len()))
        } else {
            None
        };
        // Slice 45: stash count. Glyph `≡` (U+2261) avoids colliding with
        // the dirty `*` and the staged `+`. Painted in the segment's
        // head_fg band like every other index-level count.
        if git.stash > 0 {
            let _ = write!(plain, " \u{2261}{}", git.stash);
        }

        // Compute state first so we can pass it to the style resolver below.
        // Index-level changes (`staged/unstaged/untracked`) count as
        // dirty even if the consumer didn't set the `dirty` bool.
        let state = if git.has_conflicts {
            "conflict"
        } else if git.dirty || any_change_counts {
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
        // Prepend `icon + space` inside the bg/head_fg colour band, then
        // splice red SGRs around the two attacker-style alarm subsegments
        // we paint over the head_fg band:
        //   - dirty `*` marker (single char, slice 28)
        //   - in-progress repo action label (multi-char, slice 45)
        // Both keep the green bg band; only the fg flips to red for the
        // marker span, then head_fg picks back up. The "stash count"
        // surface stays inside head_fg — it's informational, not alarm.
        let red = "\x1b[31m";
        let mut text = format!("{bg}{head_fg}{icon} ");
        // Collect the highlight spans in `plain`-byte order so we can walk
        // them once. Both spans are non-overlapping and pre-sorted: dirty
        // `*` lives before staged/unstaged/untracked counts, which live
        // before the action label.
        let mut spans: Vec<(usize, usize)> = Vec::with_capacity(2);
        if let Some(off) = dirty_marker_offset {
            spans.push((off, off + '*'.len_utf8()));
        }
        if let Some((s, e)) = action_marker {
            spans.push((s, e));
        }
        let mut cursor = 0;
        for (start, end) in spans {
            text.push_str(&plain[cursor..start]);
            text.push_str(red);
            text.push_str(&plain[start..end]);
            text.push_str(&head_fg);
            cursor = end;
        }
        text.push_str(&plain[cursor..]);
        text.push_str(reset_fg);
        text.push_str(reset_bg);

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
            upcoming_command: "",
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
    fn renders_ahead_behind_counts() {
        // Slice 30: upstream P10K uses ⇡N / ⇣N glyphs (U+21E1 / U+21E3)
        // rather than the placeholder `+N` / `-N` we shipped earlier.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "main".into(),
            ahead: 3,
            behind: 1,
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(out.text.contains("\u{21e1}3"), "missing ⇡3: {:?}", out.text);
        assert!(out.text.contains("\u{21e3}1"), "missing ⇣1: {:?}", out.text);
        assert_eq!(out.state, Some("diverged"));
    }

    #[test]
    fn renders_staged_unstaged_untracked() {
        // Slice 30: index-level change counts surface as +N ~N ?N
        // (P10K-canonical). The bare `*` is redundant when any of these
        // are present, so it must NOT render here.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "main".into(),
            dirty: true,
            staged: 2,
            unstaged: 4,
            untracked: 1,
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(out.text.contains("+2"), "missing +2: {:?}", out.text);
        assert!(out.text.contains("~4"), "missing ~4: {:?}", out.text);
        assert!(out.text.contains("?1"), "missing ?1: {:?}", out.text);
        assert!(
            !out.text.contains('*'),
            "redundant `*` should be suppressed when counts are present: {:?}",
            out.text,
        );
        assert_eq!(out.state, Some("dirty"));
    }

    #[test]
    fn renders_conflicts_indicator() {
        // Slice 30: unmerged conflicts show as a trailing `!`. The
        // state still resolves to `conflict` and the dirty `*` is
        // allowed to coexist (upstream P10K renders both).
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "main".into(),
            has_conflicts: true,
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(
            out.text.contains('!'),
            "missing conflicts !: {:?}",
            out.text
        );
        assert_eq!(out.state, Some("conflict"));
    }

    #[test]
    fn renders_stash_count() {
        // Slice 45: stash surfaces as ` ≡<n>` after the index-level counts.
        // Painted inside the head_fg band (no red splice) since it's
        // informational, not an alarm-state indicator.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "main".into(),
            stash: 2,
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(out.text.contains("\u{2261}2"), "missing ≡2: {:?}", out.text);
    }

    #[test]
    fn renders_merge_action() {
        // Slice 45: in-progress action surfaces upper-cased, painted red
        // over the head_fg band. State still resolves to whatever the
        // underlying GitState signals — `action` is orthogonal.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "main".into(),
            action: "merge".into(),
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(
            out.text.contains("MERGE"),
            "missing upper-case MERGE: {:?}",
            out.text
        );
        // Red SGR must precede the action label so it reads as an alarm
        // over the segment's head_fg band.
        let merge_idx = out.text.find("MERGE").expect("MERGE present");
        assert!(
            out.text[..merge_idx].contains("\x1b[31m"),
            "missing red SGR before MERGE: {:?}",
            out.text,
        );
    }

    #[test]
    fn renders_rebase_action_and_stash_together() {
        // Belt and braces: action + stash co-exist; both surface, the
        // action label flips red and the stash stays in head_fg.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "feat/x".into(),
            action: "rebase".into(),
            stash: 3,
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(
            out.text.contains("REBASE"),
            "missing REBASE: {:?}",
            out.text
        );
        assert!(out.text.contains("\u{2261}3"), "missing ≡3: {:?}", out.text);
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
