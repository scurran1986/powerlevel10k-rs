//! `gitstatusd` backend: long-lived daemon over FIFOs.
//!
//! Per ADR-0001, the production hot path is a single `gitstatusd` worker
//! spawned by the shell at startup, talking to `p10k-rs prompt` over the
//! documented wire protocol. This backend implements the **client** side.
//!
//! The shell init script (`p10k-rs-shell/shells/zsh/init.zsh`):
//!   1. Locates the daemon binary.
//!   2. Creates two FIFOs (`req` and `resp`) in `$XDG_RUNTIME_DIR/p10k-rs-$$`.
//!   3. Holds open one R/W fd on each in the parent shell to keep them
//!      alive across prompt invocations.
//!   4. Spawns `gitstatusd < req > resp &` in the background.
//!   5. Exports `_P10K_RS_GITSTATUSD_REQ` and `_P10K_RS_GITSTATUSD_RESP` so
//!      child `p10k-rs prompt` invocations know where to talk.
//!
//! Per request, this backend:
//!   1. Opens the req FIFO for write (non-blocking — daemon already a reader).
//!   2. Writes one `id\x1F<dir>\x1E` request.
//!   3. Opens the resp FIFO for read.
//!   4. Reads until `\x1E`, parses, maps to [`GitState`].
//!
//! Wire format reference: `.planning/powerlevel10k-rs/07-gitstatus.md` § 1.
//! The parser was lifted from `crates/spike-gitstatus/src/gitstatusd_baseline.rs`
//! and trimmed to the fields this slice's [`GitState`] carries.

#![allow(clippy::result_large_err)]

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rustix::event::{poll, PollFd, PollFlags};
use rustix::fd::AsFd;

use p10k_rs_core::safety::SafeText;
use p10k_rs_core::GitState;

use crate::Backend;

/// US (unit separator) — between fields within a record.
const US: u8 = 0x1F;
/// RS (record separator) — between records.
const RS: u8 = 0x1E;

/// Default timeout for the daemon's response. The daemon is fast (sub-ms
/// on small repos, < 100 ms even on the linux kernel post-warm-up), so 2 s
/// is a comfortable budget that still keeps a wedged daemon from stalling
/// the shell forever — `from_env_paths` returns `None` after this timeout
/// and the binary falls back to `ShellOut`.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

/// Upper bound on the response buffer. A well-formed gitstatusd record is
/// well under 1 `KiB`; capping at 1 `MiB` stops a misbehaving (or hostile)
/// peer from forcing unbounded heap growth before the delimiter byte.
/// Hitting the cap returns `None` and falls back to `ShellOut`.
const MAX_RESPONSE_LEN: usize = 1 << 20;

/// POSIX `S_IFMT` — file-type mask within a stat `mode` field.
#[cfg(unix)]
const S_IFMT: u32 = 0o170_000;

/// POSIX `S_IFIFO` — masked-mode value identifying a FIFO.
#[cfg(unix)]
const S_IFIFO: u32 = 0o010_000;

/// Long-lived gitstatusd backend. Talks to a daemon spawned by the shell
/// init script via two FIFO paths.
///
/// A `poll(2)`-based deadline ensures a wedged daemon falls back to
/// `ShellOut` instead of hanging the prompt indefinitely.
#[derive(Debug, Clone)]
pub struct Gitstatusd {
    req_fifo: PathBuf,
    resp_fifo: PathBuf,
    timeout: Duration,
}

impl Gitstatusd {
    /// Build a backend pointing at the FIFOs the shell init script created.
    ///
    /// Returns `None` if either path is missing or not a FIFO; the binary's
    /// `cmd_prompt` falls back to `ShellOut` in that case.
    #[must_use]
    pub fn from_env_paths(req: &Path, resp: &Path) -> Option<Self> {
        if !is_fifo(req) || !is_fifo(resp) {
            return None;
        }
        Some(Self {
            req_fifo: req.to_path_buf(),
            resp_fifo: resp.to_path_buf(),
            timeout: DEFAULT_TIMEOUT,
        })
    }

