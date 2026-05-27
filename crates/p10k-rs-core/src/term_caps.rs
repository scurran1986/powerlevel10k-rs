//! Terminal capability probes + cache (T1.23 / T1.24).
//!
//! Today this module owns two capability answers:
//!
//! - DECSET 2026 synchronized-output support (BSU/ESU wrap around the
//!   prompt render so a multi-line redraw doesn't tear). Probed with
//!   `\x1b[?2026$p` (DECRQM); reply parsed against the
//!   `\x1b[?2026;<v>$y` shape.
//! - 24-bit truecolor support. No round-trip probe — we read
//!   `$COLORTERM` first, then fall back to terminfo `RGB` via a single
//!   `tput colors` style heuristic and finally to "256-color only". The
//!   result rides next to the sync-output answer so a single cache file
//!   covers every prompt-time capability question.
//!
//! ## I/O exception (documented)
//!
//! `p10k-rs-core` is otherwise I/O-free (see this crate's `CLAUDE.md`).
//! This module is the **fourth** documented exception, alongside
//! [`crate::term_query`] (OSC 4 palette probe via `/dev/tty`),
//! `terminal_width` in [`crate::render_prompt`] (`ioctl(TIOCGWINSZ)`),
//! and [`crate::proc::output_with_deadline`] (poll-driven subprocess
//! read). Justification mirrors the others: the probe is a one-shot
//! per-session question whose answer must reach every render, and
//! piping it through `RenderCtx` would require the binary to perform
//! the I/O before the first prompt — which is exactly the latency-
//! critical path we'd rather not bloat. Caching on disk keeps the
//! cost paid once per session.
//!
//! ## Cache file
//!
//! Keyed by a session identifier (`$P10K_RS_SESSION_ID` if set, else
//! `$TERM_SESSION_ID`, else the parent PID via `$PPID`), written under
//! `${XDG_RUNTIME_DIR}/p10k-rs/term-caps-<session>` with mode `0600`.
//! Fallback when `XDG_RUNTIME_DIR` is unset: `/tmp/p10k-rs/`, same
//! permissions. Format is a single line of comma-separated
//! `key=value` pairs so the file stays human-readable for debugging
//! and tiny enough to read with one `read`.
//!
//! ## Sync-output Drop guard
//!
//! [`SyncOutputGuard`] writes the begin-synchronized-update (BSU)
//! sequence into a `&mut String` buffer on construction and the
//! end-synchronized-update (ESU) sequence on drop. The pair is exposed
//! as a guard rather than two free functions specifically so a panic
//! between BSU and ESU still appends the ESU before unwinding past
//! the guard's scope — without that, an in-render panic could leave
//! the terminal stuck in sync mode for the duration of the terminal-
//! side safety timeout (~150 ms–1 s, vendor-dependent).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::AsFd;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[cfg(unix)]
use rustix::event::{poll, PollFd, PollFlags};
#[cfg(unix)]
use rustix::termios::{tcgetattr, tcsetattr, InputModes, LocalModes, OptionalActions, Termios};

/// 50 ms budget for the DECRQM reply, matching the OSC 4 per-query
/// budget in [`crate::term_query`]. Long enough that a local terminal
/// always answers in time; short enough that a silent terminal doesn't
/// add visible latency to the first prompt.
const PROBE_TIMEOUT: Duration = Duration::from_millis(50);

/// CSI BSU (Begin Synchronized Update) — `\x1b[?2026h`.
pub const BSU: &str = "\x1b[?2026h";

/// CSI ESU (End Synchronized Update) — `\x1b[?2026l`.
pub const ESU: &str = "\x1b[?2026l";

/// Cached capability answers, populated lazily by [`capabilities`].
static CAPS_CACHE: OnceLock<TermCaps> = OnceLock::new();

/// Snapshot of terminal capability answers for the current session.
///
/// Constructed once per process via [`capabilities`]; thereafter shared
/// from the cache.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TermCaps {
    /// DECSET 2026 synchronized-output support.
    pub sync_output: bool,
    /// 24-bit truecolor support (per `$COLORTERM` / terminfo `RGB`).
    pub truecolor: bool,
}

