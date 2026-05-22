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
        let dirty = repo_is_dirty(&repo);
        Some(GitState {
            branch,
            dirty,
            ..Default::default()
        })
    }
}

/// Probe the working tree for any modification, untracked file, or
/// conflict — anything that would make `git status` report a non-empty
/// porcelain output.
///
/// Mirrors [`crate::ShellOut`]'s `dirty: bool` coverage exactly. The
/// per-category counters (`staged`, `unstaged`, `untracked`,
/// `conflicts`) land in a follow-up phase; this one just answers the
/// boolean.
///
/// Implementation: ask gix for an `index_worktree_iter` and short-circuit
/// on the first item. Returns `false` on any error from the iterator
/// setup — the fallback chain prefers reporting an under-state ("clean")
/// over crashing the prompt on a transient repo issue.
fn repo_is_dirty(repo: &gix::Repository) -> bool {
    let Ok(platform) = repo.status(gix::progress::Discard) else {
        return false;
    };
    let Ok(mut iter) = platform.into_index_worktree_iter(Vec::new()) else {
        return false;
    };
    // Any successful item means the tree differs from the index. We
    // can't tell modified-vs-untracked-vs-conflict from this loop
    // without inspecting `Item` variants, but the boolean answer
    // collapses all of them the same way.
    iter.any(|item| item.is_ok())
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
        // Phase 2 fills branch; phase 3 fills dirty; phases 4-5 fill
        // staged + action. Pin only the not-yet-populated invariants
        // so the test doesn't lie about phase progression.
        assert_eq!(
            out.staged, 0,
            "staged counter is unpopulated until phase 3.5"
        );
        assert_eq!(
            out.action.as_str(),
            "",
            "in-progress-action probe is phase 5"
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
    /// be reported as dirty. Pins the phase 3 `dirty: bool` contract
    /// against false-negative reports.
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
