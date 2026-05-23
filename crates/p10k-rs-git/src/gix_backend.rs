//! Pure-Rust git-status fallback via gitoxide (slice 60).
//!
//! This is the third tier in the backend chain — fires when the
//! [`crate::Gitstatusd`] daemon is unavailable *and* the [`crate::ShellOut`]
//! backend can't reach `git` on `$PATH`. Use case: AI hosts (Claude Code,
//! Cursor) running in stripped containers without a system git binary,
//! and CI images that omit `git` to save space.
//!
//! ## Status
//!
//! **Phase 2.** Branch + HEAD lookup wired. Status iteration
//! (staged / unstaged / untracked / conflicts) is phase 3; ahead /
//! behind walk is phase 4; in-progress action probing is phase 5;
//! cross-check tests against the other backends are phase 6.
//!
//! ## Dep choice
//!
//! The workspace pulls the `gix` umbrella crate with
//! `default-features = false`. Reality flipped the design doc's
//! original recommendation (scoped subcrates) — see
//! `Cargo.toml`'s gix block comment for the empirical results.

use std::path::Path;

use gix::bstr::ByteSlice;
use p10k_rs_core::safety::SafeText;
use p10k_rs_core::GitState;

use crate::Backend;

/// Pure-Rust git-status producer for cwds where neither `gitstatusd`
/// nor the `git` CLI is available.
///
/// Currently populates `GitState.branch` only (phase 2). The rest of
/// the fields land in phases 3–6.
#[derive(Debug, Default, Clone, Copy)]
pub struct GixBackend;

/// Cap on branch-name length when wrapping in [`SafeText`].
///
/// Mirrors the cap the daemon parser applies (see
/// `gitstatusd::parse_response` in this crate) so a hostile
/// `.git/refs/heads/<name>` written by hand can't blow the prompt up.
/// 4 `KiB` is generous — git's own `check-ref-format` rejects names
/// longer than `MAX_INPUT_REFNAME_SIZE` (255 bytes) at create time;
/// this is the defensive ceiling for paths that bypass that check.
const BRANCH_NAME_CAP: usize = 4096;

/// Short-OID length for detached-HEAD rendering. Mirrors what
/// `git log --oneline` and the upstream P10K prompt use — long
/// enough to be unambiguous in any reasonable repo, short enough
/// to read at a glance.
const SHORT_OID_LEN: usize = 7;

impl Backend for GixBackend {
    fn status(&self, path: &Path) -> Option<GitState> {
        // `gix::discover` walks upward looking for a `.git/` directory.
        // Returns `Err` for cwds outside any repo — collapse to `None`
        // so the fallback chain stays branchless on the not-a-repo
        // case (the most common cwd shape during prompt rendering).
        let repo = gix::discover(path).ok()?;
        let branch = branch_safetext(&repo);
        let counts = compute_status(&repo);
        // Phase 5: reuse the filesystem-based in-progress-action probe
        // from the ShellOut backend. `repo.git_dir()` gives the real
        // `.git/` path (handling worktrees + submodules correctly via
        // gix's own resolution), which `crate::detect_action` then
        // checks for MERGE_HEAD / rebase-merge / CHERRY_PICK_HEAD /
        // REVERT_HEAD / BISECT_LOG sentinels.
        let action = crate::detect_action(repo.git_dir());
        Some(GitState {
            branch,
            dirty: counts.is_dirty(),
            staged: counts.staged,
            unstaged: counts.unstaged,
            untracked: counts.untracked,
            has_conflicts: counts.has_conflicts,
            action,
            ..Default::default()
        })
    }
}

/// Per-category status counts for a working tree.
///
/// Mirrors the [`GitState`] counters exactly. `dirty` on the public
/// type is derived from any of these being non-zero / true, kept as a
/// separate field on `GitState` because the daemon backend can return
/// it without paying for the per-category breakdown.
#[derive(Debug, Default, Clone, Copy)]
struct StatusCounts {
    staged: u32,
    unstaged: u32,
    untracked: u32,
    has_conflicts: bool,
}

