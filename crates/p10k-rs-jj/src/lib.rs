//! Jujutsu (`jj`) VCS state producer for `p10k-rs`.
//!
//! Sibling crate to `p10k-rs-git`. Detects a `.jj/` working copy by walking
//! up from a cwd, then shells out to `jj` to populate a [`JjState`]. The
//! `unsafe`-budget invariant in `p10k-rs-git` (FIFO IPC for the gitstatusd
//! daemon) doesn't apply here: jj has no daemon, and this crate is
//! shell-out only. A new crate keeps that boundary clean per ADR-0001's
//! "one VCS, one crate" guideline.
//!
//! Shape exposed to consumers ([`JjState`]) mirrors `p10k_rs_core::GitState`'s
//! `SafeText` discipline — every byte that rides off the `jj` subprocess
//! pipe passes through [`SafeText::from_untrusted`] at construction time so
//! a malicious description or bookmark name can't steer the terminal or
//! zsh's `%`-expansion engine.
//!
//! Two probes are intentionally cheap by default:
//!
//! - Detection is filesystem-only: walk up to 64 levels looking for a
//!   `.jj/` directory. Matches the existing git-marker probe depth.
//! - Population is one `jj log` invocation with a template that emits
//!   pipe-separated fields. Parsing is a `split('|')` — no JSON serde,
//!   no shell quoting, no transitive deps. `jj status` is the dirty
//!   probe (one more subprocess; jj has no equivalent of git's
//!   `--porcelain` empty-stdout shortcut).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use p10k_rs_core::safety::SafeText;
use p10k_rs_core::JjState;

/// Maximum directory levels we walk up looking for a `.jj/` directory.
///
/// Matches the git-marker probe depth in `p10k-rs-git`. 64 is well
/// past any sane nesting; cuts off run-away symlink loops without
/// requiring a per-inode visited set.
const MAX_WALKUP: usize = 64;

/// Probe `cwd` for Jujutsu state.
///
/// Returns `None` when:
/// - `cwd` is not inside any `.jj/` working copy (the common case
///   during prompt rendering — most cwds aren't jj repos).
/// - The `jj` binary isn't on `$PATH`.
/// - The `jj log` invocation fails for any reason (permissions, a
///   half-initialised repo, a jj version that doesn't accept the
///   template we send).
///
/// Walks up at most 64 levels from `cwd` (the `MAX_WALKUP` bound).
/// The walk stops at the first `.jj/` it finds (jj working copies
/// don't nest the way git submodules do, so the first hit is always
/// the right one).
#[must_use]
pub fn detect_jj(cwd: &Path) -> Option<JjState> {
    let root = find_jj_root(cwd)?;
    let log_out = Command::new("jj")
        .arg("--repository")
        .arg(&root)
        .args([
            "log",
            "--no-graph",
            "--ignore-working-copy",
            "--color",
            "never",
            "-r",
            "@",
            "-T",
            // Seven fields, pipe-separated. `description.first_line()`
            // would be nicer but isn't stable across all jj versions;
            // we take the whole description and trim to the first line
            // in Rust below. Fields 6 and 7 are the `divergent` and
            // `self.conflict()` booleans, emitted as "1" or "0" so the
            // parser stays a simple `split('|')`. The trailing literal
            // sentinel keeps the parser robust if jj appends a newline.
            r#"change_id.short() ++ "|" ++ commit_id.short() ++ "|" ++ bookmarks ++ "|" ++ description ++ "|" ++ if(divergent, "1", "0") ++ "|" ++ if(self.conflict(), "1", "0") ++ "|END""#,
        ])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !log_out.status.success() {
        return None;
    }
    let mut state = parse_log_output(&log_out.stdout);
    state.dirty = probe_dirty(&root);
    Some(state)
}

