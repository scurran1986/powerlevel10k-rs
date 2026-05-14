//! OSC 4 terminal-palette probe.
//!
//! Best-effort discovery of the running terminal's 16-colour palette so
//! [`crate::style::sgr_fg`] / [`crate::style::sgr_bg`] can emit truecolor
//! SGRs that match the user's actual theme rather than the standardised
//! ANSI 16 (`[170, 0, 0]` etc.). When `colors = "follow_terminal"` is set,
//! the style helpers consult [`palette`] — which caches the result of one
//! probe per process — and degrade to [`crate::style::ColorMode::Ansi256`]
//! if the terminal didn't answer.
//!
//! # I/O exception (documented)
//!
//! `p10k-rs-core` is otherwise I/O-free (see `crates/p10k-rs-core/CLAUDE.md`).
//! This module is the **third** documented exception, alongside
//! `crate::terminal_width` (ioctl) and `p10k-rs-proc::output_with_deadline`
//! (poll-driven subprocess read). The probe:
//!
//! - Opens `/dev/tty` read+write, returning `None` on any failure.
//! - Switches the TTY into raw mode (no echo, no canonical line buffering)
//!   so the response bytes don't echo or get held until newline.
//! - Restores the previous `termios` state via an RAII guard, even on
//!   panic / early return.
//! - Uses `rustix::event::poll` with a 50 ms per-query deadline and an
//!   800 ms wall-clock budget across all 16 queries.
//!
//! # Terminal support
//!
//! Many terminals (most shell muxers without passthrough, several
//! embedded shells in IDEs, the Linux console) **do not respond to
//! OSC 4**. Treat the probe's success as opportunistic. The fallback
//! path (`Ansi256` named-colour table) is the contract for everyone
//! else — `FollowTerminal` is a niceness, not a guarantee.
//!
//! # Wire format
//!
//! Query: `\x1b]4;<index>;?\x1b\\` — request the RGB for the n-th
//! 16-colour palette entry. Response: `\x1b]4;<index>;rgb:RRRR/GGGG/BBBB\x1b\\`
//! where each channel is a 4-hex-digit value (16-bit). We take the high
//! byte of each channel for an 8-bit RGB triple. Some terminals reply
//! with the BEL terminator (`\x07`) instead of ST (`\x1b\\`); the parser
//! accepts both.

use std::fs::OpenOptions;
use std::io::Write;
use std::os::fd::AsFd;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use rustix::event::{poll, PollFd, PollFlags};
use rustix::termios::{tcgetattr, tcsetattr, InputModes, LocalModes, OptionalActions, Termios};

use crate::style::Color;

/// 50 ms per-query budget for OSC 4 responses. Comfortable for local
/// terminals; tight enough that an unresponsive terminal (or a mux that
/// silently drops the query) doesn't spike prompt latency.
const PER_QUERY_TIMEOUT: Duration = Duration::from_millis(50);

/// 800 ms hard wall-clock budget for the full 16-query probe. If the
/// terminal is slow but answering, we'd rather give up partway than
/// stall the first prompt of the session for a full second.
const TOTAL_BUDGET: Duration = Duration::from_millis(800);

/// Cached result of [`query_terminal_palette`]. `None` means "we ran the
/// probe and the terminal didn't answer" — callers should fall back to
/// `Ansi256`. `Some([..])` is the 16-entry palette ready to index by
/// the named-colour table's 8-bit ANSI index.
static PALETTE_CACHE: OnceLock<Option<[Color; 16]>> = OnceLock::new();

/// Return the cached 16-entry palette, running the probe on first call.
///
/// Idempotent across the lifetime of the process. Thread-safe via
/// [`OnceLock`]. If the probe fails (no TTY, terminal silent, partial
/// answers, total-budget exceeded), the cache stores `None` and every
/// subsequent call returns `None` immediately — there is no retry.
#[must_use]
pub fn palette() -> Option<&'static [Color; 16]> {
    PALETTE_CACHE.get_or_init(query_terminal_palette).as_ref()
}

