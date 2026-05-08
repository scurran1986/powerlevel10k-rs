//! Git state producers for `p10k-rs`.
//!
//! Per ADR-0001 (`docs/adr/0001-git-backend.md`), the production hot path is
//! a long-lived `gitstatusd` client. That landing slice ships the
//! `Gitstatusd` backend; for now this crate exposes:
//!
//! - [`Backend`] — the trait every producer implements.
//! - [`ShellOut`] — slow but always-available fallback that spawns `git`.
//!
//! The shape returned to consumers ([`p10k_rs_core::GitState`]) is owned by
//! `p10k-rs-core` so [`p10k_rs_core::RenderCtx`] can hold an `Option<&'_>`
//! without a dependency cycle. This crate produces values; segments consume.
//!
//! The pre-pivot placeholder API (rich `HeadRef`/`Oid`/`StagedSummary`/etc.)
//! was scaffolding for an in-process scanner architecture that ADR-0001
//! superseded. The richer fields come back as needed when the `Gitstatusd`
//! backend lands and starts populating them from the daemon's wire response.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::Path;
use std::process::{Command, Stdio};

use p10k_rs_core::GitState;

pub mod gitstatusd;
pub use gitstatusd::{locate_binary as locate_gitstatusd, Gitstatusd};

/// A producer of [`GitState`] for a working directory.
///
/// Returns `None` when the path isn't inside any git repo (the most common
/// case during prompt rendering — most cwds aren't repos). Implementations
/// must never panic on cwds outside a repo; that's the no-op signal.
pub trait Backend {
    /// Probe `path` for git state. Returns `None` if not a repo.
    fn status(&self, path: &Path) -> Option<GitState>;
}

/// Shell-out backend: spawns `git`. Slow but always available wherever
/// `git` is on `$PATH`. Used as the slice-4 default and as the post-slice-5
/// fallback when no `gitstatusd` is present for the host triple.
#[derive(Debug, Default, Clone, Copy)]
pub struct ShellOut;

impl Backend for ShellOut {
    fn status(&self, path: &Path) -> Option<GitState> {
        // One git invocation does both jobs: branch on the first line,
        // dirty-flag from any subsequent lines.
        let out = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["status", "--porcelain=v1", "--branch", "--no-renames"])
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            // `not a repo` exits non-zero. Stay silent.
            return None;
        }
        let stdout = String::from_utf8(out.stdout).ok()?;
        Some(parse_porcelain_v1(&stdout))
    }
}

/// Parse `git status --porcelain=v1 --branch` output into a [`GitState`].
///
/// Format we expect (`LC_ALL=C`):
///
/// - First line is always `## <branch-info>`. Examples:
///   `## main...origin/main`,
///   `## main...origin/main [ahead 1, behind 2]`,
///   `## main` (no upstream configured),
///   `## HEAD (no branch)` (detached HEAD),
///   `## No commits yet on main` (unborn branch).
/// - Subsequent lines = working-tree changes. Any line means dirty.
fn parse_porcelain_v1(s: &str) -> GitState {
    let mut lines = s.split('\n');
    let header = lines.next().unwrap_or("");
    let branch = parse_branch_header(header);
    // Count *non-empty* remaining lines so a trailing newline doesn't lie.
    let dirty = lines.any(|l| !l.is_empty());
    // ShellOut only fills the cheap fields; richer counts (ahead/behind,
    // staged/unstaged, etc.) live behind the `Gitstatusd` backend.
    GitState {
        branch,
        dirty,
        ..Default::default()
    }
}

/// Pull the branch name out of the `## …` header line.
fn parse_branch_header(header: &str) -> String {
    let rest = header.strip_prefix("## ").unwrap_or(header);
    if let Some(name) = rest.strip_prefix("No commits yet on ") {
        return name.trim().to_owned();
    }
    if rest.starts_with("HEAD (no branch)") {
        return "HEAD".to_owned();
    }
    // `main...origin/main` or `main...origin/main [ahead 1]` → first segment
    // before `...` (or the whole thing if no upstream is configured).
    let local = rest.split("...").next().unwrap_or(rest);
    local.split_whitespace().next().unwrap_or("").to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_branch_with_upstream() {
        let out = "## main...origin/main\n";
        let s = parse_porcelain_v1(out);
        assert_eq!(s.branch, "main");
        assert!(!s.dirty);
    }

    #[test]
    fn parse_branch_with_ahead_behind_and_dirt() {
        let out = "## main...origin/main [ahead 1, behind 2]\n M README.md\n?? new.txt\n";
        let s = parse_porcelain_v1(out);
        assert_eq!(s.branch, "main");
        assert!(s.dirty);
    }

    #[test]
    fn parse_no_upstream() {
        let out = "## feat/widget\n";
        let s = parse_porcelain_v1(out);
        assert_eq!(s.branch, "feat/widget");
        assert!(!s.dirty);
    }

    #[test]
    fn parse_detached_head() {
        let out = "## HEAD (no branch)\n";
        let s = parse_porcelain_v1(out);
        assert_eq!(s.branch, "HEAD");
        assert!(!s.dirty);
    }

    #[test]
    fn parse_unborn_branch() {
        let out = "## No commits yet on main\n";
        let s = parse_porcelain_v1(out);
        assert_eq!(s.branch, "main");
        assert!(!s.dirty);
    }

    #[test]
    fn parse_dirty_only() {
        let out = "## main\n M lib.rs\n";
        let s = parse_porcelain_v1(out);
        assert!(s.dirty);
    }
}