impl StatusCounts {
    /// Whether the working tree has any kind of change relative to
    /// `HEAD` or the index.
    fn is_dirty(&self) -> bool {
        self.staged > 0 || self.unstaged > 0 || self.untracked > 0 || self.has_conflicts
    }
}

/// Walk the repo's combined status iterator and bucket each item
/// into the four `GitState` categories.
///
/// `gix::status::Platform::into_iter` returns items of two outer
/// shapes:
///
/// - [`Item::TreeIndex`] — a `HEAD^{tree}` ↔ index difference. Any
///   variant of the inner `gix_diff::index::Change` (Addition,
///   Deletion, Modification, Rewrite) counts as one staged change.
///   Renames are reported as a single change here, matching
///   `git status --porcelain`'s single `R`-prefixed line.
/// - [`Item::IndexWorktree`] — an index ↔ working-tree difference,
///   split further by the inner [`index_worktree::Item`] variant:
///   - `Modification { status: Conflict { .. }, .. }` flips
///     `has_conflicts`. Conflicted entries are deliberately NOT
///     counted as `unstaged` — `git status` shows them in their
///     own "unmerged paths" section, not under "changes to be
///     committed" or "changes not staged."
///   - `Modification { status: Change(_), .. }` and
///     `Modification { status: IntentToAdd, .. }` count as one
///     unstaged change each (intent-to-add is a tracked entry
///     whose content lives in the worktree, not the index).
///   - `Modification { status: NeedsUpdate(_), .. }` is a
///     stat-cache refresh — gix is telling us the entry didn't
///     change but its mtime/size cache is stale. Ignored; not a
///     user-visible diff.
///   - `DirectoryContents { entry, .. }` where
///     `entry.status == Untracked` counts as one untracked file.
///     Ignored / tracked / pruned dirwalk hits are excluded.
///   - `Rewrite { .. }` (rename or copy between index and worktree)
///     counts as one unstaged change; the `copy` distinction
///     doesn't matter for the count.
///
/// Errors from gix collapse to "report what we counted so far." The
/// fallback chain prefers a possibly-incomplete answer over crashing
/// the prompt on a transient repo issue — same posture as the prior
/// `dirty: bool` implementation.
///
/// Counts saturate at [`u32::MAX`] to defend against pathological
/// repos with billions of entries; the prompt will display the cap
/// rather than wrap to zero.
fn compute_status(repo: &gix::Repository) -> StatusCounts {
    use gix::status::index_worktree::Item as IwItem;
    use gix::status::plumbing::index_as_worktree::EntryStatus;
    use gix::status::Item;

    let mut out = StatusCounts::default();

    let Ok(platform) = repo.status(gix::progress::Discard) else {
        return out;
    };
    let Ok(iter) = platform.into_iter(Vec::new()) else {
        return out;
    };

    for item in iter.filter_map(Result::ok) {
        match item {
            Item::TreeIndex(_) => {
                out.staged = out.staged.saturating_add(1);
            }
            Item::IndexWorktree(IwItem::Modification { status, .. }) => match status {
                EntryStatus::Conflict { .. } => out.has_conflicts = true,
                EntryStatus::Change(_) | EntryStatus::IntentToAdd => {
                    out.unstaged = out.unstaged.saturating_add(1);
                }
                EntryStatus::NeedsUpdate(_) => {}
            },
            Item::IndexWorktree(IwItem::DirectoryContents { entry, .. }) => {
                if matches!(entry.status, gix::dir::entry::Status::Untracked) {
                    out.untracked = out.untracked.saturating_add(1);
                }
            }
            Item::IndexWorktree(IwItem::Rewrite { .. }) => {
                out.unstaged = out.unstaged.saturating_add(1);
            }
        }
    }
    out
}