/// Run the OSC 4 probe against `/dev/tty` and return the 16-entry palette.
///
/// Returns `None` if `/dev/tty` cannot be opened, isn't a TTY, the
/// terminal doesn't reply to a query within the per-query timeout
/// (50 ms), any individual response fails to parse, or the total
/// elapsed time exceeds the per-call budget (800 ms).
///
/// Public so tests and the rare external caller (a diagnostics command,
/// for instance) can force a re-probe; normal renderer code goes
/// through [`palette`] for the cache.
#[must_use]
pub fn query_terminal_palette() -> Option<[Color; 16]> {
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;

    if !rustix::termios::isatty(&tty) {
        return None;
    }

    // Take a snapshot, switch to raw, drop the guard at the end of the
    // scope to restore — even on early `?` returns.
    let original = tcgetattr(&tty).ok()?;
    let mut raw = original.clone();
    // Disable canonical line buffering + echo so the response bytes
    // arrive immediately and aren't displayed to the user.
    raw.local_modes
        .remove(LocalModes::ICANON | LocalModes::ECHO | LocalModes::ISIG | LocalModes::IEXTEN);
    // Don't let CR↔NL translation mangle the ST terminator.
    raw.input_modes
        .remove(InputModes::ICRNL | InputModes::INLCR);
    tcsetattr(&tty, OptionalActions::Now, &raw).ok()?;
    let _guard = TermiosGuard {
        fd: tty.as_fd(),
        prev: &original,
    };

    let deadline = Instant::now() + TOTAL_BUDGET;
    let mut palette: [Color; 16] = std::array::from_fn(|_| Color::Rgb([0, 0, 0]));

    for index in 0u8..16 {
        if Instant::now() >= deadline {
            return None;
        }
        let rgb = probe_one(&tty, index)?;
        palette[usize::from(index)] = Color::Rgb(rgb);
    }
    Some(palette)
}

/// RAII guard that restores the saved termios on drop.
///
/// Holding a borrowed `BorrowedFd` + `&Termios` keeps the guard
/// allocation-free; the file the fd belongs to outlives the guard by
/// scope construction (the caller binds the `File` first, then the
/// guard from a borrow on it). Failure to restore is intentionally
/// silent — we're already on the way out of an error path or normal
/// exit, and panicking from a destructor would replace a recoverable
/// "no follow-terminal palette" outcome with a process abort.
struct TermiosGuard<'a> {
    fd: rustix::fd::BorrowedFd<'a>,
    prev: &'a Termios,
}

impl Drop for TermiosGuard<'_> {
    fn drop(&mut self) {
        let _ = tcsetattr(self.fd, OptionalActions::Now, self.prev);
    }
}

/// Send one OSC 4 query and parse the response.
///
/// Returns `None` on a poll timeout, read error, or unparseable
/// response. The caller treats `None` as "give up, the terminal isn't
/// going to answer the rest either" — there is no per-index retry.
fn probe_one(tty: &std::fs::File, index: u8) -> Option<[u8; 3]> {
    use std::io::Read;

    let query = format!("\x1b]4;{index};?\x1b\\");
    // Best-effort write; an EAGAIN here is fatal for the probe.
    (&*tty).write_all(query.as_bytes()).ok()?;

    let mut buf = Vec::with_capacity(32);
    let mut chunk = [0u8; 64];
    let deadline = Instant::now() + PER_QUERY_TIMEOUT;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return None;
        }
        let remaining_ms = i32::try_from((deadline - now).as_millis()).unwrap_or(i32::MAX);
        let mut fds = [PollFd::new(tty, PollFlags::IN)];
        match poll(&mut fds, remaining_ms) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {}
        }
        let n = (&*tty).read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
        if response_complete(&buf) {
            return parse_osc4_response(&buf, index);
        }
    }
}

/// True once `buf` contains a complete OSC 4 response, terminated by
/// either BEL (`\x07`) or ST (`\x1b\\`).
fn response_complete(buf: &[u8]) -> bool {
    if buf.contains(&0x07) {
        return true;
    }
    for w in buf.windows(2) {
        if w == [0x1b, b'\\'] {
            return true;
        }
    }
    false
}

/// Parse an OSC 4 response from `buf`, asserting the index matches
/// `expected_index`.
///
/// The accepted shape (whitespace added for clarity) is:
///
/// ```text
///   ESC ] 4 ; <index> ; rgb : <rrrr> / <gggg> / <bbbb>  (ST | BEL)
/// ```
///
/// `<rrrr>` is 1–4 hex digits. xterm always emits 4 (16-bit channels);
/// the wider spec allows shorter forms, so we accept 1–4 and scale the
/// high byte out of the value uniformly. Other prefixes (`rgb:` vs
/// `rgba:`, `#rrggbb`) are rare enough we don't bother — `rgb:` covers
/// every responder we've seen.
fn parse_osc4_response(buf: &[u8], expected_index: u8) -> Option<[u8; 3]> {
    // Locate the start of the OSC sequence (`\x1b]4;`).
    let osc_start = buf.windows(4).position(|w| w == b"\x1b]4;")?;
    let after_osc = &buf[osc_start + 4..];

    // Read the index up to the next ';'.
    let semi = after_osc.iter().position(|&b| b == b';')?;
    let index_str = std::str::from_utf8(&after_osc[..semi]).ok()?;
    let index: u8 = index_str.parse().ok()?;
    if index != expected_index {
        return None;
    }
    let after_index = &after_osc[semi + 1..];

    // Expect the literal `rgb:` prefix.
    let after_rgb = after_index.strip_prefix(b"rgb:")?;

    // Trim the trailing terminator. Either `\x07` or `\x1b\\` ends the
    // response; everything before is the payload.
    let payload_end = after_rgb
        .iter()
        .position(|&b| b == 0x07 || b == 0x1b)
        .unwrap_or(after_rgb.len());
    let payload = &after_rgb[..payload_end];

    let payload_str = std::str::from_utf8(payload).ok()?;
    let mut parts = payload_str.split('/');
    let r = parse_channel(parts.next()?)?;
    let g = parse_channel(parts.next()?)?;
    let b = parse_channel(parts.next()?)?;
    if parts.next().is_some() {
        // More than three channels — not the shape we expect.
        return None;
    }
    Some([r, g, b])
}