/// Return the cached capability snapshot, computing it on first call.
///
/// Resolution order, in priority:
/// 1. On-disk cache file under `${XDG_RUNTIME_DIR}/p10k-rs/term-caps-<session>`.
/// 2. Fresh probe (DECRQM round-trip + `$COLORTERM` lookup), then write
///    the result to the cache.
///
/// Both paths populate the in-process `CAPS_CACHE` so subsequent
/// renders pay only an atomic load.
#[must_use]
pub fn capabilities() -> TermCaps {
    *CAPS_CACHE.get_or_init(|| {
        if let Some(cached) = read_cache() {
            return cached;
        }
        let fresh = probe();
        let _ = write_cache(fresh);
        fresh
    })
}

/// Force a value into the cache for tests / diagnostics.
///
/// No-op if the cache has already been populated; the first writer wins
/// per [`OnceLock`] semantics. Tests use this to lock in a known answer
/// without running the probe.
pub fn set_cached_for_test(caps: TermCaps) -> bool {
    CAPS_CACHE.set(caps).is_ok()
}

/// Drop guard that pairs the begin / end synchronized-update sequences
/// around a render scope.
///
/// The guard owns nothing observable to outside code: construction
/// pushes [`BSU`] into the caller's buffer, and the destructor pushes
/// [`ESU`] into a side buffer the caller drains via [`Self::finish`].
/// Threading the ESU through the side buffer is what keeps the render
/// code mutating `left` freely between the BSU and the matching ESU —
/// a guard that held `&mut left` would block every helper that also
/// needs `&mut left` (the borrow checker, correctly, sees the open
/// scope of the guard as still using the buffer).
///
/// Panic semantics: if the code between [`Self::wrap`] / [`Self::wrap_if`]
/// and [`Self::finish`] panics, the destructor still runs (Drop fires
/// during unwind), so the ESU is written into the side buffer even
/// though `finish` never gets called. The caller's `catch_unwind`
/// recovery path can recover that buffer if it captured it; the
/// render path itself uses `panic = "abort"` in release per the
/// workspace `[profile.release]` so the abort path takes over before
/// any guard observation matters. Either way the invariant the guard
/// upholds is: "every BSU emitted has a matching ESU recorded".
///
/// Use [`SyncOutputGuard::wrap`] to scope a guard when support has
/// already been confirmed; [`SyncOutputGuard::wrap_if`] accepts the
/// predicate inline so the render path can keep the "support yes/no"
/// branch terse.
pub struct SyncOutputGuard {
    /// `Some(ESU)` until [`Self::finish`] consumes it, then `None`. The
    /// Drop impl flushes the remaining `Some` into [`Self::trailer`]
    /// so a panicking render still records the matching ESU.
    pending: Option<&'static str>,
    /// Where the ESU ends up on Drop or [`Self::finish`]. The render
    /// path concatenates this into the main buffer after the guard
    /// scope ends — the indirection keeps the borrow checker happy
    /// while still upholding the BSU/ESU pairing.
    trailer: String,
}

impl SyncOutputGuard {
    /// Construct an active guard, writing [`BSU`] into `buf` immediately.
    /// The caller then renders freely into `buf`; once done, calls
    /// [`Self::finish`] to retrieve the matching [`ESU`] for append.
    #[must_use]
    pub fn wrap(buf: &mut String) -> Self {
        buf.push_str(BSU);
        Self {
            pending: Some(ESU),
            trailer: String::new(),
        }
    }

    /// Active variant of [`Self::wrap`] — returns `Some(guard)` when
    /// `active` is `true`, otherwise emits nothing and returns `None`.
    ///
    /// The caller pattern is:
    ///
    /// ```ignore
    /// let sync = SyncOutputGuard::wrap_if(&mut left, sync_active);
    /// // ... render into `left` ...
    /// if let Some(guard) = sync {
    ///     left.push_str(&guard.finish());
    /// }
    /// ```
    pub fn wrap_if(buf: &mut String, active: bool) -> Option<Self> {
        if active {
            Some(Self::wrap(buf))
        } else {
            None
        }
    }