/// Resolve the branch name (or detached short OID) for `repo` and
/// wrap it in [`SafeText`].
///
/// Three cases:
///
/// - **On a branch:** `head_name()` returns `Some(FullName)`. The
///   `FullName::shorten()` strips the `refs/heads/` prefix, leaving
///   just the branch name. Bytes flow through `from_untrusted_bytes`
///   so a hand-written `.git/refs/heads/<n>` with control bytes
///   can't reach the prompt unsanitised.
/// - **Detached HEAD pointing at a commit:** render as the short OID
///   prefix (default 7 hex chars). Pre-sanitised hex; safe.
/// - **Unborn / unresolvable:** empty [`SafeText`]. The vcs segment
///   already handles the empty-branch case by hiding itself.
fn branch_safetext(repo: &gix::Repository) -> SafeText {
    // High-level path: ask for the named branch first. This covers
    // the common case (HEAD points at refs/heads/<name>) without
    // touching the head() construct.
    match repo.head_name() {
        Ok(Some(full)) => {
            // `shorten()` returns `&BStr` (byte string). Bytes flow
            // through `from_untrusted_bytes` (lossy UTF-8 + control
            // stripping); the result is then re-clamped through
            // `from_untrusted_with_cap` to enforce a defensive
            // length ceiling against pathological `.git/refs/heads/`
            // names that bypass git's own `check-ref-format` limit.
            let sanitised = SafeText::from_untrusted_bytes(full.shorten().as_bytes());
            return SafeText::from_untrusted_with_cap(sanitised.as_str(), BRANCH_NAME_CAP);
        }
        Ok(None) => {} // Detached HEAD — fall through to OID rendering.
        Err(_) => return SafeText::from_untrusted(""),
    }
    detached_short_oid(repo).unwrap_or_else(|| SafeText::from_untrusted(""))
}

