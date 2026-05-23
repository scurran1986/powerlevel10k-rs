//! `p10k-rs daemon-health` subcommand (slice 64 phase 4).
//!
//! Diagnostic surface for the slice 64 daemon-respawn channel. Reports
//! whether the per-shell gitstatusd daemon is healthy, wedged, or
//! dead — useful when a user notices the prompt feels slow and wants
//! to confirm whether they're paying the ShellOut fallback cost
//! because the daemon got into a bad state.
//!
//! Output is one stable line per outcome with distinct exit codes so
//! a shell script can branch on the result without parsing the text:
//!
//! | Outcome      | Stdout                                          | Exit |
//! |--------------|-------------------------------------------------|------|
//! | Healthy      | `OK pid=<pid> wedge=none`                       | 0    |
//! | Wedge sentinel exists | `WEDGED pid=<pid> wedge_age_ms=<n>`    | 2    |
//! | Daemon dead  | `DEAD pid=<pid>`                                | 3    |
//! | Channel not wired (env vars missing) | `NOT_WIRED`             | 4    |
//! | I/O error reading state | `ERROR <reason>`                     | 5    |
//!
//! The channel that this subcommand inspects is exported by the zsh
//! init script (phase 1) — `_P10K_RS_GITSTATUSD_PID_FILE` and
//! `_P10K_RS_GITSTATUSD_WEDGE`. Outside an interactive zsh that
//! sourced `p10k-rs init zsh`, both env vars are unset and the
//! subcommand correctly reports `NOT_WIRED`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::Result;

/// Env var holding the per-shell daemon PID file path (phase 1).
const PID_FILE_ENV: &str = "_P10K_RS_GITSTATUSD_PID_FILE";

/// Env var holding the per-shell wedge sentinel path (phase 1).
const WEDGE_ENV: &str = "_P10K_RS_GITSTATUSD_WEDGE";

/// Outcome of a daemon-health probe. `exit_code` and `render` together
/// form the subcommand's full wire contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DaemonHealth {
    /// Daemon is alive and there is no wedge sentinel.
    Ok { pid: i32 },
    /// Daemon is alive but a wedge sentinel exists. The age (in ms
    /// since the sentinel's mtime) is included so the user can tell
    /// "wedged just now" from "wedge marker left over from earlier."
    Wedged { pid: i32, wedge_age_ms: u128 },
    /// PID file exists and parses, but `kill -0 $pid` fails (process
    /// is gone or no longer signalable by this UID).
    Dead { pid: i32 },
    /// Either `_P10K_RS_GITSTATUSD_PID_FILE` or `_P10K_RS_GITSTATUSD_WEDGE`
    /// is unset, so the slice-64 channel was never wired in this shell.
    NotWired,
    /// PID file or wedge sentinel exists but couldn't be read /
    /// stat'd / parsed. Surfaces the underlying reason for paste-back.
    Error(String),
}

impl DaemonHealth {
    /// Exit code the subcommand returns for this outcome. See module
    /// docs for the table.
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            Self::Ok { .. } => 0,
            Self::Wedged { .. } => 2,
            Self::Dead { .. } => 3,
            Self::NotWired => 4,
            Self::Error(_) => 5,
        }
    }

    /// One-line stdout representation. Stable wire format — keep the
    /// shape compatible across versions so user scripts don't break.
    pub(crate) fn render(&self) -> String {
        match self {
            Self::Ok { pid } => format!("OK pid={pid} wedge=none"),
            Self::Wedged { pid, wedge_age_ms } => {
                format!("WEDGED pid={pid} wedge_age_ms={wedge_age_ms}")
            }
            Self::Dead { pid } => format!("DEAD pid={pid}"),
            Self::NotWired => "NOT_WIRED".to_owned(),
            Self::Error(msg) => format!("ERROR {msg}"),
        }
    }
}

/// Probe the slice-64 daemon-respawn channel by reading the env vars
/// the zsh init script exports at daemon spawn, then inspecting the
/// PID file and wedge sentinel. Pure function — no side effects on
/// the filesystem; safe to call repeatedly.
fn probe(
    pid_file_path: Option<PathBuf>,
    wedge_path: Option<PathBuf>,
    is_alive: impl Fn(i32) -> bool,
    now: SystemTime,
) -> DaemonHealth {
    let (Some(pid_path), Some(wedge_path)) = (pid_file_path, wedge_path) else {
        return DaemonHealth::NotWired;
    };

    let pid = match read_pid(&pid_path) {
        Ok(n) => n,
        Err(err) => return DaemonHealth::Error(err),
    };

    if !is_alive(pid) {
        return DaemonHealth::Dead { pid };
    }

    match wedge_age(&wedge_path, now) {
        Ok(Some(age)) => DaemonHealth::Wedged {
            pid,
            wedge_age_ms: age.as_millis(),
        },
        Ok(None) => DaemonHealth::Ok { pid },
        Err(err) => DaemonHealth::Error(err),
    }
}

