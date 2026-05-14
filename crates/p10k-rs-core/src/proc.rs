//! Subprocess execution with a wall-clock deadline.
//!
//! Slice 51 — addresses the upstream Powerlevel10k NordVPN-freeze pattern
//! (issue `#2860`, 35 reactions): segments that shell out to a third-party
//! tool block the entire prompt when that tool hangs. The classic case is
//! a VPN client that holds the world while it reconnects, but the shape
//! is the same for any `node --version` / `python --version` /
//! `rustc --version` invocation that gets wedged behind a slow filesystem,
//! a hung NFS mount, or a corrupted toolchain.
//!
//! [`output_with_deadline`] is the workspace-shared remedy: spawn the
//! child, poll [`std::process::Child::try_wait`] in a tight loop with a
//! 5 ms sleep, and if the wall clock passes `deadline` before the child
//! exits, kill it and return an [`std::io::ErrorKind::TimedOut`] error.
//! Callers (the version segments today, every future external-process
//! segment tomorrow) translate that into "render nothing" and the
//! prompt stays interactive.
//!
//! ## I/O-free crate trade-off
//!
//! `p10k-rs-core` is otherwise I/O-free by design (see this crate's
//! `CLAUDE.md`). [`terminal_width`](crate::render_prompt) was the first
//! exception (slice 29, `ioctl(TIOCGWINSZ)` for the ruler). This module
//! is the second. The justification mirrors slice 29's: every
//! external-process segment will need the same deadline-on-spawn shape,
//! and a workspace-shared helper saves four-plus near-duplicate copies
//! across `node_version`, `python_version`, `rust_version`, and the
//! kube / docker / aws segments still to come. The helper is pure
//! stdlib — no `tokio`, no extra dep — so it doesn't push the crate's
//! dependency footprint, only its module list.
//!
//! ## Pipe capture
//!
//! The helper sets `stdin(Stdio::null())`, `stdout(Stdio::piped())`,
//! and `stderr(Stdio::piped())` on the [`Command`] itself, overriding
//! whatever the caller configured. This is the simpler of the two
//! designs the spec considered: callers don't have to remember to set
//! up the pipes, and we get a single chokepoint that guarantees we can
//! read the child's output regardless of how the `Command` was
//! assembled. The trade-off is that a caller who *wants*
//! the child to inherit stdout (e.g. for debugging) can't get that here
//! — but no production caller does, and the version segments already
//! piped everything for sanitisation anyway.
//!
//! ## Polling interval
//!
//! 5 ms. Short enough that a fast tool (`node --version` typically
//! 50–150 ms warm) exits on the first or second poll and feels instant;
//! long enough that a child taking the full 500 ms deadline costs at
//! most ~100 wake-ups, which is rounding error against the cost of the
//! spawn itself. A microsecond loop would burn CPU; a 50 ms loop would
//! add visible latency to the common case.

use std::io;
use std::process::{Child, Command, Output, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

/// How often the poll loop checks [`Child::try_wait`].
///
/// 5 ms — see the module docs for the rationale.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Run `cmd` to completion, killing it if it outlives `deadline`.
///
/// Configures the command with `stdin(null)`, `stdout(piped)`, and
/// `stderr(piped)` before spawning — any prior configuration on those
/// three streams is overridden so the helper can always capture the
/// child's output. Other [`Command`] settings (env, args, cwd, etc.)
/// are preserved.
///
/// On success returns the child's full [`Output`] (status + captured
/// stdout / stderr). On deadline expiry, kills the child, reaps it
/// with [`Child::wait`], and returns an [`io::Error`] with
/// [`io::ErrorKind::TimedOut`] so callers can branch on the failure
/// mode without parsing strings.
///
/// # Errors
///
/// - [`io::ErrorKind::TimedOut`] when `deadline` elapses before the
///   child exits. The child is killed and reaped before the error
///   returns, so no zombie is left behind.
/// - Any error returned by [`Command::spawn`], [`Child::try_wait`], or
///   the pipe reads is propagated unchanged.
///
/// # Example
///
/// ```no_run
/// use std::process::Command;
/// use std::time::Duration;
/// use p10k_rs_core::output_with_deadline;
///
/// let mut cmd = Command::new("node");
/// cmd.arg("--version");
/// match output_with_deadline(&mut cmd, Duration::from_millis(500)) {
///     Ok(out) if out.status.success() => {
///         let v = String::from_utf8_lossy(&out.stdout);
///         println!("node {}", v.trim());
///     }
///     _ => { /* segment stays hidden */ }
/// }
/// ```
pub fn output_with_deadline(cmd: &mut Command, deadline: Duration) -> io::Result<Output> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            // Child exited on its own — `wait_with_output` reads the
            // captured pipes and returns the full `Output`.
            return child.wait_with_output();
        }
        if started.elapsed() >= deadline {
            return kill_and_timeout(child);
        }
        sleep(POLL_INTERVAL);
    }
}

/// Send `SIGKILL` (or platform equivalent), reap the child, and return
/// the canonical timeout error.
///
/// Both `kill` and the subsequent `wait` may legitimately fail — the
/// child could have exited between our last `try_wait` and the kill,
/// in which case `kill` returns `InvalidInput`. Those errors are
/// swallowed: the caller asked for "timed out", and producing a
/// different `io::Error` from a benign reap-race would be misleading.
fn kill_and_timeout(mut child: Child) -> io::Result<Output> {
    let _ = child.kill();
    let _ = child.wait();
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "subprocess exceeded deadline",
    ))
}

#[cfg(test)]
#[cfg(unix)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn sleep_exceeds_deadline_returns_timed_out() {
        // `/bin/sleep` is the canonical "hangs forever from our point of
        // view" probe. 5 seconds of sleep against a 200 ms deadline must
        // surface as `ErrorKind::TimedOut`, and the child must be reaped
        // before we return (we don't assert the reap directly; the
        // `kill_and_timeout` path does both unconditionally).
        if !Path::new("/bin/sleep").exists() {
            return; // platform missing the probe; skip rather than fail.
        }
        let mut cmd = Command::new("/bin/sleep");
        cmd.arg("5");
        let err = output_with_deadline(&mut cmd, Duration::from_millis(200))
            .expect_err("sleep 5 must outlive the 200ms deadline");
        assert_eq!(
            err.kind(),
            io::ErrorKind::TimedOut,
            "wrong error kind: {err}"
        );
    }

    #[test]
    fn echo_returns_stdout_within_deadline() {
        // `/bin/echo hello` is the fast-path probe. With a 5 s deadline
        // we have effectively unbounded headroom; the assertion is that
        // the captured stdout contains the literal we asked for.
        if !Path::new("/bin/echo").exists() {
            return;
        }
        let mut cmd = Command::new("/bin/echo");
        cmd.arg("hello");
        let out = output_with_deadline(&mut cmd, Duration::from_secs(5))
            .expect("echo within 5s should succeed");
        assert!(
            out.status.success(),
            "echo exited non-zero: {:?}",
            out.status
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("hello"),
            "stdout missing 'hello': {stdout:?}"
        );
    }
}
