//! Acceptance gate: the three impls must agree on the 8 load-bearing fields.
//!
//! Without this, no benchmark number is meaningful — a fast wrong answer
//! is worthless (contractor brief § "Algorithmic correctness > raw speed").
//!
//! The test creates a small repo on tmpfs with a deterministic shape
//! (one staged file, one unstaged modification, one untracked file, no
//! conflicts, no upstream), runs all three impls against it, and asserts
//! field equality. The `gitstatusd` baseline is auto-skipped if the daemon
//! binary isn't on this host — on those machines we still gate gix-only
//! against hybrid, which catches drift between our two Rust impls.

use std::fs;
use std::path::Path;
use std::process::Command;

use spike_gitstatus::{gitstatusd_baseline, gix_path, hybrid, GitStatusSummary};

fn run_git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_AUTHOR_NAME", "spike")
        .env("GIT_AUTHOR_EMAIL", "spike@example.com")
        .env("GIT_COMMITTER_NAME", "spike")
        .env("GIT_COMMITTER_EMAIL", "spike@example.com")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .status()
        .expect("spawn git");
    assert!(status.success(), "git {args:?} failed in {repo:?}");
}

fn make_fixture(root: &Path) {
    fs::create_dir_all(root).unwrap();
    run_git(root, &["init", "--initial-branch=main", "--quiet"]);

    fs::write(root.join("README.md"), b"hello\n").unwrap();
    run_git(root, &["add", "README.md"]);
    run_git(root, &["commit", "-m", "initial", "--quiet"]);

    // One staged change: new file added to the index but not committed.
    fs::write(root.join("staged.txt"), b"staged\n").unwrap();
    run_git(root, &["add", "staged.txt"]);

    // One unstaged modification: existing tracked file edited.
    fs::write(root.join("README.md"), b"hello\nworld\n").unwrap();

    // One untracked file.
    fs::write(root.join("untracked.txt"), b"untracked\n").unwrap();
}

/// Compare the fields all three impls must agree on. The gitstatusd baseline
/// can carry repo state our Rust impls can't (push remote etc.) but for the
/// 8 load-bearing fields we demand exact match.
fn assert_summaries_agree(label: &str, a: &GitStatusSummary, b: &GitStatusSummary) {
    assert_eq!(a.branch, b.branch, "{label}: branch");
    assert_eq!(a.commit, b.commit, "{label}: commit");
    assert_eq!(a.ahead, b.ahead, "{label}: ahead");
    assert_eq!(a.behind, b.behind, "{label}: behind");
    assert_eq!(a.staged_count, b.staged_count, "{label}: staged_count");
    assert_eq!(
        a.unstaged_count, b.unstaged_count,
        "{label}: unstaged_count"
    );
    assert_eq!(
        a.untracked_count, b.untracked_count,
        "{label}: untracked_count"
    );
    assert_eq!(a.has_conflicts, b.has_conflicts, "{label}: has_conflicts");
}

#[test]
fn three_impls_agree_on_simple_fixture() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    make_fixture(root);

    let g = gix_path::status(root).expect("gix-only");
    let h = hybrid::status(root).expect("hybrid");

    assert_summaries_agree("gix-only vs hybrid", &g, &h);

    if gitstatusd_baseline::is_available() {
        let d = gitstatusd_baseline::status(root).expect("baseline");
        assert_summaries_agree("hybrid vs baseline", &h, &d);
    } else {
        eprintln!(
            "[correctness] gitstatusd binary not found — gating only \
             gix-only vs hybrid. Install gitstatusd to enable the full gate."
        );
    }
}