/// Read the PID file at `path` and parse its contents as an i32.
/// Trims trailing whitespace (the zsh `print -r --` writes a newline)
/// and rejects empty / non-numeric content with a descriptive error.
fn read_pid(path: &Path) -> std::result::Result<i32, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read pid file: {e}"))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("pid file {} is empty", path.display()));
    }
    trimmed.parse::<i32>().map_err(|e| {
        format!(
            "pid file {} content {trimmed:?} is not an i32: {e}",
            path.display()
        )
    })
}

/// If the wedge sentinel exists, return its age relative to `now`.
/// Returns `Ok(None)` if the sentinel doesn't exist (the healthy
/// case). Future mtimes (clock skew) report `Some(Duration::ZERO)`
/// rather than erroring — the user wants to know the sentinel exists,
/// not get a stack trace about a backwards clock.
fn wedge_age(path: &Path, now: SystemTime) -> std::result::Result<Option<Duration>, String> {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("stat wedge sentinel: {e}")),
    };
    let mtime = meta
        .modified()
        .map_err(|e| format!("read wedge mtime: {e}"))?;
    Ok(Some(now.duration_since(mtime).unwrap_or(Duration::ZERO)))
}

/// Check whether `pid` references a process this UID can signal. Uses
/// `kill -0 <pid>` via a subprocess to avoid an `unsafe` libc call in
/// the binary crate. The fork+exec cost (~ms) is invisible against a
/// diagnostic command the user runs manually.
fn pid_is_alive_default(pid: i32) -> bool {
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `p10k-rs daemon-health` entry point — wired from `main.rs`. Prints
/// the one-line outcome to stdout and returns the corresponding exit
/// code via `std::process::exit` from the caller.
///
/// The caller is responsible for the exit-code dispatch so this stays
/// testable; we return an `i32` rather than calling `exit` ourselves.
pub(crate) fn cmd_daemon_health() -> Result<i32> {
    let pid_path = std::env::var_os(PID_FILE_ENV).map(PathBuf::from);
    let wedge_path = std::env::var_os(WEDGE_ENV).map(PathBuf::from);
    let outcome = probe(
        pid_path,
        wedge_path,
        pid_is_alive_default,
        SystemTime::now(),
    );
    println!("{}", outcome.render());
    Ok(outcome.exit_code())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_file_with(contents: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "p10krs-daemon-health-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pid");
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(contents).unwrap();
        path
    }

    fn touch(path: &Path) {
        fs::write(path, b"").unwrap();
    }

    /// No env vars → `NOT_WIRED`. The common case outside an
    /// interactive zsh.
    #[test]
    fn probe_reports_not_wired_when_paths_unset() {
        let out = probe(None, None, |_| true, SystemTime::now());
        assert_eq!(out, DaemonHealth::NotWired);
        assert_eq!(out.exit_code(), 4);
        assert_eq!(out.render(), "NOT_WIRED");
    }

    /// Half-wired (one env var set, the other unset) is treated the
    /// same as fully unwired — both halves are required for a sane
    /// answer.
    #[test]
    fn probe_reports_not_wired_when_only_one_path_set() {
        let pid_path = tmp_file_with(b"1\n");
        let out_a = probe(Some(pid_path.clone()), None, |_| true, SystemTime::now());
        assert_eq!(out_a, DaemonHealth::NotWired);

        let wedge_path = pid_path.parent().unwrap().join("wedge");
        let out_b = probe(None, Some(wedge_path), |_| true, SystemTime::now());
        assert_eq!(out_b, DaemonHealth::NotWired);
    }

    /// PID alive, no wedge sentinel → `OK`. Healthy steady state.
    #[test]
    fn probe_reports_ok_when_pid_alive_no_wedge() {
        let pid_path = tmp_file_with(b"12345\n");
        let wedge_path = pid_path.parent().unwrap().join("wedge");
        let out = probe(
            Some(pid_path),
            Some(wedge_path),
            |pid| {
                assert_eq!(pid, 12345);
                true
            },
            SystemTime::now(),
        );
        assert_eq!(out, DaemonHealth::Ok { pid: 12345 });
        assert_eq!(out.exit_code(), 0);
        assert_eq!(out.render(), "OK pid=12345 wedge=none");
    }

    /// PID alive AND wedge sentinel exists → `Wedged` with age in ms.
    /// The age check is the user-facing diagnostic: "wedged just now"
    /// is the slice-64 fast-bail path; "wedged 30s ago and still
    /// there" suggests the respawn precmd never fired.
    #[test]
    fn probe_reports_wedged_when_sentinel_exists_and_pid_alive() {
        let pid_path = tmp_file_with(b"42\n");
        let wedge_path = pid_path.parent().unwrap().join("wedge");
        touch(&wedge_path);
        let out = probe(
            Some(pid_path),
            Some(wedge_path),
            |_| true,
            SystemTime::now(),
        );
        match out {
            DaemonHealth::Wedged { pid, wedge_age_ms } => {
                assert_eq!(pid, 42);
                // Sentinel was just created; age should be tiny.
                assert!(
                    wedge_age_ms < 5_000,
                    "wedge_age_ms unexpectedly large: {wedge_age_ms}"
                );
            }
            other => panic!("expected Wedged, got {other:?}"),
        }
    }

    /// PID file says X, `kill -0 X` (mocked) returns false → `Dead`.
    /// This is the "stale PID file, daemon crashed mid-session"
    /// scenario.
    #[test]
    fn probe_reports_dead_when_pid_unreachable() {
        let pid_path = tmp_file_with(b"99999\n");
        let wedge_path = pid_path.parent().unwrap().join("wedge");
        let out = probe(
            Some(pid_path),
            Some(wedge_path),
            |_| false,
            SystemTime::now(),
        );
        assert_eq!(out, DaemonHealth::Dead { pid: 99999 });
        assert_eq!(out.exit_code(), 3);
        assert_eq!(out.render(), "DEAD pid=99999");
    }

    /// Empty / non-numeric / unreadable PID file → `Error` with the
    /// underlying reason. We don't fall through to `Dead` because the
    /// underlying state is "we can't even tell" — the user needs the
    /// diagnostic to know whether to delete the file, fix perms, etc.
    #[test]
    fn probe_reports_error_on_empty_pid_file() {
        let pid_path = tmp_file_with(b"");
        let wedge_path = pid_path.parent().unwrap().join("wedge");
        let out = probe(
            Some(pid_path),
            Some(wedge_path),
            |_| true,
            SystemTime::now(),
        );
        match out {
            DaemonHealth::Error(msg) => assert!(msg.contains("empty"), "got: {msg}"),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn probe_reports_error_on_non_numeric_pid_file() {
        let pid_path = tmp_file_with(b"not-a-pid\n");
        let wedge_path = pid_path.parent().unwrap().join("wedge");
        let out = probe(
            Some(pid_path),
            Some(wedge_path),
            |_| true,
            SystemTime::now(),
        );
        match out {
            DaemonHealth::Error(msg) => assert!(msg.contains("not an i32"), "got: {msg}"),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    /// Future-dated wedge mtime (clock skew, sandbox time injection,
    /// NTP step backwards) → age clamps to zero rather than panicking.
    #[test]
    fn probe_handles_future_wedge_mtime_gracefully() {
        let pid_path = tmp_file_with(b"7\n");
        let wedge_path = pid_path.parent().unwrap().join("wedge");
        touch(&wedge_path);
        // Pretend "now" is in the past — older than the wedge mtime.
        let past = SystemTime::UNIX_EPOCH;
        let out = probe(Some(pid_path), Some(wedge_path), |_| true, past);
        match out {
            DaemonHealth::Wedged { wedge_age_ms, .. } => {
                assert_eq!(wedge_age_ms, 0, "future mtime should clamp to age=0");
            }
            other => panic!("expected Wedged, got {other:?}"),
        }
    }

    /// Render shapes are stable wire format — pin them so a careless
    /// refactor that breaks downstream scripts trips this test.
    #[test]
    fn render_shapes_are_stable() {
        assert_eq!(DaemonHealth::Ok { pid: 1 }.render(), "OK pid=1 wedge=none");
        assert_eq!(
            DaemonHealth::Wedged {
                pid: 2,
                wedge_age_ms: 50,
            }
            .render(),
            "WEDGED pid=2 wedge_age_ms=50"
        );
        assert_eq!(DaemonHealth::Dead { pid: 3 }.render(), "DEAD pid=3");
        assert_eq!(DaemonHealth::NotWired.render(), "NOT_WIRED");
        assert_eq!(DaemonHealth::Error("io".to_owned()).render(), "ERROR io");
    }
}