    /// Consume the guard, returning the trailing [`ESU`] string for
    /// append. On normal exit the trailer is exactly [`ESU`]; on a
    /// panicking exit the trailer is whatever Drop pushed into the
    /// side buffer (also [`ESU`]). The caller pushes the returned
    /// `String` into the main buffer.
    #[must_use]
    pub fn finish(mut self) -> String {
        if let Some(esu) = self.pending.take() {
            self.trailer.push_str(esu);
        }
        std::mem::take(&mut self.trailer)
    }
}

impl Drop for SyncOutputGuard {
    fn drop(&mut self) {
        // If `finish` already ran, `pending` is `None` and this is a
        // no-op. If we're unwinding past a panic, `pending` is still
        // `Some(ESU)` and we record the ESU in the side buffer so the
        // pairing is upheld at the type level even when the caller
        // never reaches `finish`.
        if let Some(esu) = self.pending.take() {
            self.trailer.push_str(esu);
        }
    }
}

/// Run the fresh probe path: DECRQM for sync output + env/terminfo for
/// truecolor. Pure function over the process environment; no on-disk
/// reads beyond what the OS performs to open `/dev/tty`.
fn probe() -> TermCaps {
    TermCaps {
        sync_output: probe_sync_output().unwrap_or(false),
        truecolor: probe_truecolor(),
    }
}

/// DECRQM probe for DECSET 2026.
///
/// Sends `\x1b[?2026$p`, parses the `\x1b[?2026;<value>$y` reply, and
/// maps the response value to "supported = true" for `1`, `2`, or `3`
/// (set, reset-but-known, permanently set). Values `0` and `4` mean
/// the terminal doesn't recognise the mode or has it permanently
/// disabled — treat as unsupported.
///
/// Returns `None` on probe failure (no `/dev/tty`, not a TTY, timeout,
/// unparseable reply). The caller treats `None` as `Some(false)`.
///
/// Non-Unix targets return `None` unconditionally — DECRQM probing needs
/// raw-mode termios + `/dev/tty`, neither of which exist on Windows.
#[cfg(not(unix))]
fn probe_sync_output() -> Option<bool> {
    None
}

#[cfg(unix)]
fn probe_sync_output() -> Option<bool> {
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    if !rustix::termios::isatty(&tty) {
        return None;
    }

    let original = tcgetattr(&tty).ok()?;
    let mut raw = original.clone();
    raw.local_modes
        .remove(LocalModes::ICANON | LocalModes::ECHO | LocalModes::ISIG | LocalModes::IEXTEN);
    raw.input_modes
        .remove(InputModes::ICRNL | InputModes::INLCR);
    tcsetattr(&tty, OptionalActions::Now, &raw).ok()?;
    let _guard = TermiosGuard {
        fd: tty.as_fd(),
        prev: &original,
    };

    (&tty).write_all(b"\x1b[?2026$p").ok()?;

    let mut buf = Vec::with_capacity(32);
    let mut chunk = [0u8; 64];
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return None;
        }
        let remaining_ms = i32::try_from((deadline - now).as_millis()).unwrap_or(i32::MAX);
        let mut fds = [PollFd::new(&tty, PollFlags::IN)];
        match poll(&mut fds, remaining_ms) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {}
        }
        let n = (&tty).read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(answer) = parse_decrqm_reply(&buf) {
            return Some(answer);
        }
        if buf.len() > 256 {
            // Reply should be tiny (`\x1b[?2026;N$y`). Anything larger
            // is the terminal echoing unrelated bytes — give up rather
            // than spin.
            return None;
        }
    }
}

/// Parse a DECRQM reply from `buf`. Returns `Some(true)` for "supported"
/// (values `1`/`2`/`3`), `Some(false)` for "unsupported" (`0`/`4`), or
/// `None` if the reply is incomplete.
fn parse_decrqm_reply(buf: &[u8]) -> Option<bool> {
    // Locate `\x1b[?2026;`.
    let start = buf.windows(8).position(|w| w == b"\x1b[?2026;")?;
    let after = &buf[start + 8..];
    // Find the `$y` terminator that closes the reply.
    let end = after.windows(2).position(|w| w == b"$y")?;
    let val_str = std::str::from_utf8(&after[..end]).ok()?;
    let val: u8 = val_str.parse().ok()?;
    Some(matches!(val, 1..=3))
}

