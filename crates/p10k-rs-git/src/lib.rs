//! Git status engine for `p10k-rs`.
//!
//! This is the `gitstatusd` replacement. Two implementation paths share one
//! [`status`] entrypoint:
//!
//! - **Slow / clean path:** `gix-status` end-to-end. Used as a correctness
//!   reference and on platforms where the parent-fd walker doesn't pay off.
//! - **Hot path:** `gix` for index parse, packed-refs, tag DB, ahead/behind,
//!   and push-remote resolution; a custom `rustix`-based walker
//!   (`openat`/`fstatat`/`getdents`) for the dirty-tree scan, fed by a
//!   per-directory mtime cache in `$XDG_CACHE_HOME/p10k-rs/`.
//!
//! The day-1 spike (`crates/spike-gitstatus`) decides which path wins.
//!
//! See `ARCHITECTURE.md` § 2.4 and `07-gitstatus.md` for the full plan.

// `unsafe` is permitted in this crate only — see the workspace policy in
// `ARCHITECTURE.md` § 3.5. Every `unsafe` block must carry a SAFETY comment
// per the contractor brief.
#![warn(missing_docs)]

use std::path::PathBuf;

use thiserror::Error;

/// Snapshot of git state for a working directory.
///
/// Mirrors `ARCHITECTURE.md` § 2.4. Built once per prompt and reused across
/// segments through [`p10k_rs_core::RenderCtx::git`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GitState {
    /// Absolute path to the working directory root.
    pub workdir: PathBuf,
    /// What HEAD currently points at.
    pub head: HeadRef,
    /// Commits ahead of upstream.
    pub ahead: u32,
    /// Commits behind upstream.
    pub behind: u32,
    /// Commits ahead of the configured push remote.
    pub push_ahead: u32,
    /// Commits behind the configured push remote.
    pub push_behind: u32,
    /// Number of stashed states.
    pub stash_count: u32,
    /// Active multi-step operation, if any (`"rebase"`, `"merge"`, ...).
    pub action: Option<&'static str>,
    /// Number of unmerged paths.
    pub conflicts: u32,
    /// Counts of staged changes, by kind.
    pub staged: StagedSummary,
    /// Counts of unstaged changes, by kind.
    pub unstaged: UnstagedSummary,
    /// Untracked file count.
    pub untracked: u32,
    /// Files marked `assume-unchanged`.
    pub assume_unchanged: u32,
    /// Files marked `skip-worktree`.
    pub skip_worktree: u32,
    /// HEAD commit OID as bytes (rendering decides hex/short).
    pub commit: Oid,
    /// Subject line of the HEAD commit.
    pub commit_summary: String,
    /// Configured upstream tracking ref.
    pub remote: Option<RemoteRef>,
    /// Configured push remote ref.
    pub push_remote: Option<RemoteRef>,
}

/// Where HEAD points.
#[derive(Debug, Clone)]
pub enum HeadRef {
    /// Attached to a branch by name.
    Branch(String),
    /// Detached at a specific commit.
    DetachedAt(Oid),
    /// Detached at an annotated tag.
    Tagged(String),
}

/// Object identifier (raw 20-byte SHA-1 or 32-byte SHA-256).
///
/// Carried as bytes so we don't allocate a hex string per prompt; the
/// renderer formats once at the end of the pipeline.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Oid(pub Vec<u8>);

/// Counts of staged changes, broken out by kind.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct StagedSummary {
    /// Newly added files.
    pub new: u32,
    /// Deleted files.
    pub deleted: u32,
    /// Modified files.
    pub modified: u32,
}

/// Counts of unstaged changes, broken out by kind.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct UnstagedSummary {
    /// Deleted files.
    pub deleted: u32,
    /// Modified files.
    pub modified: u32,
}

/// A remote tracking ref (e.g. `origin/main`).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RemoteRef {
    /// Remote name, typically `origin`.
    pub remote: String,
    /// Branch name on the remote.
    pub branch: String,
}

/// Inputs to a git-status query.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StatusRequest {
    /// Working directory whose state we want.
    pub workdir: PathBuf,
    /// Hard cap on file scanning. `-1` means "no limit".
    pub max_index_size_dirty: i64,
    /// Whether to count untracked files (an upstream knob; expensive on
    /// repositories with large `node_modules`).
    pub count_untracked: bool,
}

/// Errors returned from git-status calls.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GitError {
    /// The path is not inside any git working tree.
    #[error("not a git repository: {0}")]
    NotARepo(PathBuf),
    /// Index parsing failed; passed through from `gix`.
    #[error("git index parse failed: {0}")]
    Index(String),
    /// Generic I/O failure during the walk.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Compute git state for the working directory in `req`.
///
/// # Errors
///
/// Returns [`GitError::NotARepo`] when no `.git` directory is found upward
/// from `req.workdir`. Other variants surface I/O and parse failures.
///
/// # Panics
///
/// Currently unimplemented; calls panic with `unimplemented!()`. The day-1
/// spike crate (`crates/spike-gitstatus`) is what drives the implementation.
pub fn status(_req: &StatusRequest) -> Result<GitState, GitError> {
    unimplemented!("status() lands after the spike picks an implementation path")
}
