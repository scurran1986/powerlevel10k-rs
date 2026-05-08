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
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use p10k_rs_core::GitState;

use crate::Backend;

/// US (unit separator) — between fields within a record.
const US: u8 = 0x1F;
/// RS (record separator) — between records.
const RS: u8 = 0x1E;

/// Long-lived gitstatusd backend. Talks to a daemon spawned by the shell
/// init script via two FIFO paths.
///
/// Slice 6 has no read-timeout: a wedged daemon will hang the prompt. The
/// daemon is fast in practice (sub-ms on small repos, < 100 ms even on the
/// linux kernel) so this is acceptable as the first cut. Slice 7 adds
/// non-blocking IO + a select-with-deadline so a stuck daemon falls back
/// to `ShellOut` instead of stalling the shell.
#[derive(Debug, Clone)]
pub struct Gitstatusd {
    req_fifo: PathBuf,
    resp_fifo: PathBuf,
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
        })
    }
}

impl Backend for Gitstatusd {
    fn status(&self, path: &Path) -> Option<GitState> {
        // Open both FIFOs. The shell holds R/W fds on each (kept alive for
        // process lifetime), so neither open should block — the daemon is
        // already on the other end.
        let mut req = OpenOptions::new().write(true).open(&self.req_fifo).ok()?;

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

        // Open resp for read with the daemon's view also open. Read until
        // the first \x1E. We trust that timeout is enforced by the
        // surrounding context (the shell precmd is willing to wait); a
        // future slice can add a select-with-timeout for hard guarantees.
        let resp = OpenOptions::new().read(true).open(&self.resp_fifo).ok()?;
        let mut reader = BufReader::new(resp);
        let mut record = Vec::with_capacity(4096);
        let _ = reader.read_until(RS, &mut record).ok()?;
        // Strip the trailing RS if present.
        if record.last() == Some(&RS) {
            record.pop();
        }
        parse_response(&record)
    }
}

/// Parse a single response record (the trailing `\x1E` already stripped).
///
/// Slice 6 only populates the same fields `ShellOut` does (branch, dirty);
/// future slices map ahead/behind/conflicts/etc into `GitState` once the
/// segment registry actually consumes them.
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

    // Field offsets per 07-gitstatus.md § 1.3:
    //   4  = local branch
    //   10 = num staged
    //   11 = num unstaged
    //   12 = num conflicts
    //   13 = num untracked
    let branch = s(4).to_owned();
    let dirty = parse_u(10) > 0 || parse_u(11) > 0 || parse_u(12) > 0 || parse_u(13) > 0;
    Some(GitState { branch, dirty })
}

/// Returns `true` if `p` exists and is a named pipe (FIFO).
fn is_fifo(p: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::metadata(p)
        .map(|m| m.file_type().is_fifo())
        .unwrap_or(false)
}

/// Locate `gitstatusd` on the host. Probes (in order):
///   1. `$P10K_RS_GITSTATUSD_BIN` (explicit override).
///   2. The vendored upstream path used by the spike harness.
///   3. `gitstatusd` and `gitstatusd-linux-x86_64` on `$PATH`.
#[must_use]
pub fn locate_binary() -> Option<PathBuf> {
    if let Some(env) = std::env::var_os("P10K_RS_GITSTATUSD_BIN") {
        let p = PathBuf::from(env);
        if p.is_file() {
            return Some(p);
        }
    }
    let vendored = PathBuf::from(
        "/home/seaburdz/github/powerlevel10k/gitstatus/usrbin/gitstatusd-linux-x86_64",
    );
    if vendored.is_file() {
        return Some(vendored);
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
            "2", // staged → dirty
            "0",
            "0",
            "1", // untracked → dirty
            "0",
            "0",
            "0",
        ]);
        let g = parse_response(&bytes).unwrap();
        assert_eq!(g.branch, "feat/x");
        assert!(g.dirty);
    }

    #[test]
    fn parses_clean_repo() {
        let bytes = build_response(&[
            "id", "1", "/repo", "deadbeef", "main", "", "", "", "", "100", "0", "0", "0", "0", "0",
            "0", "0",
        ]);
        let g = parse_response(&bytes).unwrap();
        assert_eq!(g.branch, "main");
        assert!(!g.dirty);
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
}
