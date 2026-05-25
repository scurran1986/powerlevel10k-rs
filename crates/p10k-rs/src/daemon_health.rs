//! `p10k-rs daemon-health` subcommand (slice 64 phase 4).
//!
//! Diagnostic surface for the slice 64 daemon-respawn channel. Reports
//! whether the per-shell gitstatusd daemon is healthy, wedged, or
//! dead — useful when a user notices the prompt feels slow and wants
//! to confirm whether they're paying the `ShellOut` fallback cost
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

    /// Machine-readable JSON representation. Field set per variant:
    ///
    /// - `Ok`:        `{"status":"OK","pid":<n>,"wedge":null}`
    /// - `Wedged`:    `{"status":"WEDGED","pid":<n>,"wedge_age_ms":<n>}`
    /// - `Dead`:      `{"status":"DEAD","pid":<n>}`
    /// - `NotWired`:  `{"status":"NOT_WIRED"}`
    /// - `Error`:     `{"status":"ERROR","reason":"<escaped>"}`
    ///
    /// Hand-rolled because the binary doesn't otherwise depend on
    /// `serde_json` and the surface is bounded: status is one of five
    /// known strings, pid is `i32`, `wedge_age_ms` is `u128`, and
    /// `reason` is the only string that needs escaping. JSON-escapes
    /// the reason via [`json_escape`] — see that doc-comment for the
    /// covered cases.
    pub(crate) fn render_json(&self) -> String {
        match self {
            Self::Ok { pid } => {
                format!("{{\"status\":\"OK\",\"pid\":{pid},\"wedge\":null}}")
            }
            Self::Wedged { pid, wedge_age_ms } => {
                format!("{{\"status\":\"WEDGED\",\"pid\":{pid},\"wedge_age_ms\":{wedge_age_ms}}}")
            }
            Self::Dead { pid } => format!("{{\"status\":\"DEAD\",\"pid\":{pid}}}"),
            Self::NotWired => "{\"status\":\"NOT_WIRED\"}".to_owned(),
            Self::Error(msg) => {
                format!(
                    "{{\"status\":\"ERROR\",\"reason\":\"{}\"}}",
                    json_escape(msg)
                )
            }
        }
    }
}

/// Minimal JSON string escape — quote, backslash, and the C0
/// controls. Sufficient for the `DaemonHealth::Error` reason field,
/// whose payloads come from `std::io::Error` messages and PID-parse
/// errors. Non-ASCII bytes pass through as-is (UTF-8 is valid JSON);
/// the function is intentionally NOT a general-purpose JSON encoder.
fn json_escape(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                // `write!` on a `String` is infallible — the `fmt::Write`
                // impl for `String` never returns Err. Discarding via
                // `let _ =` keeps `clippy::format_push_string` quiet
                // without adding a panic surface.
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
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
    // Silence stderr so a dead-pid probe doesn't print "kill: (X): No
    // such process" onto the user's terminal — they're running this
    // command to *diagnose* a dead daemon, not to see /bin/kill's
    // editorial on the situation.
    std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `p10k-rs daemon-health` entry point — wired from `main.rs`. Prints
/// the one-line outcome to stdout and returns the corresponding exit
/// code. The caller is responsible for `std::process::exit` dispatch
/// so this stays testable; we return the `i32` rather than calling
/// `exit` ourselves.
///
/// Infallible by construction — every path through `probe` lands on a
/// `DaemonHealth` variant (including the `Error` variant for I/O
/// failures), so there's nothing to surface upward as a `Result`.
pub(crate) fn cmd_daemon_health(json: bool) -> i32 {
    let pid_path = std::env::var_os(PID_FILE_ENV).map(PathBuf::from);
    let wedge_path = std::env::var_os(WEDGE_ENV).map(PathBuf::from);
    let outcome = probe(
        pid_path,
        wedge_path,
        pid_is_alive_default,
        SystemTime::now(),
    );
    if json {
        println!("{}", outcome.render_json());
    } else {
        println!("{}", outcome.render());
    }
    outcome.exit_code()
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

    #[test]
    fn render_json_shapes_are_stable() {
        assert_eq!(
            DaemonHealth::Ok { pid: 1 }.render_json(),
            r#"{"status":"OK","pid":1,"wedge":null}"#
        );
        assert_eq!(
            DaemonHealth::Wedged {
                pid: 2,
                wedge_age_ms: 50,
            }
            .render_json(),
            r#"{"status":"WEDGED","pid":2,"wedge_age_ms":50}"#
        );
        assert_eq!(
            DaemonHealth::Dead { pid: 3 }.render_json(),
            r#"{"status":"DEAD","pid":3}"#
        );
        assert_eq!(
            DaemonHealth::NotWired.render_json(),
            r#"{"status":"NOT_WIRED"}"#
        );
        assert_eq!(
            DaemonHealth::Error("io".to_owned()).render_json(),
            r#"{"status":"ERROR","reason":"io"}"#
        );
    }

    #[test]
    fn render_json_negative_pid_is_emitted_unquoted() {
        // pids are i32 — a negative value (rare; would come from a
        // corrupt PID file) must still emit as a bare JSON number,
        // not a string, so downstream parsers don't crash on the
        // type mismatch.
        assert_eq!(
            DaemonHealth::Ok { pid: -1 }.render_json(),
            r#"{"status":"OK","pid":-1,"wedge":null}"#
        );
    }

    #[test]
    fn json_escape_handles_quote_and_backslash() {
        // The two structural chars JSON strings can't contain raw.
        let msg = r#"read pid file: file "path" \ broken"#;
        let out = DaemonHealth::Error(msg.to_owned()).render_json();
        assert_eq!(
            out,
            r#"{"status":"ERROR","reason":"read pid file: file \"path\" \\ broken"}"#
        );
    }

    #[test]
    fn json_escape_handles_control_chars() {
        // \n, \r, \t get their short forms; other C0 (< 0x20) gets \u00xx.
        let msg = "line1\nline2\rtab\there\x07bel";
        let out = DaemonHealth::Error(msg.to_owned()).render_json();
        assert_eq!(
            out,
            r#"{"status":"ERROR","reason":"line1\nline2\rtab\there\u0007bel"}"#
        );
    }

    #[test]
    fn json_escape_passes_utf8_through_unchanged() {
        // Non-ASCII multi-byte UTF-8 is valid in JSON strings; don't
        // \u-escape it (that would inflate the byte count without
        // gaining wire-compatibility).
        let msg = "héllo 世界";
        let out = DaemonHealth::Error(msg.to_owned()).render_json();
        assert_eq!(out, r#"{"status":"ERROR","reason":"héllo 世界"}"#);
    }
}