/// Truecolor probe via `$COLORTERM` first, then terminfo `RGB` via
/// `$TERM` heuristics. `$COLORTERM ∈ {truecolor, 24bit}` is the
/// canonical signal modern terminals set; the `*-direct` / `*-truecolor`
/// terminfo families are the historical fallback.
fn probe_truecolor() -> bool {
    if let Ok(v) = std::env::var("COLORTERM") {
        let lower = v.to_ascii_lowercase();
        if lower == "truecolor" || lower == "24bit" {
            return true;
        }
    }
    if let Ok(term) = std::env::var("TERM") {
        let lower = term.to_ascii_lowercase();
        if lower.contains("direct") || lower.contains("truecolor") {
            return true;
        }
    }
    false
}

/// Compute the session-scoped cache path under `${XDG_RUNTIME_DIR}` or
/// the `/tmp` fallback. Always returns a path; whether the parent
/// directory exists is the caller's problem (writers create it with
/// mode `0700`).
fn cache_path() -> PathBuf {
    let session = session_id();
    let parent = std::env::var_os("XDG_RUNTIME_DIR")
        .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from)
        .join("p10k-rs");
    parent.join(format!("term-caps-{session}"))
}

/// Derive a session-scoped string suffix for the cache filename.
///
/// Priority: `$P10K_RS_SESSION_ID`, then `$TERM_SESSION_ID` (iTerm2,
/// Apple Terminal), then `$PPID` (parent shell PID, stable for the
/// lifetime of the shell), then the literal `"unknown"` — which still
/// gives every cache writer a deterministic filename, just one shared
/// across PIDs that couldn't read any of the three.
fn session_id() -> String {
    for key in ["P10K_RS_SESSION_ID", "TERM_SESSION_ID", "PPID"] {
        if let Ok(v) = std::env::var(key) {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                // Conservative sanitisation: cache filename joins this
                // into a path, so strip any byte that could escape the
                // parent directory or land special characters in the
                // filesystem (path separators, NUL).
                let safe: String = trimmed
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
                    .collect();
                if !safe.is_empty() {
                    return safe;
                }
            }
        }
    }
    "unknown".to_owned()
}

/// Read and parse the cache file. Returns `None` on any failure — a
/// stale or malformed cache is functionally equivalent to "no cache".
fn read_cache() -> Option<TermCaps> {
    let path = cache_path();
    let contents = fs::read_to_string(&path).ok()?;
    parse_cache(&contents)
}

