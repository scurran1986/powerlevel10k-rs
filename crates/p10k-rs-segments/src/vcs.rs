//! `vcs` — version-control segment.
//!
//! Branch name painted black-on-green (P10K-classic palette), with a trailing
//! `*` when the working tree is dirty. The dirty marker and action label colour
//! defaults to red; users can override it via `[segment.vcs].marker_foreground`
//! in their TOML config. Disabled (skipped by the renderer) when not in a repo.
//! ADR-0001's daemon client will replace the producer behind
//! [`RenderCtx::git`] later; this segment doesn't change when that swap
//! happens.

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default icon glyph for the vcs segment. Nerd Font v3 git glyph.
/// Overridable via `[segment.vcs].icon = "..."` in the user's TOML config.
const DEFAULT_ICON: &str = "\u{f1d3}";

/// Glyph used to mark a detached-HEAD render (Nerd Font `mdi-source-commit`).
/// Prepended to the 7-char SHA prefix so the prompt visually announces "you
/// are not on a branch" rather than silently swapping a branch name for hex.
const DETACHED_HEAD_GLYPH: &str = "\u{f00ec}";

/// Length of the abbreviated SHA we render in detached-HEAD mode. Matches
/// `git`'s default short-OID width and upstream P10K's `VCS_COMMIT_HASH`
/// truncation. Source `commit` is already `SafeText` — ASCII hex passes
/// through sanitisation unchanged.
const SHA_PREFIX_LEN: usize = 7;

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

        let any_change_counts = git.staged > 0 || git.unstaged > 0 || git.untracked > 0;
        let sub_sep = ctx
            .config
            .layout
            .separators
            .subsegment
            .as_deref()
            .unwrap_or(" ");
        let (plain, dirty_marker_offset, action_marker) =
            build_plain_text(git, sub_sep, any_change_counts);

        // State: conflict > dirty > diverged > clean.
        let state = if git.has_conflicts {
            "conflict"
        } else if git.dirty || any_change_counts {
            "dirty"
        } else if git.ahead > 0 || git.behind > 0 {
            "diverged"
        } else {
            "clean"
        };

        let icon = style::resolve_icon(ctx.config, self.name(), Some(state), DEFAULT_ICON);
        // Default palette: black-on-green (P10K classic). Marker colour defaults
        // to red but honours `[segment.vcs].marker_foreground` when set.
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
        let marker_fg = resolve_marker_fg(ctx.config, self.name());
        let text = paint_alarm_spans(
            &bg_sgr,
            &head_fg,
            icon,
            &plain,
            dirty_marker_offset,
            action_marker,
            &marker_fg,
        );
        let plain_len = u16::try_from(plain.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2);

        SegmentOutput {
            text,
            plain_len,
            state: Some(state),
            icon: Some(DEFAULT_ICON),
            background: Some(style::resolve_bg(
                ctx.config,
                self.name(),
                Some(state),
                Color::Named("green".into()),
            )),
        }
    }
}

/// Build the display-width plain-text string for the vcs segment.
///
/// Returns `(plain, dirty_marker_offset, action_marker)` where:
/// - `plain` is the fully assembled display string (branch/SHA, tag, counts).
/// - `dirty_marker_offset` is the byte offset of the `*` glyph when present.
/// - `action_marker` is `Some((start, end))` byte span of the action label.
///
/// Extracted from `Vcs::render` to keep that function under the 100-line limit.
fn build_plain_text(
    git: &p10k_rs_core::GitState,
    sub_sep: &str,
    any_change_counts: bool,
) -> (String, Option<usize>, Option<(usize, usize)>) {
    use std::fmt::Write as _;
    let show_dirty_marker = git.dirty && !any_change_counts;

    // Detached-HEAD: branch == "HEAD" (or empty) with a non-empty commit OID.
    let branch_label = git.branch.as_str();
    let commit_label = git.commit.as_str();
    let detached = (branch_label == "HEAD" || branch_label.is_empty()) && !commit_label.is_empty();
    let sha_prefix = if detached {
        commit_label
            .chars()
            .take(SHA_PREFIX_LEN)
            .collect::<String>()
    } else {
        String::new()
    };

    let mut plain = String::with_capacity(git.branch.len() + git.tag.len() + 32);
    if detached {
        plain.push_str(DETACHED_HEAD_GLYPH);
        plain.push_str(&sha_prefix);
    } else {
        plain.push_str(branch_label);
    }
    // Tag: ` @ <tag>` when HEAD points at a named tag (daemon field 18).
    if !git.tag.as_str().is_empty() {
        let _ = write!(plain, "{sub_sep}@ {}", git.tag.as_str());
    }
    if git.ahead > 0 {
        let _ = write!(plain, "{sub_sep}\u{21e1}{}", git.ahead);
    }
    if git.behind > 0 {
        let _ = write!(plain, "{sub_sep}\u{21e3}{}", git.behind);
    }
    // Track byte offset of the alarm-coloured `*` for paint_alarm_spans.
    let dirty_marker_offset = if show_dirty_marker {
        plain.push_str(sub_sep);
        let off = plain.len();
        plain.push('*');
        Some(off)
    } else {
        None
    };
    let action_marker = append_index_indicators(&mut plain, sub_sep, git);
    (plain, dirty_marker_offset, action_marker)
}