    /// Override the default response timeout. Mostly for tests and
    /// future-config wiring; the default is fine for normal use.
    #[must_use]
    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }
}

impl Backend for Gitstatusd {
    fn status(&self, path: &Path) -> Option<GitState> {
        // Open both FIFOs. The shell holds R/W fds on each (kept alive for
        // process lifetime), so neither open should block — the daemon is
        // already on the other end.
        //
        // `open_fifo_safely` closes the lstat→open TOCTOU window from
        // `is_fifo()` by re-verifying type and owner against the fd we
        // actually hold (with `O_NOFOLLOW` on the open itself so a
        // mid-flight symlink swap returns `ELOOP` rather than the wrong
        // pipe).
        let mut req = open_fifo_safely(&self.req_fifo, FifoMode::Write)?;

        // Build request: `id\x1F<dir>\x1E`. id is opaque per
        // 07-gitstatus.md; we use "p10k-rs-prompt".
        let dir_bytes = path.as_os_str().as_encoded_bytes();
        let mut buf = Vec::with_capacity(dir_bytes.len() + 32);
        buf.extend_from_slice(b"p10k-rs-prompt");
        buf.push(US);
        buf.extend_from_slice(dir_bytes);
        buf.push(RS);
        req.write_all(&buf).ok()?;
        // Drop write fd to flush; do NOT close the daemon's read side
        // (other writers — namely the shell's keep-alive — keep it open).
        drop(req);

        // Open resp and read until \x1E with a poll-driven deadline. If
        // the daemon doesn't respond by deadline we return None and the
        // binary falls back to ShellOut.
        let resp = open_fifo_safely(&self.resp_fifo, FifoMode::Read)?;
        let record = read_until_with_deadline(&resp, RS, self.timeout)?;
        parse_response(&record)
    }
}

/// Open mode for [`open_fifo_safely`].
#[derive(Clone, Copy)]
enum FifoMode {
    Read,
    Write,
}

/// Open `path` for read or write and verify on the resulting fd that
/// the file is still a FIFO owned by the effective uid.
///
/// The `is_fifo()` check in [`Gitstatusd::from_env_paths`] runs lstat
/// once at construction. By the time `Backend::status` is called the
/// path may have been swapped (TOCTOU). Defences applied here:
///
/// - `O_NOFOLLOW` on the open: a symlink swap turns into `ELOOP`.
/// - `fstat` on the open fd: re-verify the file type is still FIFO and
///   the owner uid is still us. The check operates on the inode behind
///   the fd we already hold, so it is race-free relative to any further
///   path manipulation.
///
/// Returns `None` if any of: open fails, fstat fails, type isn't FIFO,
/// owner uid differs. The caller falls back to `ShellOut`.
#[cfg(unix)]
fn open_fifo_safely(path: &Path, mode: FifoMode) -> Option<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut opts = OpenOptions::new();
    match mode {
        FifoMode::Read => {
            opts.read(true);
        }
        FifoMode::Write => {
            opts.write(true);
        }
    }
    #[allow(clippy::cast_possible_wrap)]
    let nofollow = rustix::fs::OFlags::NOFOLLOW.bits() as i32;
    opts.custom_flags(nofollow);
    let f = opts.open(path).ok()?;
    let stat = rustix::fs::fstat(&f).ok()?;
    // Re-verify file type via the canonical mode mask. Using bitmask
    // arithmetic (S_IFMT / S_IFIFO hoisted as module consts) rather than
    // `rustix::fs::FileType::from_raw_mode` so the check is identical
    // across rustix releases that have churned that enum.
    //
    // `st_mode` is `u32` on Linux and `u16` on macOS; `u32::from`
    // widens both losslessly and avoids the `clippy::cast_lossless`
    // warning that fired on the macOS leg of CI.
    let mode_bits: u32 = u32::from(stat.st_mode);
    if mode_bits & S_IFMT != S_IFIFO {
        return None;
    }
    if stat.st_uid != rustix::process::geteuid().as_raw() {
        return None;
    }
    Some(f)
}