/// Parse the cache file's `key=value,key=value` body.
fn parse_cache(body: &str) -> Option<TermCaps> {
    let mut caps = TermCaps::default();
    let mut seen_sync = false;
    let mut seen_truecolor = false;
    for pair in body.trim().split(',') {
        let (key, value) = pair.split_once('=')?;
        match key.trim() {
            "sync_output" => {
                caps.sync_output = parse_bool(value.trim())?;
                seen_sync = true;
            }
            "truecolor" => {
                caps.truecolor = parse_bool(value.trim())?;
                seen_truecolor = true;
            }
            _ => {
                // Unknown key — accept and ignore so a future field
                // can land without invalidating in-flight caches.
            }
        }
    }
    if seen_sync && seen_truecolor {
        Some(caps)
    } else {
        None
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

/// Render the capability snapshot to its on-disk form.
fn render_cache(caps: TermCaps) -> String {
    format!(
        "sync_output={},truecolor={}",
        u8::from(caps.sync_output),
        u8::from(caps.truecolor),
    )
}

/// Best-effort cache write. `Ok(())` doesn't promise the next reader
/// will see the file — just that we tried.
///
/// Creates the parent directory with mode `0700` when missing; opens
/// the file `O_CREAT | O_WRONLY | O_TRUNC` with mode `0600` so a
/// shared `/tmp/p10k-rs/` can't be read by other users. On any failure
/// the cache is simply absent next session — we re-probe.
fn write_cache(caps: TermCaps) -> std::io::Result<()> {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        match fs::create_dir_all(parent) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        // Tighten the parent directory mode best-effort. Failure is
        // non-fatal — the file's own 0600 mode is the load-bearing
        // guard; the directory mode is defence in depth.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let mut file: File = opts.open(&path)?;
    file.write_all(render_cache(caps).as_bytes())?;
    file.flush()?;
    Ok(())
}

/// RAII guard that restores the saved termios on drop.
///
/// Lifted directly from [`crate::term_query::TermiosGuard`] so the
/// DECRQM probe doesn't depend on a private item in a sibling module.
/// Failure to restore is silent — we're already exiting the probe path
/// either way and a destructor panic would replace a recoverable
/// "no sync-output answer" outcome with a process abort.
#[cfg(unix)]
struct TermiosGuard<'a> {
    fd: rustix::fd::BorrowedFd<'a>,
    prev: &'a Termios,
}

#[cfg(unix)]
impl Drop for TermiosGuard<'_> {
    fn drop(&mut self) {
        let _ = tcsetattr(self.fd, OptionalActions::Now, self.prev);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    /// Serialises tests that mutate process-global env vars consumed by
    /// `cache_path()` / `session_id()` / `probe_truecolor()` —
    /// specifically `XDG_RUNTIME_DIR`, `P10K_RS_SESSION_ID`,
    /// `TERM_SESSION_ID`, `COLORTERM`, and `TERM`. `cargo test` runs
    /// tests on a thread-pool; without this guard one test can clobber
    /// `XDG_RUNTIME_DIR` or `P10K_RS_SESSION_ID` in the window between
    /// another test's `set_var` and the `write_cache` / `metadata` it
    /// depends on. Poison is ignored so a panicked test doesn't silently
    /// skip the rest.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn guard_emits_bsu_immediately_and_esu_on_finish() {
        // Construction writes BSU into the caller's buffer; `finish`
        // returns the matching ESU for append. Smoke check the
        // end-to-end pairing on the normal (no-panic) path.
        let mut buf = String::new();
        let guard = SyncOutputGuard::wrap(&mut buf);
        buf.push_str("payload");
        let trailer = guard.finish();
        buf.push_str(&trailer);
        assert_eq!(buf, format!("{BSU}payload{ESU}"));
    }

    /// Helper: wraps a [`SyncOutputGuard`] and, on its own Drop,
    /// pulls the guard's pending ESU into the supplied side channel
    /// so the test can observe what the guard recorded during a panic
    /// unwind.
    ///
    /// Kept at module scope (rather than nested inside the panic test)
    /// because clippy's `items_after_statements` flags inner items
    /// declared mid-function, and our workspace lints promote that
    /// guidance to a hard `-D warnings` denial.
    struct CaptureOnDrop<'a> {
        guard: Option<SyncOutputGuard>,
        out: &'a std::sync::Mutex<String>,
    }

    impl Drop for CaptureOnDrop<'_> {
        fn drop(&mut self) {
            // Pull the wrapped guard out and run its Drop deterministically
            // before we observe its trailer. Without the explicit `take`
            // the wrapped guard's Drop would fire after this Drop returns
            // and the observation would race the ESU write.
            if let Some(mut inner) = self.guard.take() {
                if let Some(esu) = inner.pending.take() {
                    inner.trailer.push_str(esu);
                }
                if let Ok(mut out) = self.out.lock() {
                    out.push_str(&inner.trailer);
                }
            }
        }
    }

    #[test]
    fn guard_drop_records_esu_on_panic() {
        // Load-bearing safety: a panic between BSU and `finish` must
        // still result in the ESU being recorded by the guard so a
        // recovering catch_unwind handler can flush the matched bytes
        // to the terminal. Without that, an in-render panic could
        // leave the terminal stuck in sync mode until its safety
        // timeout fires.
        //
        // The buffer `left` already has the BSU written into it by
        // `wrap`. The guard owns the ESU separately and surfaces it
        // either through `finish` (normal path) or through its Drop
        // recording it on the trailer (panic path). We capture the
        // trailer via a side channel inside the unwinding closure.
        let mut buf = String::new();
        let trailer_after_panic = std::sync::Mutex::new(String::new());
        let result = catch_unwind(AssertUnwindSafe(|| {
            let guard = SyncOutputGuard::wrap(&mut buf);
            buf.push_str("partial");
            let cap = CaptureOnDrop {
                guard: Some(guard),
                out: &trailer_after_panic,
            };
            // Bind `cap` so the panic unwinds through its Drop. The
            // black-box read keeps the binding live without tripping
            // `let _ = ...` style nits.
            std::hint::black_box(&cap);
            panic!("simulated mid-render panic");
        }));
        assert!(result.is_err(), "panic must propagate past the guard");
        assert!(
            buf.starts_with(BSU),
            "BSU must remain in the buffer even on panic: {buf:?}",
        );
        assert!(buf.contains("partial"));
        let recorded = trailer_after_panic.lock().expect("trailer mutex");
        assert_eq!(
            recorded.as_str(),
            ESU,
            "guard Drop must record the ESU on the trailer when finish is skipped",
        );
    }

    #[test]
    fn guard_wrap_if_inactive_is_a_noop() {
        // `wrap_if(_, false)` must emit nothing — neither BSU nor
        // (later) ESU. The buffer stays byte-identical to what the
        // caller passed in.
        let mut buf = String::from("intact");
        let guard = SyncOutputGuard::wrap_if(&mut buf, false);
        assert!(guard.is_none(), "inactive guard must be None");
        assert_eq!(buf, "intact");
    }

    #[test]
    fn guard_wrap_if_active_emits_pair() {
        let mut buf = String::new();
        let guard = SyncOutputGuard::wrap_if(&mut buf, true).expect("active guard");
        buf.push_str("payload");
        let trailer = guard.finish();
        buf.push_str(&trailer);
        assert_eq!(buf, format!("{BSU}payload{ESU}"));
    }

    #[test]
    fn parse_decrqm_reply_accepts_supported_values() {
        // `1` = currently set, `2` = supported but reset, `3` = permanently
        // set — all three mean "yes, this terminal honours DECSET 2026".
        assert_eq!(parse_decrqm_reply(b"\x1b[?2026;1$y"), Some(true));
        assert_eq!(parse_decrqm_reply(b"\x1b[?2026;2$y"), Some(true));
        assert_eq!(parse_decrqm_reply(b"\x1b[?2026;3$y"), Some(true));
    }

    #[test]
    fn parse_decrqm_reply_rejects_unsupported_values() {
        // `0` = mode unrecognised, `4` = permanently disabled — both
        // mean "do not emit BSU/ESU for this terminal".
        assert_eq!(parse_decrqm_reply(b"\x1b[?2026;0$y"), Some(false));
        assert_eq!(parse_decrqm_reply(b"\x1b[?2026;4$y"), Some(false));
    }

    #[test]
    fn parse_decrqm_reply_returns_none_for_incomplete_input() {
        // No `$y` terminator yet → caller must keep reading.
        assert_eq!(parse_decrqm_reply(b"\x1b[?2026;1"), None);
        // Wrong CSI prefix → not our reply.
        assert_eq!(parse_decrqm_reply(b"\x1b[?1004;1$y"), None);
    }

    #[test]
    fn render_and_parse_cache_roundtrip() {
        let caps = TermCaps {
            sync_output: true,
            truecolor: false,
        };
        let body = render_cache(caps);
        assert_eq!(body, "sync_output=1,truecolor=0");
        assert_eq!(parse_cache(&body), Some(caps));
    }

    #[test]
    fn parse_cache_accepts_true_false_words() {
        // The writer always emits 0/1, but parsing accepts the word
        // form so a hand-edited cache (or a future writer) still
        // round-trips.
        let body = "sync_output=true,truecolor=false";
        assert_eq!(
            parse_cache(body),
            Some(TermCaps {
                sync_output: true,
                truecolor: false
            })
        );
    }

    #[test]
    fn parse_cache_requires_both_keys() {
        // A partial cache (only one of the two keys present) is
        // treated as a cache miss so we re-probe rather than emit
        // half a capability snapshot.
        assert!(parse_cache("sync_output=1").is_none());
        assert!(parse_cache("truecolor=1").is_none());
    }

    #[test]
    fn write_cache_uses_0600_perms_on_unix() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Pin the load-bearing on-disk permissions for the cache file:
        // shared `/tmp/p10k-rs/` is the fallback location, and any
        // other user on the box must NOT be able to read or modify the
        // capability snapshot we emit.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Use an isolated XDG_RUNTIME_DIR so the test doesn't fight
            // a real session cache.
            let tmp = std::env::temp_dir().join(format!(
                "p10krs-caps-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&tmp).expect("create temp xdg dir");
            let prev_xdg = std::env::var_os("XDG_RUNTIME_DIR");
            let prev_session = std::env::var_os("P10K_RS_SESSION_ID");
            std::env::set_var("XDG_RUNTIME_DIR", &tmp);
            std::env::set_var("P10K_RS_SESSION_ID", "perm-check");
            let caps = TermCaps {
                sync_output: true,
                truecolor: true,
            };
            write_cache(caps).expect("write cache");
            let path = cache_path();
            let meta = std::fs::metadata(&path).expect("stat cache");
            let mode = meta.permissions().mode() & 0o777;
            // Restore env before any panic in assertions can leak it.
            match prev_xdg {
                Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
                None => std::env::remove_var("XDG_RUNTIME_DIR"),
            }
            match prev_session {
                Some(v) => std::env::set_var("P10K_RS_SESSION_ID", v),
                None => std::env::remove_var("P10K_RS_SESSION_ID"),
            }
            let _ = std::fs::remove_dir_all(&tmp);
            assert_eq!(mode, 0o600, "cache file must be 0600, got 0o{mode:o}");
        }
    }

    #[test]
    fn session_id_uses_p10k_env_first() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Priority of session keys is part of the cache contract — a
        // change here invalidates every running session's cache, so
        // pin the order in a test.
        let prev_p10k = std::env::var_os("P10K_RS_SESSION_ID");
        let prev_term = std::env::var_os("TERM_SESSION_ID");
        std::env::set_var("P10K_RS_SESSION_ID", "alpha");
        std::env::set_var("TERM_SESSION_ID", "beta");
        let id = session_id();
        match prev_p10k {
            Some(v) => std::env::set_var("P10K_RS_SESSION_ID", v),
            None => std::env::remove_var("P10K_RS_SESSION_ID"),
        }
        match prev_term {
            Some(v) => std::env::set_var("TERM_SESSION_ID", v),
            None => std::env::remove_var("TERM_SESSION_ID"),
        }
        assert_eq!(id, "alpha");
    }

    #[test]
    fn session_id_sanitises_path_separators() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A malicious / typo'd session id with `/` or `..` must not
        // escape the cache directory.
        let prev = std::env::var_os("P10K_RS_SESSION_ID");
        std::env::set_var("P10K_RS_SESSION_ID", "../etc/passwd");
        let id = session_id();
        match prev {
            Some(v) => std::env::set_var("P10K_RS_SESSION_ID", v),
            None => std::env::remove_var("P10K_RS_SESSION_ID"),
        }
        // The dots survive (allowed character) but the slashes are
        // stripped, so the worst the attacker gets is the literal
        // filename `..etcpasswd` inside our cache dir — no escape.
        assert!(!id.contains('/'), "session id leaked path separator: {id}");
    }

    #[test]
    fn probe_truecolor_honours_colorterm() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let prev = std::env::var_os("COLORTERM");
        std::env::set_var("COLORTERM", "truecolor");
        let yes = probe_truecolor();
        std::env::set_var("COLORTERM", "16color");
        let no_via_value = probe_truecolor();
        std::env::remove_var("COLORTERM");
        let no_via_unset = {
            // Also blank $TERM so the second probe arm doesn't catch
            // a host shell's `xterm-direct` and flip the result.
            let prev_term = std::env::var_os("TERM");
            std::env::set_var("TERM", "dumb");
            let v = probe_truecolor();
            match prev_term {
                Some(t) => std::env::set_var("TERM", t),
                None => std::env::remove_var("TERM"),
            }
            v
        };
        match prev {
            Some(v) => std::env::set_var("COLORTERM", v),
            None => std::env::remove_var("COLORTERM"),
        }
        assert!(yes, "COLORTERM=truecolor must opt into truecolor");
        assert!(
            !no_via_value,
            "COLORTERM=16color must not flag truecolor support"
        );
        assert!(
            !no_via_unset,
            "absent COLORTERM with dumb TERM must default to false"
        );
    }
}