/// Parse one OSC 4 hex channel. 1–4 hex digits, scaled so the most
/// significant nibble lands in the returned u8.
///
/// - 4 digits (`abcd`) → `0xab` (high byte, xterm's canonical form).
/// - 3 digits (`abc`)  → `0xab` (left-justify to 16 bits via shift).
/// - 2 digits (`ab`)   → `0xab`.
/// - 1 digit  (`a`)    → `0xa0`.
fn parse_channel(s: &str) -> Option<u8> {
    if s.is_empty() || s.len() > 4 {
        return None;
    }
    let raw = u16::from_str_radix(s, 16).ok()?;
    let shifted = match s.len() {
        4 => raw,
        3 => raw << 4,
        2 => raw << 8,
        1 => raw << 12,
        _ => return None,
    };
    Some((shifted >> 8) as u8)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_xterm_response_st_terminated() {
        // ESC ] 4 ; 1 ; rgb:cd31/3131/3131 ESC \
        // `cd31` → high byte 0xcd; `3131` → 0x31. This is the literal
        // shape `xterm` emits for the red palette slot under the
        // default colour scheme.
        let resp = b"\x1b]4;1;rgb:cd31/3131/3131\x1b\\";
        let rgb = parse_osc4_response(resp, 1).expect("parse");
        assert_eq!(rgb, [0xCD, 0x31, 0x31]);
    }

    #[test]
    fn parses_bel_terminated_response() {
        // Some terminals (older xterm builds, a few muxers) terminate
        // OSC with BEL instead of ST. Accept both.
        let resp = b"\x1b]4;2;rgb:0000/cd31/0000\x07";
        let rgb = parse_osc4_response(resp, 2).expect("parse");
        assert_eq!(rgb, [0x00, 0xCD, 0x00]);
    }

    #[test]
    fn rejects_mismatched_index() {
        // The OSC index in the payload (`5`) doesn't match the index
        // the caller asked for (`3`). Bail rather than silently writing
        // index 5's RGB into slot 3.
        let resp = b"\x1b]4;5;rgb:0000/0000/cd31\x1b\\";
        assert!(parse_osc4_response(resp, 3).is_none());
    }

    #[test]
    fn rejects_malformed_payload() {
        // Missing `rgb:` prefix → bail; we don't pretend to understand
        // a shape we don't emit a fallback for.
        let resp = b"\x1b]4;0;cd31/3131/3131\x1b\\";
        assert!(parse_osc4_response(resp, 0).is_none());
    }

    #[test]
    fn parse_channel_handles_short_forms() {
        // xterm emits 4 hex digits; the spec allows 1–3 too. Each form
        // left-aligns within 16 bits, then we keep the high byte.
        assert_eq!(parse_channel("ff").unwrap(), 0xFF);
        assert_eq!(parse_channel("ffff").unwrap(), 0xFF);
        assert_eq!(parse_channel("0").unwrap(), 0x00);
        assert_eq!(parse_channel("a").unwrap(), 0xA0);
        assert_eq!(parse_channel("abc").unwrap(), 0xAB);
        assert!(parse_channel("").is_none());
        assert!(parse_channel("12345").is_none());
        assert!(parse_channel("zz").is_none());
    }

    #[test]
    fn response_complete_detects_both_terminators() {
        assert!(response_complete(b"\x1b]4;0;rgb:0000/0000/0000\x07"));
        assert!(response_complete(b"\x1b]4;0;rgb:0000/0000/0000\x1b\\"));
        // Partial response with neither terminator yet.
        assert!(!response_complete(b"\x1b]4;0;rgb:0000/0000/00"));
    }
}