#[cfg(not(unix))]
fn open_fifo_safely(_path: &Path, _mode: FifoMode) -> Option<std::fs::File> {
    // FIFO IPC is a unix concept; on non-unix targets we never construct
    // a `Gitstatusd` in the first place (`is_fifo()` returns false).
    None
}

/// Read from `f` into a buffer until `delim` appears or the deadline elapses.
///
/// Uses `poll(2)` with the remaining timeout on each loop. Returns `None`
/// on timeout, EOF before delimiter, or read error. The returned buffer
/// does **not** include the delimiter byte.
fn read_until_with_deadline(f: &impl AsFd, delim: u8, timeout: Duration) -> Option<Vec<u8>> {
    let mut record = Vec::with_capacity(4096);
    let mut buf = [0u8; 4096];
    let deadline = Instant::now() + timeout;

    loop {
        let now = Instant::now();
        if now >= deadline {
            return None;
        }
        let remaining = deadline - now;
        // poll's i32 ms argument: clamp to i32::MAX (~24 days). Way past
        // any reasonable timeout.
        let ms = i32::try_from(remaining.as_millis()).unwrap_or(i32::MAX);
        let mut fds = [PollFd::new(f, PollFlags::IN)];
        let revents = match poll(&mut fds, ms) {
            Ok(0) | Err(_) => return None, // timeout or poll error
            Ok(_) => fds[0].revents(),
        };
        if revents.contains(PollFlags::HUP) && !revents.contains(PollFlags::IN) {
            // Hangup with nothing to read.
            return None;
        }
        // POLLIN says data is available; read won't block.
        let n = rustix::io::read(f, &mut buf).ok()?;
        if n == 0 {
            // EOF before delimiter.
            return None;
        }
        // Cap before extending so a misbehaving daemon can't force
        // unbounded heap growth before a `\x1E` ever arrives.
        if record.len().saturating_add(n) > MAX_RESPONSE_LEN {
            return None;
        }
        record.extend_from_slice(&buf[..n]);
        if let Some(pos) = record.iter().position(|&b| b == delim) {
            record.truncate(pos);
            return Some(record);
        }
    }
}

/// Parse a single response record (the trailing `\x1E` already stripped).
///
/// Maps the wire-format fields gitstatusd emits onto [`GitState`]. See
/// `07-gitstatus.md` § 1.3 for the field-index table.
fn parse_response(record: &[u8]) -> Option<GitState> {
    let fields: Vec<&[u8]> = record.split(|&b| b == US).collect();
    if fields.len() < 2 {
        return None;
    }
    if fields[1] != b"1" {
        // Not in a repo. Caller treats `Some(default)` as "in repo with
        // empty fields" so we use `None` to mean "not a repo".
        return None;
    }
    if fields.len() < 17 {
        // Daemon spoke a wire-format we don't recognise. Bail.
        return None;
    }
    let s = |i: usize| -> &str { std::str::from_utf8(fields[i]).unwrap_or("") };
    let parse_u = |i: usize| -> u32 { s(i).parse().unwrap_or(0) };
    // Untrusted field — the daemon emits whatever bytes it read from the
    // repo (branch names, paths, commit OIDs). `SafeText::from_untrusted_bytes`
    // does the lossy-UTF-8 → sanitise pipeline in one step and bakes the
    // sanitised invariant into the type so the consumer can't accidentally
    // re-introduce control bytes.
    let untrusted_field = |i: usize| -> SafeText { SafeText::from_untrusted_bytes(fields[i]) };

    // Field offsets per 07-gitstatus.md § 1.3 (0-based here):
    //   3  = HEAD commit oid
    //   4  = local branch
    //   8  = repo action (merge / rebase-i / cherry-pick / revert / bisect)
    //   10 = num staged
    //   11 = num unstaged
    //   12 = num conflicts
    //   13 = num untracked
    //   14 = ahead
    //   15 = behind
    //   16 = num stashes
    //   17 = tag (greatest lexicographic tag pointing at HEAD) — optional;
    //        per 07-gitstatus.md § 1.3 the tag is wire-field 18 (1-indexed)
    //        which is our 0-indexed slot 17. Older / minimal responders
    //        cap at 17 (1-indexed) fields = length 17 (0-indexed), so we
    //        gate the read on length > 17 to mean "at least one more
    //        field beyond stash is present".
    let commit = untrusted_field(3);
    let branch = untrusted_field(4);
    let action = normalise_action(s(8));
    let staged = parse_u(10);
    let unstaged = parse_u(11);
    let conflicts = parse_u(12);
    let untracked = parse_u(13);
    let ahead = parse_u(14);
    let behind = parse_u(15);
    let stash = parse_u(16);
    let tag = if fields.len() > 17 {
        untrusted_field(17)
    } else {
        SafeText::default()
    };
    let has_conflicts = conflicts > 0;
    let dirty = staged > 0 || unstaged > 0 || conflicts > 0 || untracked > 0;
    Some(GitState {
        branch,
        dirty,
        ahead,
        behind,
        staged,
        unstaged,
        untracked,
        has_conflicts,
        commit,
        stash,
        action,
        tag,
    })
}