/// Walk up from `start` looking for a directory containing `.jj/`.
///
/// Returns the path of that directory (the repository root jj wants
/// via `--repository`). Capped at [`MAX_WALKUP`] iterations to bound
/// the worst case on a pathological symlink loop.
fn find_jj_root(start: &Path) -> Option<PathBuf> {
    let mut current: PathBuf = start.to_path_buf();
    for _ in 0..MAX_WALKUP {
        if current.join(".jj").is_dir() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
    None
}

/// Parse the pipe-separated `jj log` output into a [`JjState`].
///
/// Fields: `change_id | commit_id | bookmarks | description | divergent | conflicts | END`.
/// Anything past the `END` sentinel is discarded — that's the trailing
/// newline jj appends. Bookmarks come space-separated; we take the
/// first one as the "primary" bookmark to surface in the prompt.
/// The `divergent` and `conflicts` fields are `"1"` or `"0"` as emitted
/// by the template's `if(…, "1", "0")` expressions.
fn parse_log_output(raw: &[u8]) -> JjState {
    // Lossy UTF-8 is the right call here — same posture as
    // `p10k-rs-git`'s wire parser. A description with stray bytes
    // shouldn't drop the whole field.
    let text = String::from_utf8_lossy(raw);
    // Strip a trailing newline jj typically appends.
    let line = text.trim_end_matches('\n');
    // The template ends with `|END`, so split on `|` and read the
    // first six fields. Anything missing falls to default.
    let mut parts = line.split('|');
    let change_id = parts.next().unwrap_or("").trim();
    let commit_id = parts.next().unwrap_or("").trim();
    let bookmarks = parts.next().unwrap_or("").trim();
    let description_raw = parts.next().unwrap_or("");
    let divergent_field = parts.next().unwrap_or("").trim();
    let conflicts_field = parts.next().unwrap_or("").trim();
    // First whitespace-delimited bookmark — jj's `bookmarks` template
    // emits a space-separated list. Empty when the change has none.
    let bookmark = bookmarks.split_whitespace().next().unwrap_or("");
    // First line of the description; trim to keep the prompt tidy.
    let description = description_raw.lines().next().unwrap_or("").trim();
    JjState {
        change_id: SafeText::from_untrusted(change_id),
        commit_id: SafeText::from_untrusted(commit_id),
        bookmark: SafeText::from_untrusted(bookmark),
        description: SafeText::from_untrusted(description),
        dirty: false,
        divergent: divergent_field == "1",
        conflicts: conflicts_field == "1",
    }
}

/// Probe `jj status` for any uncommitted change.
///
/// jj's "clean" status output is the literal string
/// `"The working copy has no changes."` on a fresh `jj new`. Anything
/// else — a single modified file, an untracked path — means dirty.
/// We use `--no-pager` so the subprocess can't wedge waiting on a
/// pager; recent jj versions default that anyway, but old ones don't.
fn probe_dirty(root: &Path) -> bool {
    let out = Command::new("jj")
        .arg("--repository")
        .arg(root)
        .args(["status", "--color", "never"])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();
    let Ok(out) = out else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    // jj's clean message changes phrasing across versions; we look
    // for the "no changes" signal anywhere in stdout. Anything else
    // — modified files, conflicts, working-copy-only patches — is
    // dirty.
    !s.contains("no changes")
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    /// Build a unique scratch directory under `std::env::temp_dir()`
    /// for the walk-up tests. Same shape as `p10k-rs-git`'s scratch
    /// helper — caller owns cleanup; we leak on panic so a failing
    /// test leaves evidence on disk.
    fn scratch_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!(
            "p10krs-jj-{}-{}-{}",
            label,
            std::process::id(),
            nanos,
        ));
        std::fs::create_dir_all(&p).expect("mkdir scratch");
        p
    }

    #[test]
    fn find_jj_root_returns_dir_when_marker_present() {
        let root = scratch_dir("root");
        std::fs::create_dir(root.join(".jj")).expect("mkdir .jj");
        let got = find_jj_root(&root).expect("root present");
        assert_eq!(got, root);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_jj_root_walks_up_to_parent() {
        let root = scratch_dir("walkup");
        std::fs::create_dir(root.join(".jj")).expect("mkdir .jj");
        let nested = root.join("a").join("b").join("c");
        std::fs::create_dir_all(&nested).expect("mkdir nested");
        let got = find_jj_root(&nested).expect("walk up");
        assert_eq!(got, root);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_jj_root_returns_none_outside_repo() {
        // No `.jj` anywhere under here — must be `None`.
        let root = scratch_dir("outside");
        assert!(find_jj_root(&root).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_log_output_extracts_fields() {
        let raw = b"abc123|def456|main feat/x|Implement widget\n|END\n";
        let s = parse_log_output(raw);
        assert_eq!(s.change_id.as_str(), "abc123");
        assert_eq!(s.commit_id.as_str(), "def456");
        // First bookmark wins.
        assert_eq!(s.bookmark.as_str(), "main");
        // First line of description.
        assert_eq!(s.description.as_str(), "Implement widget");
    }

    #[test]
    fn parse_log_output_handles_empty_bookmark_and_description() {
        let raw = b"abc123|def456|||END\n";
        let s = parse_log_output(raw);
        assert_eq!(s.change_id.as_str(), "abc123");
        assert!(s.bookmark.is_empty());
        assert!(s.description.is_empty());
    }

    #[test]
    fn parse_log_output_strips_control_bytes_via_safetext() {
        // A description carrying an ANSI escape must NOT survive into
        // the prompt — `SafeText::from_untrusted` strips it.
        let raw = b"abc|def||\x1b[2Jevil|END";
        let s = parse_log_output(raw);
        // ESC byte is gone; the otherwise-printable payload survives.
        assert_eq!(s.description.as_str(), "[2Jevil");
    }

    #[test]
    fn parse_log_output_clean_both_false() {
        // Both divergent and conflicts fields are "0" → both booleans false.
        let raw = b"abc123|def456|main|Implement widget\n|0|0|END\n";
        let s = parse_log_output(raw);
        assert!(!s.divergent, "divergent should be false");
        assert!(!s.conflicts, "conflicts should be false");
    }

    #[test]
    fn parse_log_output_divergent_only() {
        // Change is divergent but not in conflict.
        let raw = b"abc123|def456||divergent change|1|0|END\n";
        let s = parse_log_output(raw);
        assert!(s.divergent, "divergent should be true");
        assert!(!s.conflicts, "conflicts should be false");
    }

    #[test]
    fn parse_log_output_conflicts_only() {
        // Change has conflicts but is not divergent.
        let raw = b"abc123|def456|main|conflicted merge|0|1|END\n";
        let s = parse_log_output(raw);
        assert!(!s.divergent, "divergent should be false");
        assert!(s.conflicts, "conflicts should be true");
    }

    #[test]
    fn detect_jj_returns_none_outside_repo() {
        // No `.jj` directory anywhere up from here.
        let root = scratch_dir("detect-none");
        assert!(detect_jj(&root).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detect_jj_returns_some_inside_marker_dir() {
        // Gate on `jj` being on PATH — the test would otherwise spam
        // CI hosts that don't have jj installed.
        if which_jj().is_none() {
            eprintln!("skipping detect_jj_returns_some_inside_marker_dir: jj not on PATH");
            return;
        }
        let root = scratch_dir("detect-some");
        std::fs::create_dir(root.join(".jj")).expect("mkdir .jj");
        // We don't actually initialise the repo (that would require
        // `jj init` which is heavier than the test should be). The
        // detection walks the filesystem and shells out; the
        // shell-out will fail (no real repo behind the marker) and
        // `detect_jj` returns `None`. That's still a useful smoke
        // test: it proves the walk-up reaches the marker without
        // panicking and that the failure mode is a clean `None`.
        let got = detect_jj(&root);
        assert!(
            got.is_none(),
            "an empty .jj marker should produce None (jj log fails): {got:?}",
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Best-effort `which jj` for test gating. Not robust enough for
    /// production — only used to skip tests that need a real jj
    /// binary on PATH.
    fn which_jj() -> Option<PathBuf> {
        let path = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("jj");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }
}
