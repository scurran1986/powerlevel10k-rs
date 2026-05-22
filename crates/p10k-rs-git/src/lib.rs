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

use std::io;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use p10k_rs_core::safety::SafeText;
use p10k_rs_core::GitState;

pub mod gitstatusd;
pub use gitstatusd::{
    locate_binary as locate_gitstatusd, locate_binary_checked as locate_gitstatusd_checked,
    Gitstatusd, LocateError as LocateGitstatusdError,
};

pub mod pins;
pub use pins::{detect_host_triple, PinEntry, Pins, PinsError, PINS_TOML};

pub mod gix_backend;
pub use gix_backend::GixBackend;

/// Wall-clock budget for the [`ShellOut`] `git status` spawn.
///
/// Mirrors the [`gitstatusd::DEFAULT_TIMEOUT`] budget. A malicious `.git/`
/// (fsmonitor exploit, hung helper, slow filesystem) must not stall the
/// prompt indefinitely — past this deadline we kill the child and return
/// [`ShellOutError::Timeout`] so the caller can move on. T0.4.
const SHELLOUT_TIMEOUT: Duration = Duration::from_secs(2);

/// Poll cadence for [`std::process::Child::try_wait`] while waiting on the
/// shell-out `git` child. 25 ms keeps wake-ups cheap on the common
/// sub-100-ms case while still giving us 80 chances to notice completion
/// inside the 2 s deadline.
const SHELLOUT_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Errors the [`ShellOut`] backend can surface from the spawn path.
///
/// `Backend::status` continues to return `Option<GitState>` so the prompt's
/// fallback chain stays branchless on the happy path; the typed errors are
/// used by [`ShellOut::status_with_deadline`] and the unit tests so a
/// timeout is distinguishable from a `not-a-repo` non-zero exit.
#[derive(Debug, thiserror::Error)]
pub enum ShellOutError {
    /// `git` couldn't be spawned, or polling/waiting on the child failed.
    #[error("git spawn failed: {0}")]
    Spawn(#[from] io::Error),

    /// The child outran the wall-clock deadline and was killed. The prompt
    /// falls back to "no git state" rather than blocking the shell.
    #[error("git status exceeded {0:?} wall-clock deadline; child killed")]
    Timeout(Duration),
}

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
        // dirty-flag from any subsequent lines. The deadline-wrapped spawn
        // (T0.4) returns `Err` on timeout/io failure; we collapse to `None`
        // here to keep the fallback chain branchless, and a non-zero exit
        // (e.g. `not a repo`) also collapses to `None`.
        let out = Self::status_with_deadline(path, SHELLOUT_TIMEOUT).ok()?;
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

impl ShellOut {
    /// Spawn `git status --porcelain=v1 --branch` against `path` and wait
    /// up to `deadline` for it to finish.
    ///
    /// Implementation note: we poll [`std::process::Child::try_wait`] on a
    /// 25 ms cadence rather than spending an OS thread on a blocking
    /// `wait_with_output`, because the slow case we're defending against is
    /// a wedged child — not a slow one. On timeout, we send `SIGKILL` via
    /// [`std::process::Child::kill`] and then drain stdout/stderr with a
    /// blocking `wait_with_output` so the post-kill `read(2)` returns EOF
    /// promptly rather than leaving zombie pipe fds attached to the
    /// process. The drained bytes are discarded — the only signal we
    /// return on timeout is [`ShellOutError::Timeout`].
    ///
    /// The non-timeout error path collapses ordinary `io::Error`s
    /// (spawn-failed, EBADF on the pipe, etc.) into
    /// [`ShellOutError::Spawn`].
    pub fn status_with_deadline(path: &Path, deadline: Duration) -> Result<Output, ShellOutError> {
        let mut cmd = Command::new("git");
        cmd.arg("-C")
            .arg(path)
            .args(["status", "--porcelain=v1", "--branch", "--no-renames"]);
        apply_hardened_git_env(&mut cmd);
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        wait_with_deadline(&mut child, deadline)
    }
}

/// Apply defensive environment overrides to a `git` [`Command`] before spawn.
///
/// Belt-and-braces alongside the [`SHELLOUT_TIMEOUT`] (T0.4). The prompt is
/// run on every keypress in untrusted cwds; a leaked parent env var or an
/// attacker-controlled `.git/config` that turns on `core.fsmonitor` must not
/// be able to escalate beyond a slow `git status`. T1.19.
///
/// Overrides applied:
///
/// - `LC_ALL=C` — keep `git status --porcelain=v1` output stable for
///   parsing regardless of the user's locale.
/// - `GIT_CEILING_DIRECTORIES=` (empty) — prevents `git` from walking
///   *up* from `-C <path>` to discover a parent `.git`. A `.git`
///   planted at `/tmp/attacker/` can't trump the requested path.
/// - `GIT_OPTIONAL_LOCKS=0` — `git status` will not take optional write
///   locks (index refresh, fsmonitor handshake). Reduces the wedge
///   surface and avoids gid-race classes.
/// - `GIT_TERMINAL_PROMPT=0` — never prompt for credentials. The prompt
///   runs without a controlling TTY of its own; a prompt attempt would
///   either hang (until the [`SHELLOUT_TIMEOUT`] kill) or steal input
///   from the user's shell.
/// - `GIT_CONFIG_NOSYSTEM=1` — skip `/etc/gitconfig`. Defends against
///   an admin-poisoned or package-shipped system config injecting a
///   `core.fsmonitor` or similar exploit vector.
fn apply_hardened_git_env(cmd: &mut Command) {
    cmd.env("LC_ALL", "C")
        .env("GIT_CEILING_DIRECTORIES", "")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1");
}

/// Poll `child` until exit or `deadline` elapses.
///
/// On exit, read stdout (stderr is `Stdio::null` at the spawn site, so
/// nothing to drain there) and reap the child via [`std::process::Child::wait`].
/// On deadline, send `SIGKILL`, then `wait()` to reap the zombie before
/// returning [`ShellOutError::Timeout`]. The kernel side of the stdout
/// `pipe(2)` is released when `child` is dropped by the caller; we
/// deliberately do not read post-kill stdout — any bytes the child
/// emitted before being killed are discarded along with the error.
fn wait_with_deadline(
    child: &mut std::process::Child,
    deadline: Duration,
) -> Result<Output, ShellOutError> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            // Child exited under the deadline. Drain stdout (stderr is
            // null) and return the same shape `Command::output` would.
            let mut stdout_buf = Vec::new();
            if let Some(mut stdout) = child.stdout.take() {
                use std::io::Read;
                stdout.read_to_end(&mut stdout_buf)?;
            }
            return Ok(Output {
                status,
                stdout: stdout_buf,
                stderr: Vec::new(),
            });
        }
        if start.elapsed() >= deadline {
            // Best-effort kill, then reap. Both ignore-on-error:
            // `kill` returns `InvalidInput` if the child already
            // exited in the race window, and `wait` only matters
            // for zombie reaping — we already know we're timing out.
            let _ = child.kill();
            let _ = child.wait();
            return Err(ShellOutError::Timeout(deadline));
        }
        thread::sleep(SHELLOUT_POLL_INTERVAL);
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
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(path).args(["rev-parse", "--git-dir"]);
    apply_hardened_git_env(&mut cmd);
    let out = cmd
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
/// `MERGE_HEAD`) resolve to a single, stable label rather than oscillating
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
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
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

    // ---- T1.19: defensive env on ShellOut spawn ------------------------

    /// `apply_hardened_git_env` must wire every documented override onto
    /// the [`Command`]. Use `Command::get_envs()` to capture the env-tuple
    /// list rather than spawning, so the assertion is hermetic and runs
    /// even on systems without `git` on `$PATH`.
    #[test]
    fn apply_hardened_git_env_sets_documented_overrides() {
        use std::collections::BTreeMap;
        use std::ffi::OsStr;

        let mut cmd = Command::new("git");
        apply_hardened_git_env(&mut cmd);

        let envs: BTreeMap<&OsStr, Option<&OsStr>> = cmd.get_envs().collect();

        let expected: &[(&str, &str)] = &[
            ("LC_ALL", "C"),
            ("GIT_CEILING_DIRECTORIES", ""),
            ("GIT_OPTIONAL_LOCKS", "0"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
        ];

        for (k, v) in expected {
            let got = envs
                .get(OsStr::new(*k))
                .unwrap_or_else(|| panic!("env override {k} missing from spawn"));
            let got = got.unwrap_or_else(|| panic!("env override {k} marked for removal, not set"));
            assert_eq!(got, OsStr::new(*v), "env override {k} has wrong value");
        }
    }

    // ---- T0.4: wall-clock timeout on ShellOut spawns -------------------

    /// A child that finishes under the deadline returns its [`Output`]
    /// normally. Uses `printf` so we also confirm stdout is captured the
    /// same way `Command::output` would capture it.
    #[test]
    fn wait_with_deadline_returns_output_when_child_completes() {
        let mut child = Command::new("printf")
            .arg("hello")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn printf");

        let out = wait_with_deadline(&mut child, Duration::from_secs(2))
            .expect("printf completed under deadline");
        assert!(out.status.success(), "printf exited non-zero");
        assert_eq!(out.stdout, b"hello");
        assert!(out.stderr.is_empty(), "stderr was Stdio::null");
    }

    /// A child that outruns the deadline is killed and the typed timeout
    /// error is returned. `sleep 5` against a 100 ms deadline is the
    /// slow-child fixture; the whole test must complete in well under
    /// `sleep 5`'s 5 s budget to prove the deadline actually fired.
    #[test]
    fn wait_with_deadline_kills_slow_child_and_returns_timeout() {
        let deadline = Duration::from_millis(100);
        let mut child = Command::new("sleep")
            .arg("5")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sleep 5");

        let started = Instant::now();
        let err =
            wait_with_deadline(&mut child, deadline).expect_err("sleep 5 must hit the deadline");
        let elapsed = started.elapsed();

        match err {
            ShellOutError::Timeout(d) => assert_eq!(d, deadline),
            other @ ShellOutError::Spawn(_) => panic!("expected Timeout, got {other:?}"),
        }
        // Generous upper bound: 1 s is still ~5x faster than `sleep 5`'s
        // natural exit, so a pass here proves the kill path ran.
        assert!(
            elapsed < Duration::from_secs(1),
            "deadline didn't fire promptly: elapsed = {elapsed:?}"
        );
        // Reap-confirmation: `try_wait` after the timeout path must say
        // the child is gone (`Some(_)`), proving we actually waited on it.
        let reaped = child.try_wait().expect("try_wait after timeout").is_some();
        assert!(reaped, "child wasn't reaped after timeout");
    }
}
