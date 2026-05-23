//! Slice 60 phase 6 — cross-check tests between [`ShellOut`] and
//! [`GixBackend`].
//!
//! These tests prove that for the same on-disk repo state, the two
//! tiers of the fallback chain return the same answer on the fields
//! both backends populate. A divergence here is a UX bug: the prompt
//! would render differently depending on whether the user's host has
//! a `git` binary on PATH (ShellOut serving) or doesn't (GixBackend
//! serving), and that's exactly the kind of inconsistency the
//! fallback chain is supposed to make invisible.
//!
//! ## Known divergences (not asserted)
//!
//! - **Detached HEAD branch field.** `ShellOut` returns the literal
//!   string `"HEAD"` (it parses `## HEAD (no branch)` from
//!   `git status --porcelain=v1 --branch`); `GixBackend` returns a
//!   7-char short OID. The right UX is the short OID (gitstatusd and
//!   upstream P10K both do that), but lifting ShellOut to match is a
//!   separate slice. The detached-HEAD fixture below asserts only
//!   that both report `dirty == false`, not the branch string.
//! - **`ahead` / `behind`.** ShellOut's `parse_porcelain_v1` doesn't
//!   parse the `[ahead N, behind M]` suffix from the branch header
//!   line yet — leaves both counters at zero. The fixtures here
//!   assert GixBackend-side numbers explicitly (where relevant) but
//!   don't cross-check the counters against ShellOut. Landing
//!   parse-side ahead/behind is a follow-up slice; until then this
//!   field stays out of the parity assertion.
//! - **`action` field.** Both backends call the same
//!   [`detect_action`] helper, so cross-check on `action` is
//!   structurally identical and doesn't add coverage; it's omitted
//!   from the assertion set.
//!
//! ## Now asserted (post-slice-60-phase-6 follow-up)
//!
//! ShellOut's `parse_porcelain_v1` now populates `staged`,
//! `unstaged`, `untracked`, and `has_conflicts` from the porcelain
//! v1 XY status pairs (matching the gix backend's bucketing rules).
//! Cross-check therefore asserts agreement on all four counters
//! across both tiers wherever an `expect_counts` argument is
//! supplied to [`assert_agree`].
//!
//! ## Skipping when git is unavailable
//!
//! Fixture setup uses `git` for everything except the pure-gix
//! cases (e.g. `gix::init`). Tests that need shell-out git for setup
//! skip silently on hosts without `git` on PATH — the runtime path
//! under test is still pure-Rust gix for the `GixBackend` side, so
//! CI (which always has git) is the place these tests bite.

// Integration-test scaffolding may panic freely via expect/unwrap
// (matches the unit-test module allow-set in `gix_backend.rs`), and
// the prose in the module doccomment names the public types directly
// — backticking every `ShellOut` / `GixBackend` mention to placate
// `doc_markdown` would hurt readability without adding rigour.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::doc_markdown
)]

use std::path::Path;

use p10k_rs_git::{Backend as _, GixBackend, ShellOut};

// ---------------------------------------------------------------------------
// Fixture helpers (duplicated from gix_backend.rs's private test module
// because integration tests can't see private items; this is the minimal
// surface needed and keeps the unit-test module untouched).
// ---------------------------------------------------------------------------

/// `true` if `git --version` succeeds. Tests that need git for
/// fixture setup skip silently on hosts without git.
fn have_git() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run a git command inside `dir` with a hermetic env so co-tenant
/// system / global config can't bleed into the test fixture.
fn git(dir: &Path, args: &[&str]) -> bool {
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

/// Make a unique temp dir for a fixture so parallel test runs don't
/// collide. The name embeds the test slug, pid, and a nanosecond
/// timestamp.
fn scratch_for(slug: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "p10krs-xcheck-{}-{}-{}",
        slug,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    ))
}

