//! Day-1 spike: three implementations of `git status` with timings.
//!
//! The spike answers exactly one question: **on a clean chromium repo, can a
//! pure-Rust path get within ~2x of `gitstatusd`'s latency?** The decision
//! tree from `MVP-SPEC.md` § "Day-1 Spike" routes the project from there.
//!
//! Three impls live side-by-side so we can compare them on identical inputs:
//!
//! - [`gix_path`] — straight `gix-status` walker. The "boring Rust" baseline.
//! - [`hybrid`] — `gix` for index/refs, `rustix` parent-fd walker with a
//!   per-directory mtime untracked-cache, mirroring `gitstatusd`'s
//!   `index.cc::ScanDirs`.
//! - [`gitstatusd_baseline`] — fork/exec the prebuilt C++ daemon, send one
//!   request on the wire protocol, parse one response. The "ground truth"
//!   reference.
//!
//! All three return the same [`GitStatusSummary`] subset of `VCS_STATUS_*`
//! fields. The integration test in `tests/correctness.rs` is the **acceptance
//! gate**: if the three impls disagree, no benchmark number matters until the
//! disagreement is explained.

#![allow(clippy::too_many_lines)] // The walkers are linear; splitting them harms readability.

use std::path::PathBuf;

pub mod gitstatusd_baseline;
pub mod gix_path;
pub mod hybrid;

/// Subset of `VCS_STATUS_*` fields that the spike compares across all three
/// impls. Picked from `07-gitstatus.md` § wire format as the eight most
/// load-bearing for a prompt:
///
/// 1. `branch` (`VCS_STATUS_LOCAL_BRANCH`) — empty on detached HEAD.
/// 2. `commit` (`VCS_STATUS_COMMIT`)       — 40-hex HEAD oid, empty if unborn.
/// 3. `ahead` (`VCS_STATUS_COMMITS_AHEAD`).
/// 4. `behind` (`VCS_STATUS_COMMITS_BEHIND`).
/// 5. `staged_count`   (`VCS_STATUS_NUM_STAGED`).
/// 6. `unstaged_count` (`VCS_STATUS_NUM_UNSTAGED`).
/// 7. `untracked_count` (`VCS_STATUS_NUM_UNTRACKED`).
/// 8. `has_conflicts`   (derived from `VCS_STATUS_NUM_CONFLICTED > 0`).
///
/// Fields are unbounded `u32` here — the per-category caps from `gitstatusd`
/// (`-s` / `-u` / `-c` / `-d`) are a downstream concern, not a correctness one.
///
/// `TODO(adviser):` rename detection is off in all three impls (matching
/// `gitstatusd` defaults). If we ever turn it on, `staged_count` semantics
/// shift and these vectors will drift.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GitStatusSummary {
    /// Local branch short name. Empty string on detached HEAD.
    pub branch: String,
    /// 40-hex HEAD object id. Empty string on an unborn HEAD.
    pub commit: String,
    /// Commits the local branch has that the upstream does not. `0` if no upstream.
    pub ahead: u32,
    /// Commits the upstream has that the local branch does not. `0` if no upstream.
    pub behind: u32,
    /// Files with staged changes (tree-vs-index diff entries).
    pub staged_count: u32,
    /// Files with unstaged changes (index-vs-workdir diff entries, excluding untracked).
    pub unstaged_count: u32,
    /// Files present in the workdir that are neither tracked nor ignored.
    pub untracked_count: u32,
    /// `true` iff the index reports any unmerged (conflicted) entries.
    pub has_conflicts: bool,
}

/// Errors surfaced by the three spike implementations.
///
/// Library code never panics. The CLI binary may `?`-bubble these into
/// `anyhow::Error` for top-level reporting.
#[derive(Debug, thiserror::Error)]
pub enum SpikeError {
    /// The path the user passed is not inside a git repository.
    #[error("not a git repository: {0}")]
    NotARepository(PathBuf),

    /// Anything `gix` couldn't do for us.
    #[error("gix error: {0}")]
    Gix(#[from] gix::open::Error),

    /// `gix` reference resolution failed.
    #[error("ref resolution: {0}")]
    GixRef(String),

    /// `gix` status produced a hard error.
    #[error("gix-status: {0}")]
    GixStatus(String),

    /// `rustix` syscall failure.
    #[error("rustix syscall: {0}")]
    Rustix(#[from] rustix::io::Errno),

    /// I/O at the boring `std::io` layer.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// The cached untracked-cache file is unreadable. Recoverable: just rebuild.
    #[error("untracked-cache decode: {0}")]
    Cache(String),

    /// `gitstatusd` subprocess failed or printed garbage.
    #[error("gitstatusd: {0}")]
    Daemon(String),

    /// Could not locate the `gitstatusd` binary.
    #[error("gitstatusd binary not found; searched: {searched:?}")]
    DaemonNotFound { searched: Vec<PathBuf> },
}

/// Result alias used internally by the three impls.
pub type Result<T> = std::result::Result<T, SpikeError>;
