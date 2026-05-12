//! Git state producers for `p10k-rs`.
//!
//! Per ADR-0001 (`docs/adr/0001-git-backend.md`), the production hot path is
//! a long-lived `gitstatusd` client. This crate exposes:
//!
//! - [`Backend`] — the trait every producer implements.
//! - [`ShellOut`] — slow but always-available fallback that spawns `git`.
//! - [`Gitstatusd`] — the daemon-backed fast path.
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

use p10k_rs_core::safety::SafeText;
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
/// `git` is on `$PATH`. Used as the fallback when no `gitstatusd` is
/// present for the host triple.
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
        let mut state = parse_porcelain_v1(&stdout);
        // Slice 45: surface in-progress repo actions (merge / rebase / …)
        // from the filesystem. `git rev-parse --git-dir` resolves through
        // worktrees and submodules cleanly; an `Option` lookup keeps the
        // shell-out path silent on failure.
        if let Some(git_dir) = resolve_git_dir(path) {
            state.action = detect_action(&git_dir);
        }
        Some(state)
    }
}

/// Resolve the `.git` directory for `path` via `git rev-parse --git-dir`.
///
/// Worktrees and submodules don't keep their state under a literal
/// `<repo>/.git/` directory, so we ask `git` instead of joining paths
/// blindly. The output is one line: an absolute path. We discard the
/// trailing newline and trust git's own canonicalisation. Returns
/// `None` if `git` isn't on `$PATH`, the path isn't inside any repo,
/// or stdout isn't valid UTF-8.
fn resolve_git_dir(path: &Path) -> Option<std::path::PathBuf> {
    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--git-dir"])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = std::str::from_utf8(&out.stdout).ok()?.trim_end();
    if s.is_empty() {
        return None;
    }
    let raw = std::path::PathBuf::from(s);
    if raw.is_absolute() {
        Some(raw)
    } else {
        Some(path.join(raw))
    }
}

/// Inspect `git_dir` for the canonical in-progress action sentinels.
///
/// Mirrors libgit2's `git_repository_state` / the upstream P10K
/// `gitstatusd` action probe. Probe order is deterministic so that two
/// overlapping markers (e.g. an interrupted rebase that also has a
/// MERGE_HEAD) resolve to a single, stable label rather than oscillating
/// between prompts.
///
/// The returned [`SafeText`] is sanitised at construction time even
/// though the values are static ASCII today — the type system makes
/// the invariant uniform across every prompt input, so a future variant
/// that bakes in an attacker-controlled tail (e.g. rebase step number
/// pulled from `.git/rebase-merge/msgnum`) can't bypass it by accident.
#[must_use]
pub fn detect_action(git_dir: &Path) -> SafeText {
    if git_dir.join("MERGE_HEAD").exists() {
        return SafeText::from_untrusted("merge");
    }
    if git_dir.join("rebase-merge").is_dir() || git_dir.join("rebase-apply").is_dir() {
        return SafeText::from_untrusted("rebase");
    }
    if git_dir.join("CHERRY_PICK_HEAD").exists() {
        return SafeText::from_untrusted("cherry-pick");
    }
    if git_dir.join("REVERT_HEAD").exists() {
        return SafeText::from_untrusted("revert");
    }
    if git_dir.join("BISECT_LOG").exists() {
        return SafeText::from_untrusted("bisect");
    }
    SafeText::from_untrusted("")
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
///
/// Returns a [`SafeText`] — branches with embedded control bytes get
/// sanitised at construction time so they can't reach the prompt
/// unsanitised. Git's `check-ref-format` rejects such names at commit
/// time, but a malicious `.git/refs/heads/<name>` written by hand or
/// by a misbehaving tool can still surface here.
fn parse_branch_header(header: &str) -> SafeText {
    SafeText::from_untrusted(&parse_branch_header_raw(header))
}

fn parse_branch_header_raw(header: &str) -> String {
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

    /// Build a unique scratch `.git`-ish directory under
    /// `std::env::temp_dir()`. Caller owns cleanup; we deliberately leak
    /// on panic so a failing test leaves evidence on disk.
    fn scratch_git_dir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!(
            "p10krs-detect-action-{}-{}-{}",
            label,
            std::process::id(),
            nanos,
        ));
        std::fs::create_dir_all(&p).expect("mkdir scratch");
        p
    }

    #[test]
    fn detect_action_empty_when_no_markers() {
        let dir = scratch_git_dir("empty");
        assert_eq!(detect_action(&dir).as_str(), "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_action_merge() {
        let dir = scratch_git_dir("merge");
        std::fs::write(dir.join("MERGE_HEAD"), b"deadbeef\n").expect("write");
        assert_eq!(detect_action(&dir).as_str(), "merge");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_action_rebase_merge() {
        let dir = scratch_git_dir("rebase-merge");
        std::fs::create_dir(dir.join("rebase-merge")).expect("mkdir");
        assert_eq!(detect_action(&dir).as_str(), "rebase");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_action_rebase_apply() {
        let dir = scratch_git_dir("rebase-apply");
        std::fs::create_dir(dir.join("rebase-apply")).expect("mkdir");
        assert_eq!(detect_action(&dir).as_str(), "rebase");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_action_cherry_pick() {
        let dir = scratch_git_dir("cherry");
        std::fs::write(dir.join("CHERRY_PICK_HEAD"), b"deadbeef\n").expect("write");
        assert_eq!(detect_action(&dir).as_str(), "cherry-pick");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_action_revert() {
        let dir = scratch_git_dir("revert");
        std::fs::write(dir.join("REVERT_HEAD"), b"deadbeef\n").expect("write");
        assert_eq!(detect_action(&dir).as_str(), "revert");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_action_bisect() {
        let dir = scratch_git_dir("bisect");
        std::fs::write(dir.join("BISECT_LOG"), b"git bisect start\n").expect("write");
        assert_eq!(detect_action(&dir).as_str(), "bisect");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_action_merge_wins_over_rebase() {
        // Deterministic precedence: an interrupted rebase with both
        // MERGE_HEAD and a rebase-merge dir resolves to `merge`. This
        // keeps the prompt label stable across consecutive prompts.
        let dir = scratch_git_dir("merge-precedence");
        std::fs::write(dir.join("MERGE_HEAD"), b"deadbeef\n").expect("write");
        std::fs::create_dir(dir.join("rebase-merge")).expect("mkdir");
        assert_eq!(detect_action(&dir).as_str(), "merge");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_branch_with_control_chars_strips_them() {
        // Git's `check-ref-format` rejects control bytes in normal flows,
        // but a hand-written `.git/refs/heads/<name>` (or a misbehaving
        // tool) can bypass that check. Defend the prompt anyway.
        //
        // Note that `\r` is Unicode `White_Space`, so `split_whitespace`
        // in the parser cuts at it before `sanitize_for_terminal` even
        // runs — `\x1b`, `\x07`, `\x08` are not whitespace and rely on
        // sanitisation to be stripped.
        assert_eq!(parse_branch_header("## \x1b[2Jmain"), "[2Jmain");
        assert_eq!(parse_branch_header("## main\x07evil"), "mainevil");
        assert_eq!(parse_branch_header("## main\rEVIL"), "main");
    }
}