/// Resolve the SGR escape for vcs marker glyphs (`* ! + ~ ? ≡`).
///
/// Returns the `[segment.vcs].marker_foreground` value rendered through
/// [`style::sgr_fg`] when set; falls back to the historical hardcoded red
/// (`\x1b[31m`) when absent so existing users see no change.
fn resolve_marker_fg(
    config: &p10k_rs_config::Config,
    segment: &str,
) -> std::borrow::Cow<'static, str> {
    config
        .segments
        .get(segment)
        .and_then(|sc| sc.marker_foreground.clone())
        .map_or(std::borrow::Cow::Borrowed("\x1b[31m"), |c| {
            style::sgr_fg(&c, config.colors)
        })
}

/// Append index-level indicators to `plain`: conflicts `!`, staged `+N`,
/// unstaged `~N`, untracked `?N`, in-progress action label (upper-cased),
/// and stash `≡N`. Returns the byte-offset span of the action label
/// inside `plain` so the caller's red-SGR splice can target it.
///
/// Extracted from `Vcs::render` to keep that function under clippy's
/// `too_many_lines` threshold. Order matches upstream P10K's visual
/// convention; see slice 30 + slice 45 commit messages.
fn append_index_indicators(
    plain: &mut String,
    sub_sep: &str,
    git: &p10k_rs_core::GitState,
) -> Option<(usize, usize)> {
    use std::fmt::Write as _;
    if git.has_conflicts {
        let _ = write!(plain, "{sub_sep}!");
    }
    if git.staged > 0 {
        let _ = write!(plain, "{sub_sep}+{}", git.staged);
    }
    if git.unstaged > 0 {
        let _ = write!(plain, "{sub_sep}~{}", git.unstaged);
    }
    if git.untracked > 0 {
        let _ = write!(plain, "{sub_sep}?{}", git.untracked);
    }
    let action_marker = if git.action.as_str().is_empty() {
        None
    } else {
        plain.push_str(sub_sep);
        let start = plain.len();
        for ch in git.action.as_str().chars() {
            for upper in ch.to_uppercase() {
                plain.push(upper);
            }
        }
        Some((start, plain.len()))
    };
    if git.stash > 0 {
        let _ = write!(plain, "{sub_sep}\u{2261}{}", git.stash);
    }
    action_marker
}