/// Map gitstatusd's repo-action string onto the canonical labels
/// [`GitState::action`] documents. The daemon emits `rebase-i` /
/// `rebase-m` to distinguish interactive from merge rebases; we collapse
/// both to `"rebase"` because the prompt only cares that a rebase is in
/// flight, not which flavour. Unknown values pass through as-is so we
/// keep forward compatibility if upstream adds a new action label.
///
/// Empty input maps to an empty [`SafeText`] — the "no in-progress
/// action" sentinel. `SafeText` strips control bytes either way; this
/// helper only normalises the action *names*.
fn normalise_action(raw: &str) -> SafeText {
    let canonical = match raw {
        "" => "",
        "rebase-i" | "rebase-m" | "rebase" => "rebase",
        "merge" => "merge",
        "cherry-pick" | "cherry" => "cherry-pick",
        "revert" => "revert",
        "bisect" => "bisect",
        other => other,
    };
    SafeText::from_untrusted(canonical)
}

/// Returns `true` if `p` exists, is a named pipe (FIFO), and is owned by
/// the current effective UID. Security rationale:
///
/// - `symlink_metadata` (lstat) instead of `metadata` (stat) — refuses to
///   follow a symlink. Defends against an attacker swapping our FIFO path
///   for a symlink to their own pipe, which would otherwise hijack IPC.
/// - Owner-UID check — refuses FIFOs not owned by us. Defends against a
///   co-tenant pre-planting a FIFO in a path we'd otherwise trust.
fn is_fifo(p: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::fs::MetadataExt;
    let Ok(md) = std::fs::symlink_metadata(p) else {
        return false;
    };
    if !md.file_type().is_fifo() {
        return false;
    }
    let me = rustix::process::geteuid().as_raw();
    md.uid() == me
}