/// Render the current detached-HEAD commit as a short OID prefix.
///
/// Returns `None` if HEAD can't be read or is unborn. The OID hex
/// is pure ASCII so [`SafeText::from_untrusted`] is the right wrap
/// (no byte-level sanitisation needed beyond the type-system gate).
fn detached_short_oid(repo: &gix::Repository) -> Option<SafeText> {
    let head = repo.head().ok()?;
    match head.kind {
        gix::head::Kind::Detached { target, .. } => {
            let hex = target.to_hex_with_len(SHORT_OID_LEN).to_string();
            Some(SafeText::from_untrusted(&hex))
        }
        // Symbolic should have been caught by head_name() above; if
        // it falls through here something odd happened — bail empty.
        gix::head::Kind::Symbolic(_) | gix::head::Kind::Unborn(_) => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// A non-repo cwd must return `None` — same shape every other
    /// backend uses for "not in a repo." Pinned in phase 1; preserved.
    #[test]
    fn returns_none_outside_a_repo() {
        let p = std::env::temp_dir();
        let out = GixBackend.status(&p);
        assert!(out.is_none(), "expected None for non-repo cwd, got {out:?}");
    }

    /// When run inside the workspace repo (which the test process
    /// always is), the backend must report *some* non-empty branch
    /// name. The exact name depends on the maintainer's local
    /// checkout, so we assert on the shape only.
    #[test]
    fn reports_branch_inside_workspace() {
        // Walk up from the crate dir to find the workspace's `.git`.
        let crate_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let out = GixBackend
            .status(&crate_dir)
            .expect("workspace is a git repo; backend must report it");
        // Branch should be non-empty either as a named branch or as
        // a short-OID detached marker. CI may run on a tag-detached
        // checkout, so accept either.
        assert!(
            !out.branch.as_str().is_empty(),
            "branch must be non-empty inside a repo"
        );
        // Per-category counts (phase 3.5) and action (phase 5) depend
        // on the maintainer's working-tree state — pin only that the
        // fields are *queryable*, not their values.
        let _ = (
            out.staged,
            out.unstaged,
            out.untracked,
            out.has_conflicts,
            out.action.as_str(),
        );
    }

    /// `branch_safetext` should never panic and never produce text
    /// longer than the cap. Builds a fresh repo via `gix::init` to
    /// avoid depending on the workspace's exact branch state.
    /// A fresh repo with no commits and no untracked files must be
    /// reported as clean. Pins the phase 3 `dirty: bool` contract
    /// against false-positive reports.
    #[test]
    fn fresh_init_repo_is_clean() {
        let scratch = std::env::temp_dir().join(format!(
            "p10krs-gix-clean-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        ));
        let _repo = gix::init(&scratch).expect("gix::init");
        let out = GixBackend
            .status(&scratch)
            .expect("scratch is a repo; backend must report it");
        assert!(
            !out.dirty,
            "fresh empty repo must be reported clean, got dirty=true"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A fresh repo with an untracked file in its working tree must
    /// be reported as dirty *and* have exactly one untracked counted
    /// (and zero of the other three categories). Pins both the phase
    /// 3 `dirty: bool` contract and the phase 3.5 per-category
    /// breakdown for the untracked bucket.
    #[test]
    fn fresh_init_repo_with_untracked_is_dirty() {
        let scratch = std::env::temp_dir().join(format!(
            "p10krs-gix-untracked-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        ));
        let _repo = gix::init(&scratch).expect("gix::init");
        std::fs::write(scratch.join("hello.txt"), b"world\n").expect("write file");
        let out = GixBackend
            .status(&scratch)
            .expect("scratch is a repo; backend must report it");
        assert!(out.dirty, "repo with untracked file must be reported dirty");
        assert_eq!(out.untracked, 1, "exactly one untracked file expected");
        assert_eq!(out.staged, 0, "no staged changes in a fresh init");
        assert_eq!(out.unstaged, 0, "no tracked-file changes");
        assert!(!out.has_conflicts, "no conflict on a fresh init");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// Run a git command inside `dir` with a hermetic env so co-tenant
    /// global / system git config can't bleed into test setup. Returns
    /// `false` if git isn't on PATH or the command fails — used to
    /// gracefully skip tests that need git for state setup (the
    /// runtime path under test is pure gix; git is only here to
    /// stage / commit fixtures).
    fn git_setup(dir: &std::path::Path, args: &[&str]) -> bool {
        let mut cmd = std::process::Command::new("git");
        cmd.current_dir(dir)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("HOME", dir)
            .env("GIT_AUTHOR_NAME", "p10krs test")
            .env("GIT_AUTHOR_EMAIL", "test@p10krs.local")
            .env("GIT_COMMITTER_NAME", "p10krs test")
            .env("GIT_COMMITTER_EMAIL", "test@p10krs.local");
        cmd.status().is_ok_and(|s| s.success())
    }

    /// `true` if `git --version` succeeds. Tests that need git for
    /// fixture setup skip silently on hosts without git — they still
    /// exercise the read path under test in CI, which always has git.
    fn have_git() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// A repo where one file is in the index but the worktree is
    /// otherwise empty of differences must be reported with
    /// `staged == 1` and zero of everything else.
    ///
    /// Setup uses `git add` rather than gix's plumbing because
    /// gix's high-level add API isn't trivially callable; the
    /// runtime path under test is still pure gix. Skips on hosts
    /// without `git` on PATH.
    #[test]
    fn repo_with_staged_file_counts_one_staged() {
        if !have_git() {
            return;
        }
        let scratch = std::env::temp_dir().join(format!(
            "p10krs-gix-staged-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        ));
        std::fs::create_dir_all(&scratch).expect("mkdir scratch");
        assert!(git_setup(&scratch, &["init", "-q"]), "git init failed");
        std::fs::write(scratch.join("a.txt"), b"hello\n").expect("write a.txt");
        assert!(
            git_setup(&scratch, &["add", "a.txt"]),
            "git add a.txt failed"
        );
        let out = GixBackend
            .status(&scratch)
            .expect("scratch is a repo; backend must report it");
        assert_eq!(out.staged, 1, "exactly one staged change expected");
        assert_eq!(out.unstaged, 0, "no unstaged tracked changes expected");
        assert_eq!(
            out.untracked, 0,
            "no untracked files expected (a.txt is staged)"
        );
        assert!(!out.has_conflicts, "no conflicts expected");
        assert!(out.dirty, "staged change → dirty");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A repo with a tracked file modified in the worktree (but
    /// nothing newly staged on top) must report `unstaged == 1`
    /// and zero of the other three categories.
    #[test]
    fn repo_with_modified_tracked_file_counts_one_unstaged() {
        if !have_git() {
            return;
        }
        let scratch = std::env::temp_dir().join(format!(
            "p10krs-gix-modified-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        ));
        std::fs::create_dir_all(&scratch).expect("mkdir scratch");
        assert!(git_setup(&scratch, &["init", "-q"]), "git init failed");
        std::fs::write(scratch.join("a.txt"), b"hello\n").expect("write a.txt");
        assert!(git_setup(&scratch, &["add", "a.txt"]), "git add failed");
        assert!(
            git_setup(&scratch, &["commit", "-q", "-m", "init"]),
            "git commit failed"
        );
        // Modify the now-tracked file. Worktree diverges from index;
        // index still matches HEAD.
        std::fs::write(scratch.join("a.txt"), b"world\n").expect("modify a.txt");
        let out = GixBackend
            .status(&scratch)
            .expect("scratch is a repo; backend must report it");
        assert_eq!(out.unstaged, 1, "exactly one unstaged change expected");
        assert_eq!(out.staged, 0, "nothing new staged on top");
        assert_eq!(out.untracked, 0, "no untracked files");
        assert!(!out.has_conflicts, "no conflicts");
        assert!(out.dirty, "modified tracked file → dirty");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A repo with a `MERGE_HEAD` sentinel in `.git/` must report
    /// `action = "merge"`. Pins the phase 5 in-progress-action
    /// probe against false negatives.
    #[test]
    fn fresh_init_repo_with_merge_head_reports_merge_action() {
        let scratch = std::env::temp_dir().join(format!(
            "p10krs-gix-merge-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        ));
        let _repo = gix::init(&scratch).expect("gix::init");
        // gix::init places the git dir at <scratch>/.git on a
        // non-bare repo. Plant a MERGE_HEAD sentinel inside it.
        std::fs::write(
            scratch.join(".git").join("MERGE_HEAD"),
            b"0123456789abcdef0123456789abcdef01234567\n",
        )
        .expect("write MERGE_HEAD");
        let out = GixBackend
            .status(&scratch)
            .expect("scratch is a repo; backend must report it");
        assert_eq!(
            out.action.as_str(),
            "merge",
            "MERGE_HEAD sentinel should map to action=merge"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn fresh_init_repo_has_initial_branch_name() {
        let scratch = std::env::temp_dir().join(format!(
            "p10krs-gix-init-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos()),
        ));
        // `gix::init` creates the .git/ structure programmatically —
        // no shell-out to `git`, so this test runs even on a host
        // without a system git binary (which is the use case this
        // whole backend exists to serve).
        let repo = gix::init(&scratch).expect("gix::init must succeed");
        let _ = &repo; // suppress unused warning; the side-effect is on disk
        let out = GixBackend
            .status(&scratch)
            .expect("scratch is a repo; backend must report it");
        // Fresh repos are unborn (HEAD points at refs/heads/<default>
        // but no commits yet). gix surfaces this via `head_name()`
        // returning a name — so branch should be non-empty. Just
        // pin the cap invariant.
        assert!(
            out.branch.as_str().len() <= BRANCH_NAME_CAP,
            "branch name longer than cap"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }
}
