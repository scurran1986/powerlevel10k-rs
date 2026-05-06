//! Baseline impl: shell out to the prebuilt `gitstatusd` C++ daemon and
//! parse its single-shot response.
//!
//! Used as the **ground truth** in the correctness gate: if our two Rust
//! impls disagree with this output on the 8 load-bearing fields, no benchmark
//! number matters. This is the integration test's reference, full stop.
//!
//! Wire format (from `07-gitstatus.md` § 1):
//! - Request:  `id \x1F dir \x1E`     (3 fields, terminated by RS)
//! - Response: `id \x1F is_repo \x1F ...payload... \x1E`
//!
//! We send one request and read until the first `\x1E`. The daemon stays
//! alive after — that's fine for the correctness test, and for the bench we
//! pay the daemon-spawn cost on every iteration, mirroring how we'd invoke it
//! from a fresh shell. (If we kept the daemon warm we'd be measuring IPC, not
//! the algorithm; both numbers are interesting but only the cold one is
//! comparable to a single-shot Rust call.)

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::{GitStatusSummary, Result, SpikeError};

const RS: u8 = 0x1E;
const US: u8 = 0x1F;

/// Where to look for the daemon, in priority order.
fn daemon_search_paths() -> Vec<PathBuf> {
    let mut v = vec![PathBuf::from(
        "/home/seaburdz/github/powerlevel10k/gitstatus/usrbin/gitstatusd-linux-x86_64",
    )];
    if let Ok(path_env) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_env) {
            v.push(dir.join("gitstatusd"));
            v.push(dir.join("gitstatusd-linux-x86_64"));
        }
    }
    v
}

/// Locate the daemon on disk. First hit wins.
fn locate_daemon() -> Result<PathBuf> {
    let candidates = daemon_search_paths();
    for c in &candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }
    Err(SpikeError::DaemonNotFound {
        searched: candidates,
    })
}

/// Compute a [`GitStatusSummary`] by querying `gitstatusd` over its wire
/// protocol. Each call spawns a fresh daemon; we rely on its 1-second select
/// tick for a clean shutdown after we close stdin.
pub fn status(repo_path: &Path) -> Result<GitStatusSummary> {
    let daemon = locate_daemon()?;

    let mut child = Command::new(&daemon)
        // No threads: the spike measures one query end-to-end, not the
        // multi-request scaling regime.
        .args(["-t", "1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| SpikeError::Daemon(format!("spawn {daemon:?}: {e}")))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| SpikeError::Daemon("no stdin".into()))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| SpikeError::Daemon("no stdout".into()))?;

    // Build request: `id \x1F path \x1E`. The id is opaque; we use "spike".
    let mut req = Vec::with_capacity(repo_path.as_os_str().len() + 16);
    req.extend_from_slice(b"spike");
    req.push(US);
    req.extend_from_slice(repo_path.as_os_str().as_encoded_bytes());
    req.push(RS);

    stdin
        .write_all(&req)
        .map_err(|e| SpikeError::Daemon(format!("write request: {e}")))?;
    drop(stdin); // EOF tells the daemon it can shut down after this response.

    // Read until the first `\x1E`.
    let mut buf = Vec::with_capacity(4096);
    let mut byte = [0u8; 1];
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        match stdout.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                if byte[0] == RS {
                    break;
                }
                buf.push(byte[0]);
            }
            Err(e) => return Err(SpikeError::Daemon(format!("read response: {e}"))),
        }
    }

    let _ = child.wait();
    parse_response(&buf)
}

/// Parse a single response record (the trailing `\x1E` already stripped).
fn parse_response(record: &[u8]) -> Result<GitStatusSummary> {
    let fields: Vec<&[u8]> = record.split(|&b| b == US).collect();
    // Field 0: request id (echo). Field 1: '0'/'1'.
    if fields.len() < 2 {
        return Err(SpikeError::Daemon(format!(
            "short response: {} fields",
            fields.len()
        )));
    }
    if fields[1] != b"1" {
        // Not in a repo. Return an empty summary so callers can
        // disambiguate via `commit.is_empty()` if they care.
        return Ok(GitStatusSummary::default());
    }
    if fields.len() < 17 {
        return Err(SpikeError::Daemon(format!(
            "expected ≥17 fields, got {}",
            fields.len()
        )));
    }

    // Wire-field indexing per `07-gitstatus.md` § 1.3 (0-based here, 1-based there):
    //   2 -> workdir
    //   3 -> commit
    //   4 -> local branch
    //  10 -> num staged
    //  11 -> num unstaged
    //  12 -> num conflicted
    //  13 -> num untracked
    //  14 -> ahead
    //  15 -> behind
    let s = |i: usize| -> &str { std::str::from_utf8(fields[i]).unwrap_or("") };
    let parse_u = |i: usize| -> u32 { s(i).parse().unwrap_or(0) };

    Ok(GitStatusSummary {
        commit: s(3).to_string(),
        branch: s(4).to_string(),
        staged_count: parse_u(10),
        unstaged_count: parse_u(11),
        has_conflicts: parse_u(12) > 0,
        untracked_count: parse_u(13),
        ahead: parse_u(14),
        behind: parse_u(15),
    })
}

/// Returns `true` iff the daemon is locatable on this host. Bench harness
/// uses this to skip the baseline group cleanly when the binary isn't
/// installed (CI without gitstatus prebuilt, fresh dev machines).
pub fn is_available() -> bool {
    locate_daemon().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_norepo() {
        // id, '0' (no repo), trailing junk should be ignored if absent.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"spike");
        bytes.push(US);
        bytes.extend_from_slice(b"0");
        let s = parse_response(&bytes).unwrap();
        assert_eq!(s, GitStatusSummary::default());
    }

    #[test]
    fn parse_minimal_repo() {
        // 17 fields: id, '1', workdir, commit, branch, _, _, _, _, _, _,
        // staged, unstaged, conflicted, untracked, ahead, behind
        let mut bytes = Vec::new();
        let push = |b: &mut Vec<u8>, s: &str| {
            b.extend_from_slice(s.as_bytes());
            b.push(US);
        };
        push(&mut bytes, "spike");
        push(&mut bytes, "1");
        push(&mut bytes, "/repo");
        push(&mut bytes, "deadbeef");
        push(&mut bytes, "main");
        push(&mut bytes, ""); // upstream
        push(&mut bytes, ""); // remote name
        push(&mut bytes, ""); // remote url
        push(&mut bytes, ""); // action
        push(&mut bytes, "100"); // index size
        push(&mut bytes, "2"); // staged
        push(&mut bytes, "1"); // unstaged
        push(&mut bytes, "0"); // conflicted
        push(&mut bytes, "5"); // untracked
        push(&mut bytes, "3"); // ahead
        push(&mut bytes, "0"); // behind
        bytes.extend_from_slice(b"0"); // stashes (the last field, no trailing US)

        let s = parse_response(&bytes).unwrap();
        assert_eq!(s.commit, "deadbeef");
        assert_eq!(s.branch, "main");
        assert_eq!(s.staged_count, 2);
        assert_eq!(s.unstaged_count, 1);
        assert_eq!(s.untracked_count, 5);
        assert_eq!(s.ahead, 3);
        assert_eq!(s.behind, 0);
        assert!(!s.has_conflicts);
    }
}