/// Locate `gitstatusd` on the host. Probes (in order):
///   1. `$P10K_RS_GITSTATUSD_BIN` (explicit override).
///   2. `gitstatusd` and `gitstatusd-linux-x86_64` on `$PATH`.
///
/// The dev-machine fallback path (`/home/seaburdz/...`) was deliberately
/// removed: it never resolved on any other machine and would happily pick
/// up any binary that happened to live there on a multi-user host.
#[must_use]
pub fn locate_binary() -> Option<PathBuf> {
    if let Some(env) = std::env::var_os("P10K_RS_GITSTATUSD_BIN") {
        let p = PathBuf::from(env);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(path_env) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_env) {
            for name in ["gitstatusd", "gitstatusd-linux-x86_64"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn build_response(fields: &[&str]) -> Vec<u8> {
        let mut buf = Vec::new();
        for (i, f) in fields.iter().enumerate() {
            buf.extend_from_slice(f.as_bytes());
            if i + 1 < fields.len() {
                buf.push(US);
            }
        }
        buf
    }

    #[test]
    fn parses_branch_with_control_chars_stripped() {
        // C2: branch name carrying an OSC 0 sequence (the daemon emits
        // raw bytes; the parser is the chokepoint that prevents them
        // from reaching the prompt).
        let bytes = build_response(&[
            "p10k-rs-prompt",
            "1",
            "/repo",
            "deadbeef",
            "main\x1b]0;TARS-OWNED\x07",
            "",
            "",
            "",
            "",
            "0",
            "0",
            "0",
            "0",
            "0",
            "0",
            "0",
            "0",
        ]);
        let s = parse_response(&bytes).unwrap();
        assert!(
            !s.branch.as_str().contains('\x1b'),
            "ESC in branch: {:?}",
            s.branch
        );
        assert!(
            !s.branch.as_str().contains('\x07'),
            "BEL in branch: {:?}",
            s.branch
        );
        assert_eq!(s.branch, "main]0;TARS-OWNED");
    }

    #[test]
    fn parses_non_utf8_branch_lossily_rather_than_dropping() {
        // M2: pre-fix `from_utf8(...).unwrap_or("")` made non-UTF-8
        // branch names render as empty. `from_utf8_lossy` keeps the
        // non-malformed bytes and substitutes U+FFFD for the bad ones,
        // so the user can see something meaningful.
        let mut branch_bytes = Vec::from(b"main");
        branch_bytes.push(0xFF); // invalid UTF-8 continuation byte
        branch_bytes.push(0xFE);
        let mut record = Vec::new();
        for (i, f) in [
            b"p10k-rs-prompt".as_ref(),
            b"1".as_ref(),
            b"/repo".as_ref(),
            b"deadbeef".as_ref(),
            branch_bytes.as_ref(),
            b"".as_ref(),
            b"".as_ref(),
            b"".as_ref(),
            b"".as_ref(),
            b"0".as_ref(),
            b"0".as_ref(),
            b"0".as_ref(),
            b"0".as_ref(),
            b"0".as_ref(),
            b"0".as_ref(),
            b"0".as_ref(),
            b"0".as_ref(),
        ]
        .iter()
        .enumerate()
        {
            record.extend_from_slice(f);
            if i + 1 < 17 {
                record.push(US);
            }
        }
        let s = parse_response(&record).unwrap();
        assert!(
            s.branch.as_str().starts_with("main"),
            "branch: {:?}",
            s.branch
        );
        // Replacement char `\u{FFFD}` is itself a non-control char, so
        // it survives sanitisation.
        assert!(s.branch.as_str().contains('\u{FFFD}'));
    }

    #[test]
    fn parses_repo_with_dirt() {
        // 17 fields: id, '1', workdir, commit, branch, upstream, remote,
        //            url, action, indexsz, staged, unstaged, conflicts,
        //            untracked, ahead, behind, stash
        let bytes = build_response(&[
            "p10k-rs-prompt",
            "1",
            "/repo",
            "deadbeef",
            "feat/x",
            "",
            "",
            "",
            "",
            "100",
            "2", // staged
            "0",
            "0",
            "1", // untracked
            "3", // ahead
            "1", // behind
            "0",
        ]);
        let g = parse_response(&bytes).unwrap();
        assert_eq!(g.branch, "feat/x");
        assert_eq!(g.commit, "deadbeef");
        assert_eq!(g.staged, 2);
        assert_eq!(g.untracked, 1);
        assert_eq!(g.ahead, 3);
        assert_eq!(g.behind, 1);
        assert!(!g.has_conflicts);
        assert!(g.dirty);
        assert_eq!(g.stash, 0);
        assert_eq!(g.action, "");
    }

    #[test]
    fn parses_stash_count() {
        let bytes = build_response(&[
            "id", "1", "/repo", "deadbeef", "main", "", "", "", "", "100", "0", "0", "0", "0", "0",
            "0", "4", // stash
        ]);
        let g = parse_response(&bytes).unwrap();
        assert_eq!(g.stash, 4);
    }

    #[test]
    fn parses_tag_when_present() {
        // Wire-format field 18 (1-indexed, per 07-gitstatus.md § 1.3) is
        // `VCS_STATUS_TAG` — the greatest lexicographic tag pointing at
        // HEAD. In 0-indexed terms that's slot 17. Pad the response out
        // to 18 fields here to exercise the read; older daemons that
        // emit only 17 (1-indexed) fields still parse with an empty tag
        // (see the next test).
        let bytes = build_response(&[
            "id",     // 0  → wire 1   request id
            "1",      // 1  → wire 2   is_repo
            "/repo",  // 2  → wire 3   workdir
            "abc",    // 3  → wire 4   commit
            "main",   // 4  → wire 5   branch
            "",       // 5  → wire 6
            "",       // 6  → wire 7
            "",       // 7  → wire 8
            "",       // 8  → wire 9   action
            "100",    // 9  → wire 10  index size
            "0",      // 10 → wire 11  staged
            "0",      // 11 → wire 12  unstaged
            "0",      // 12 → wire 13  conflicted
            "0",      // 13 → wire 14  untracked
            "0",      // 14 → wire 15  ahead
            "0",      // 15 → wire 16  behind
            "0",      // 16 → wire 17  stashes
            "v1.2.3", // 17 → wire 18  TAG
        ]);
        let g = parse_response(&bytes).unwrap();
        assert_eq!(g.tag, "v1.2.3");
    }

    #[test]
    fn parses_tag_empty_when_absent_from_wire() {
        // The 17-field minimum stays. A daemon that doesn't emit field 18
        // (older builds, or a future trimmed mode) must still parse
        // cleanly with an empty tag — not bail.
        let bytes = build_response(&[
            "id", "1", "/repo", "abc", "main", "", "", "", "", "100", "0", "0", "0", "0", "0", "0",
            "0",
        ]);
        let g = parse_response(&bytes).unwrap();
        assert_eq!(g.tag, "");
    }

    #[test]
    fn parses_merge_action() {
        let bytes = build_response(&[
            "id", "1", "/repo", "abc", "main", "", "", "", "merge", // action
            "100", "0", "0", "0", "0", "0", "0", "0",
        ]);
        let g = parse_response(&bytes).unwrap();
        assert_eq!(g.action, "merge");
    }

    #[test]
    fn rebase_variants_collapse_to_rebase() {
        for raw in ["rebase-i", "rebase-m", "rebase"] {
            let bytes = build_response(&[
                "id", "1", "/repo", "abc", "main", "", "", "", raw, "100", "0", "0", "0", "0", "0",
                "0", "0",
            ]);
            let g = parse_response(&bytes).unwrap();
            assert_eq!(g.action, "rebase", "raw action: {raw:?}");
        }
    }

    #[test]
    fn parses_clean_repo() {
        let bytes = build_response(&[
            "id", "1", "/repo", "deadbeef", "main", "", "", "", "", "100", "0", "0", "0", "0", "0",
            "0", "0",
        ]);
        let g = parse_response(&bytes).unwrap();
        assert_eq!(g.branch, "main");
        assert_eq!(g.commit, "deadbeef");
        assert!(!g.dirty);
        assert_eq!(g.ahead, 0);
        assert_eq!(g.behind, 0);
    }

    #[test]
    fn parses_conflict_repo() {
        let bytes = build_response(&[
            "id", "1", "/repo", "abc", "main", "", "", "", "", "100", "0", "0", "2", "0", "0", "0",
            "0",
        ]);
        let g = parse_response(&bytes).unwrap();
        assert!(g.has_conflicts);
        assert!(g.dirty);
    }

    #[test]
    fn no_repo_returns_none() {
        let bytes = build_response(&["id", "0"]);
        assert!(parse_response(&bytes).is_none());
    }

    #[test]
    fn short_record_is_none() {
        let bytes = build_response(&["id"]);
        assert!(parse_response(&bytes).is_none());
    }

    #[test]
    fn truncated_repo_record_is_none() {
        // Says "1" (is repo) but doesn't have the full 17 fields.
        let bytes = build_response(&["id", "1", "/repo", "deadbeef", "main"]);
        assert!(parse_response(&bytes).is_none());
    }

    // ---------- security regression tests (slice E) ----------

    /// Build a unique scratch dir under `$TMPDIR` for filesystem tests.
    /// We hand-roll instead of pulling `tempfile` back in (removed in
    /// slice 9 per workspace dep policy).
    fn scratch_dir(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!(
            "p10k-rs-gitstatusd-test-{tag}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&p).unwrap();
        p
    }

    fn rm_rf(p: &Path) {
        let _ = std::fs::remove_dir_all(p);
    }

    fn mkfifo(path: &Path) {
        // Shell out: rustix 0.38 with our enabled features doesn't expose
        // `mkfifoat`/`mknodat` portably (linux only). `/usr/bin/mkfifo`
        // is in coreutils on every unix CI runner we target. Slow
        // (~1 ms per test) but only test code.
        let status = std::process::Command::new("mkfifo")
            .arg(path)
            .status()
            .unwrap();
        assert!(status.success(), "mkfifo failed: {status:?}");
    }

    #[test]
    fn open_fifo_safely_accepts_real_fifo_owned_by_us() {
        let dir = scratch_dir("fifo-accept");
        let p = dir.join("req");
        mkfifo(&p);
        // R/W open keeps both ends "occupied" so neither read-only nor
        // write-only open will block. Matches the zsh init script's
        // keep-alive trick (`exec 3<>"$req_fifo"`).
        let _holder = OpenOptions::new().read(true).write(true).open(&p).unwrap();
        assert!(open_fifo_safely(&p, FifoMode::Read).is_some());
        assert!(open_fifo_safely(&p, FifoMode::Write).is_some());
        rm_rf(&dir);
    }

    #[test]
    fn open_fifo_safely_rejects_regular_file() {
        let dir = scratch_dir("fifo-reject-regular");
        let p = dir.join("not_a_fifo");
        std::fs::write(&p, b"hello").unwrap();
        assert!(open_fifo_safely(&p, FifoMode::Read).is_none());
        rm_rf(&dir);
    }

    #[test]
    fn open_fifo_safely_rejects_symlink_to_fifo() {
        // Defence against the TOCTOU swap: even if the path is a symlink
        // to a real FIFO, O_NOFOLLOW on open should fail.
        let dir = scratch_dir("fifo-reject-symlink");
        let real = dir.join("real");
        mkfifo(&real);
        let _holder = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&real)
            .unwrap();
        let link = dir.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(open_fifo_safely(&link, FifoMode::Read).is_none());
        rm_rf(&dir);
    }

    #[test]
    fn read_until_caps_at_max_response_len() {
        // Stream past the 1 MiB cap without ever emitting the delimiter.
        // The reader must return None instead of unbounded heap growth.
        let (rfd, wfd) = rustix::pipe::pipe().unwrap();
        let writer = std::thread::spawn(move || {
            let chunk = vec![b'X'; 64 * 1024];
            // ~18 MiB worth of writes; one of them will fail with EPIPE
            // once the reader returns and drops `rfd`. That's fine — the
            // thread just exits.
            for _ in 0..(MAX_RESPONSE_LEN / chunk.len() + 16) {
                if rustix::io::write(&wfd, &chunk).is_err() {
                    return;
                }
            }
        });
        let result = read_until_with_deadline(&rfd, RS, Duration::from_secs(5));
        drop(rfd); // unblock the writer thread via EPIPE
        writer.join().unwrap();
        assert!(result.is_none(), "expected None when stream exceeds cap");
    }
}