/// Splice marker SGRs around the dirty `*` and in-progress action label spans
/// inside `plain`, leaving the rest of the `head_fg` colour band intact.
///
/// `marker_fg` is the SGR escape to apply to alarm spans — defaults to red
/// (`\x1b[31m`) but callers can pass a resolved `marker_foreground` override.
///
/// Extracted from `Vcs::render` to keep that function under clippy's
/// `too_many_lines` threshold. Both spans are non-overlapping and
/// pre-sorted in `plain`-byte order by the caller; this function walks
/// them once with a single cursor.
fn paint_alarm_spans(
    bg_sgr: &str,
    head_fg: &str,
    icon: &str,
    plain: &str,
    dirty_marker_offset: Option<usize>,
    action_marker: Option<(usize, usize)>,
    marker_fg: &str,
) -> String {
    let reset_fg = style::reset_fg();
    let reset_bg = style::reset_bg();
    let mut text = format!("{bg_sgr}{head_fg}{icon} ");
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
        text.push_str(marker_fg);
        text.push_str(&plain[start..end]);
        text.push_str(head_fg);
        cursor = end;
    }
    text.push_str(&plain[cursor..]);
    text.push_str(reset_fg);
    text.push_str(reset_bg);
    text
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use p10k_rs_core::safety::SafeText;
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
            cwd_display: SafeText::default(),
            git,
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

    #[test]
    fn renders_detached_head_with_sha_prefix() {
        // Slice 47: branch == "HEAD" + non-empty commit => detached. We
        // swap the branch label for `mdi-source-commit` + 7-char SHA so
        // the prompt visibly announces "no branch" instead of literally
        // showing the string "HEAD".
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "HEAD".into(),
            commit: "7b3a9f2deadbeef12345678".into(),
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(
            out.text.contains('\u{f00ec}'),
            "missing detached-HEAD glyph: {:?}",
            out.text
        );
        assert!(
            out.text.contains("7b3a9f2"),
            "missing 7-char SHA prefix: {:?}",
            out.text
        );
        // The literal "HEAD" must not leak through — it's been
        // structurally replaced by the glyph+sha display.
        assert!(
            !out.text.contains("HEAD"),
            "raw branch label HEAD leaked into detached render: {:?}",
            out.text
        );
    }

    #[test]
    fn renders_normal_branch_when_not_detached() {
        // Belt-and-braces: a non-"HEAD" branch with a populated commit
        // OID must still render the branch name, NOT the SHA prefix.
        // Guards against the detached path firing whenever `commit` is
        // present (which the daemon backend populates for every repo).
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "main".into(),
            commit: "7b3a9f2deadbeef12345678".into(),
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(out.text.contains("main"), "missing branch: {:?}", out.text);
        assert!(
            !out.text.contains("7b3a9f2"),
            "SHA prefix must not appear on a named branch: {:?}",
            out.text
        );
        assert!(
            !out.text.contains('\u{f00ec}'),
            "detached-HEAD glyph must not appear on a named branch: {:?}",
            out.text
        );
    }

    #[test]
    fn renders_tag_after_branch() {
        // Slice 47: tag display. Upstream P10K format is `<branch> @ <tag>`;
        // we surface the daemon's field-18 tag inside the same head_fg
        // colour band (no special marker — it's not an alarm state).
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "main".into(),
            tag: "v1.2.3".into(),
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(
            out.text.contains("main @ v1.2.3"),
            "missing tag-after-branch render: {:?}",
            out.text
        );
    }

    #[test]
    fn subsegment_separator_replaces_default_space() {
        // Slice 56: `[layout.separators].subsegment` joins the in-segment
        // pieces (branch + ahead/behind + index counts + stash + action)
        // in place of the historical single space. Default is still " ",
        // so unset users see byte-identical output; here we set it to
        // " · " and assert it surrounds every joined piece.
        let cfg = p10k_rs_core::Config::from_toml(
            "schema_version = 1\n\
             [layout.separators]\n\
             subsegment = \" \u{b7} \"\n",
        )
        .expect("fixture parses");
        let env = EnvSnapshot::default();
        let g = GitState {
            branch: "main".into(),
            ahead: 2,
            behind: 1,
            staged: 3,
            stash: 1,
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(
            out.text.contains("main \u{b7} \u{21e1}2"),
            "subsegment separator missing between branch and ahead: {:?}",
            out.text
        );
        assert!(
            out.text.contains("\u{21e1}2 \u{b7} \u{21e3}1"),
            "subsegment separator missing between ahead and behind: {:?}",
            out.text
        );
        assert!(
            out.text.contains("\u{21e3}1 \u{b7} +3"),
            "subsegment separator missing before staged count: {:?}",
            out.text
        );
        assert!(
            out.text.contains("+3 \u{b7} \u{2261}1"),
            "subsegment separator missing before stash count: {:?}",
            out.text
        );
    }

    #[test]
    fn subsegment_separator_defaults_to_space() {
        // Belt-and-braces: with no `[layout.separators]` configured, the
        // join falls back to a single space — byte-identical to the
        // pre-slice-56 hardcoded behaviour.
        let (cfg, env) = (p10k_rs_core::Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "main".into(),
            ahead: 1,
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(
            out.text.contains("main \u{21e1}1"),
            "default single-space join broke: {:?}",
            out.text
        );
    }

    #[test]
    fn renders_detached_head_with_tag() {
        // Detached + tag co-exist: glyph + sha THEN ` @ <tag>` — same
        // order as the branch case, just with the SHA standing in for
        // the branch label. Matches how upstream P10K reads a
        // tag-annotated checkout.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "HEAD".into(),
            commit: "abcdef0123456789".into(),
            tag: "v2.0.0".into(),
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(
            out.text.contains("abcdef0"),
            "missing SHA prefix: {:?}",
            out.text
        );
        assert!(
            out.text.contains("@ v2.0.0"),
            "missing tag after detached SHA: {:?}",
            out.text
        );
    }

    #[test]
    fn marker_foreground_defaults_to_red() {
        // Slice 59: absent `marker_foreground` must preserve the historical
        // `\x1b[31m` (red) so existing users see no change.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let g = GitState {
            branch: "main".into(),
            dirty: true,
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(
            out.text.contains("\x1b[31m"),
            "default marker colour must be red (\\x1b[31m): {:?}",
            out.text
        );
        assert!(out.text.contains('*'), "dirty marker must be present");
    }

    #[test]
    fn marker_foreground_override_replaces_red() {
        // Slice 59: `[segment.vcs].marker_foreground = "blue"` must paint
        // the dirty `*` and action labels blue instead of red. The blue
        // SGR must appear in the output and the default red must not.
        let cfg = p10k_rs_core::Config::from_toml(
            "schema_version = 1\n\
             [segment.vcs]\n\
             marker_foreground = \"blue\"\n",
        )
        .expect("fixture parses");
        let env = EnvSnapshot::default();
        let g = GitState {
            branch: "feat/x".into(),
            dirty: true,
            ..Default::default()
        };
        let ctx = ctx_with_git(&cfg, &env, Path::new("/"), Some(&g));
        let out = Vcs.render(&ctx);
        assert!(out.text.contains('*'), "dirty marker must be present");
        // Ansi256 blue is `\x1b[38;5;4m`; that's the default ColorMode.
        assert!(
            out.text.contains("\x1b[38;5;4m"),
            "marker must use blue SGR (\\x1b[38;5;4m): {:?}",
            out.text
        );
        // The hardcoded red must not appear — it was replaced by blue.
        assert!(
            !out.text.contains("\x1b[31m"),
            "hardcoded red must not appear when marker_foreground overrides it: {:?}",
            out.text
        );
    }
}