/// Init a repo on `main` and create `n` commits (one per supplied
/// file name). Returns `true` on success; tests propagate via
/// `assert!`.
fn init_repo_with_commits(scratch: &Path, files: &[&str]) -> bool {
    std::fs::create_dir_all(scratch).expect("mkdir scratch");
    if !git(scratch, &["init", "-q", "-b", "main"]) {
        return false;
    }
    for name in files {
        std::fs::write(scratch.join(name), name.as_bytes()).expect("write fixture");
        if !git(scratch, &["add", name]) {
            return false;
        }
        if !git(scratch, &["commit", "-q", "-m", name]) {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// The cross-check.
// ---------------------------------------------------------------------------

/// Expected per-category counts for an [`assert_agree`] call.
/// Tuple shape: `(staged, unstaged, untracked, has_conflicts)`.
type ExpectedCounts = (u32, u32, u32, bool);

/// Assert that both backends agree on the user-visible fields:
/// `branch` (when `expect_branch` is supplied), `dirty`, and the
/// per-category counters `staged` / `unstaged` / `untracked` /
/// `has_conflicts` (when `expect_counts` is supplied).
///
/// `expect_branch` is `None` for the detached-HEAD case where the
/// two backends are known to disagree on the branch field (see the
/// module doccomment). `expect_counts` is `None` for fixtures where
/// the maintainer is intentionally only spot-checking the parity
/// at the dirty-bool level.
fn assert_agree(
    slug: &str,
    path: &Path,
    expect_dirty: bool,
    expect_branch: Option<&str>,
    expect_counts: Option<ExpectedCounts>,
) {
    let shell_out = ShellOut.status(path).expect("ShellOut reports a state");
    let gix_out = GixBackend.status(path).expect("GixBackend reports a state");

    assert_eq!(
        shell_out.dirty, expect_dirty,
        "[{slug}] ShellOut.dirty mismatch"
    );
    assert_eq!(
        gix_out.dirty, expect_dirty,
        "[{slug}] GixBackend.dirty mismatch"
    );
    assert_eq!(
        shell_out.dirty, gix_out.dirty,
        "[{slug}] backends disagree on `dirty`"
    );

    if let Some(expected) = expect_branch {
        assert_eq!(
            shell_out.branch.as_str(),
            expected,
            "[{slug}] ShellOut.branch mismatch"
        );
        assert_eq!(
            gix_out.branch.as_str(),
            expected,
            "[{slug}] GixBackend.branch mismatch"
        );
    }

    if let Some((staged, unstaged, untracked, has_conflicts)) = expect_counts {
        // Both backends must agree on each per-category field AND
        // on the absolute expectation. Three asserts per field
        // catches both "backends agree but on the wrong number"
        // and "backends disagree" with distinct error messages.
        assert_eq!(
            shell_out.staged, staged,
            "[{slug}] ShellOut.staged mismatch"
        );
        assert_eq!(
            gix_out.staged, staged,
            "[{slug}] GixBackend.staged mismatch"
        );
        assert_eq!(
            shell_out.staged, gix_out.staged,
            "[{slug}] backends disagree on `staged`"
        );

        assert_eq!(
            shell_out.unstaged, unstaged,
            "[{slug}] ShellOut.unstaged mismatch"
        );
        assert_eq!(
            gix_out.unstaged, unstaged,
            "[{slug}] GixBackend.unstaged mismatch"
        );
        assert_eq!(
            shell_out.unstaged, gix_out.unstaged,
            "[{slug}] backends disagree on `unstaged`"
        );

        assert_eq!(
            shell_out.untracked, untracked,
            "[{slug}] ShellOut.untracked mismatch"
        );
        assert_eq!(
            gix_out.untracked, untracked,
            "[{slug}] GixBackend.untracked mismatch"
        );
        assert_eq!(
            shell_out.untracked, gix_out.untracked,
            "[{slug}] backends disagree on `untracked`"
        );

        assert_eq!(
            shell_out.has_conflicts, has_conflicts,
            "[{slug}] ShellOut.has_conflicts mismatch"
        );
        assert_eq!(
            gix_out.has_conflicts, has_conflicts,
            "[{slug}] GixBackend.has_conflicts mismatch"
        );
        assert_eq!(
            shell_out.has_conflicts, gix_out.has_conflicts,
            "[{slug}] backends disagree on `has_conflicts`"
        );
    }
}

/// Clean repo on `main`: both backends must report `dirty=false`
/// and `branch="main"`.
#[test]
fn agree_clean_repo() {
    if !have_git() {
        return;
    }
    let scratch = scratch_for("clean");
    assert!(init_repo_with_commits(&scratch, &["a.txt"]));
    assert_agree(
        "clean",
        &scratch,
        false,
        Some("main"),
        Some((0, 0, 0, false)),
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Repo with an untracked file. ShellOut sees the `??` porcelain
/// line and flips `dirty`; GixBackend reaches the same answer via
/// the dirwalk `DirectoryContents::Untracked` arm in
/// `compute_status`.
#[test]
fn agree_untracked_file() {
    if !have_git() {
        return;
    }
    let scratch = scratch_for("untracked");
    assert!(init_repo_with_commits(&scratch, &["a.txt"]));
    std::fs::write(scratch.join("uncommitted.txt"), b"hi\n").expect("write untracked");
    assert_agree(
        "untracked",
        &scratch,
        true,
        Some("main"),
        Some((0, 0, 1, false)),
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Repo with a tracked file modified in the worktree but not staged.
#[test]
fn agree_modified_tracked_file() {
    if !have_git() {
        return;
    }
    let scratch = scratch_for("modified");
    assert!(init_repo_with_commits(&scratch, &["a.txt"]));
    std::fs::write(scratch.join("a.txt"), b"changed\n").expect("modify a.txt");
    assert_agree(
        "modified",
        &scratch,
        true,
        Some("main"),
        Some((0, 1, 0, false)),
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Repo with a freshly staged file (no commit on top).
#[test]
fn agree_staged_file() {
    if !have_git() {
        return;
    }
    let scratch = scratch_for("staged");
    assert!(init_repo_with_commits(&scratch, &["a.txt"]));
    std::fs::write(scratch.join("b.txt"), b"new\n").expect("write b.txt");
    assert!(git(&scratch, &["add", "b.txt"]));
    assert_agree(
        "staged",
        &scratch,
        true,
        Some("main"),
        Some((1, 0, 0, false)),
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Detached HEAD: branch field DIVERGES (ShellOut → `"HEAD"`,
/// GixBackend → short OID) per the module doccomment. Assert only
/// `dirty == false`. Documenting the divergence here in code makes
/// "fix ShellOut to render short OID" a discoverable follow-up.
#[test]
fn agree_dirty_only_on_detached_head() {
    if !have_git() {
        return;
    }
    let scratch = scratch_for("detached");
    assert!(init_repo_with_commits(&scratch, &["a.txt", "b.txt"]));
    // Detach by checking out the parent commit directly.
    assert!(git(&scratch, &["checkout", "-q", "--detach", "HEAD~1"]));

    // Sanity-check that the divergence on `branch` is real, so the
    // assertion-skipping in `assert_agree` is justified by the test
    // itself rather than only by the module doccomment. If a future
    // ShellOut refactor lifts the detached-HEAD rendering to a short
    // OID, this test will FAIL — at which point the right move is
    // to delete this divergence and use `assert_agree(..., Some(<oid>))`.
    let shell_out = ShellOut.status(&scratch).expect("shellout");
    let gix_out = GixBackend.status(&scratch).expect("gix");
    assert_eq!(shell_out.branch.as_str(), "HEAD");
    assert_ne!(gix_out.branch.as_str(), "HEAD");
    assert_eq!(gix_out.branch.as_str().len(), 7);

    // Detached HEAD with a clean worktree: counts agree even though
    // branch diverges.
    assert_agree("detached", &scratch, false, None, Some((0, 0, 0, false)));
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Merge conflict: both backends must report `has_conflicts=true`
/// and zero in the staged / unstaged / untracked counters
/// (conflicts go in their own bucket; matches `git status`'s
/// "Unmerged paths" section). Setup forces a real merge that
/// touches the same line of the same file from two branches.
#[test]
fn agree_merge_conflict_reports_has_conflicts() {
    if !have_git() {
        return;
    }
    let scratch = scratch_for("conflict");
    std::fs::create_dir_all(&scratch).expect("mkdir scratch");
    assert!(git(&scratch, &["init", "-q", "-b", "main"]), "git init");
    std::fs::write(scratch.join("a.txt"), b"common\n").expect("write a.txt");
    assert!(git(&scratch, &["add", "a.txt"]), "git add");
    assert!(git(&scratch, &["commit", "-q", "-m", "base"]), "git commit");

    // Branch off and make a conflicting change.
    assert!(
        git(&scratch, &["checkout", "-q", "-b", "feature"]),
        "checkout -b feature"
    );
    std::fs::write(scratch.join("a.txt"), b"feature\n").expect("write feature");
    assert!(
        git(&scratch, &["commit", "-q", "-am", "feat"]),
        "commit feat"
    );

    // Back to main, make a different change to the same line.
    assert!(git(&scratch, &["checkout", "-q", "main"]), "checkout main");
    std::fs::write(scratch.join("a.txt"), b"main\n").expect("write main");
    assert!(
        git(&scratch, &["commit", "-q", "-am", "main"]),
        "commit main"
    );

    // Merge — produces a conflict in a.txt; git exits non-zero so
    // we don't assert success on this one. The repo is now in
    // "in the middle of a conflicting merge" state.
    let _ = git(&scratch, &["merge", "feature"]);

    // Both backends must agree: has_conflicts=true, all counters
    // zero (conflicted entries don't bump the staged/unstaged/
    // untracked buckets per the parser's documented discipline).
    assert_agree(
        "conflict",
        &scratch,
        true,
        Some("main"),
        Some((0, 0, 0, true)),
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Ahead-of-upstream: ShellOut and GixBackend both agree the repo
/// is clean. (ShellOut doesn't track ahead/behind, so we don't
/// cross-check those numbers — the GixBackend ahead/behind unit
/// tests in `gix_backend.rs` cover them on the gix side.)
#[test]
fn agree_clean_with_upstream_set() {
    if !have_git() {
        return;
    }
    let scratch = scratch_for("upstream");
    assert!(init_repo_with_commits(
        &scratch,
        &["a.txt", "b.txt", "c.txt"]
    ));
    // Set up an `origin` upstream pointing at HEAD~2 so GixBackend
    // computes ahead=2. ShellOut doesn't surface that count, but
    // `dirty` must still be false on both — the worktree is clean.
    for cfg in [
        ("remote.origin.url", "none://placeholder"),
        ("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"),
        ("branch.main.remote", "origin"),
        ("branch.main.merge", "refs/heads/main"),
    ] {
        assert!(git(&scratch, &["config", cfg.0, cfg.1]));
    }
    assert!(git(
        &scratch,
        &["update-ref", "refs/remotes/origin/main", "HEAD~2"]
    ));

    assert_agree(
        "upstream",
        &scratch,
        false,
        Some("main"),
        Some((0, 0, 0, false)),
    );

    // Additional GixBackend-side sanity check: confirm ahead/behind
    // computed correctly on this fixture so cross-check + the
    // GixBackend-only count are linked in one place.
    let gix_out = GixBackend.status(&scratch).expect("gix");
    assert_eq!(gix_out.ahead, 2, "[upstream] GixBackend.ahead");
    assert_eq!(gix_out.behind, 0, "[upstream] GixBackend.behind");

    let _ = std::fs::remove_dir_all(&scratch);
}
