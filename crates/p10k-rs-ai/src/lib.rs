//! AI host detection, OSC emission, and `--host` statusline rendering.
//!
//! Three responsibilities:
//!
//! 1. Detect the AI host the shell is running inside (Claude Code, Cursor,
//!    Aider, ...) from environment heuristics. The result rides in
//!    [`p10k_rs_core::HostKind`] inside `RenderCtx::host` so segments can
//!    react to it.
//! 2. Emit the OSC sequences that semantically demarcate prompt boundaries
//!    and signal cwd to host terminals (OSC 7, OSC 133).
//! 3. Render the `p10k-rs statusline --host claude-code` payload that AI
//!    hosts shell out to.
//!
//! Only (1) is wired today. (2) and (3) ship in later slices.
//!
//! See `ARCHITECTURE.md` § 2.6 and `10-ai-integration.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::Path;

pub use p10k_rs_core::HostKind;

/// Detect which AI host (if any) is wrapping the current shell by probing
/// process environment variables.
///
/// This is the production entry point — it reads `std::env`. For unit
/// tests and any caller that wants a deterministic input, use
/// [`detect_from_env`] with a closure that returns the values you want
/// the detector to see.
///
/// Detection precedence matches the order callers usually care about:
///
/// 1. `$CLAUDECODE` set (any value) → [`HostKind::ClaudeCode`].
/// 2. Any `$AIDER_*` env var set → [`HostKind::Aider`].
/// 3. Any `$CURSOR_*` env var set → [`HostKind::Cursor`].
/// 4. Otherwise → [`HostKind::None`].
///
/// Hosts beyond Claude Code are best-effort stubs for this slice — the
/// goal is the wiring, not perfect detection. Future slices can refine
/// fingerprints and add structured per-host data.
#[must_use]
pub fn detect_host_kind() -> HostKind {
    detect_from_env(|name| std::env::var(name).ok())
}

/// Pure variant of [`detect_host_kind`] that probes via the supplied
/// callback instead of `std::env`. The callback receives an env-var
/// name and returns the value, or `None` if unset.
///
/// Factored out so unit tests can drive the detector with a synthetic
/// environment without mutating process-global state — relevant because
/// `std::env::set_var` is racy across the test threadpool.
///
/// The probe list is intentionally short — one canonical "this host is
/// running" fingerprint per supported host. Adding more probes per host
/// is fine, but keep the precedence stable (Claude Code → Aider →
/// Cursor → None) so a single shell wrapped by multiple integrations
/// reports the outermost one.
#[must_use]
pub fn detect_from_env<F: Fn(&str) -> Option<String>>(env: F) -> HostKind {
    // Claude Code exports `$CLAUDECODE` on every spawn — that's the
    // canonical fingerprint and the only one we treat as authoritative
    // for this slice.
    if env("CLAUDECODE").is_some() {
        return HostKind::ClaudeCode;
    }

    // Aider doesn't (today) export a single canonical variable. The
    // most stable signals are `$AIDER_API_KEY` (set when the user
    // configured a key) and `$AIDER_MODEL` (set on every invocation
    // when the model is pinned). Either one is good-enough for the
    // MVP — false positives here only mis-paint a prompt badge.
    for key in ["AIDER_API_KEY", "AIDER_MODEL", "AIDER_AUTO_COMMITS"] {
        if env(key).is_some() {
            return HostKind::Aider;
        }
    }

    // Cursor exports `$CURSOR_TRACE_ID` for its built-in terminal and
    // `$CURSOR_SESSION_ID` for some integrations. Either signals we're
    // running inside a Cursor shell.
    for key in ["CURSOR_TRACE_ID", "CURSOR_SESSION_ID"] {
        if env(key).is_some() {
            return HostKind::Cursor;
        }
    }

    HostKind::None
}

/// Render the JSON statusline payload for `host`, given the host's input
/// JSON on stdin.
///
/// Returns an empty string today — the per-host statusline format lands
/// in the AI integration phase (slice 61). The function exists as a
/// stable public entry point so the binary's `statusline --host …`
/// subcommand can route to it once richer per-host metadata is wired.
/// Returning `""` instead of panicking keeps the public surface
/// crash-safe: a caller that reaches this from a misrouted CLI invocation
/// gets no output rather than a process-killing `unimplemented!()`.
#[must_use]
pub fn render_statusline(_host: HostKind, _json_in: &[u8]) -> String {
    String::new()
}

/// Emit an OSC 7 sequence reporting `cwd` to the host terminal.
///
/// Format: `\x1b]7;file://<host>/<percent-encoded-path>\x1b\\`. The
/// hostname is left empty (`file:///path`) — Claude Code, `VSCode`, and
/// Cursor parse the path regardless of the host field, and probing for
/// a hostname would push us off the I/O-free render path.
///
/// Path encoding uses a conservative RFC-3986-style percent-encoder:
/// the unreserved set (`A-Z a-z 0-9 - _ . ~`) plus `/` (path separator)
/// pass through; every other byte is encoded as `%XX`. Spaces become
/// `%20`. Non-UTF-8 paths are encoded byte-by-byte via the lossy UTF-8
/// representation — `Path::to_string_lossy` already replaces invalid
/// sequences with U+FFFD, which then percent-encodes cleanly.
///
/// Control bytes don't appear here in practice: `RenderCtx::cwd` is
/// the process cwd, which the kernel guarantees is free of `\0`; any
/// other control byte the renderer eventually surfaces has already been
/// stripped by `sanitize_for_terminal`. The encoder still escapes them
/// defensively as `%XX`.
#[must_use]
pub fn osc7_emit(cwd: &Path) -> String {
    let raw = cwd.to_string_lossy();
    let mut encoded = String::with_capacity(raw.len() + 16);
    for b in raw.as_bytes() {
        let c = *b;
        let unreserved = c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~' | b'/');
        if unreserved {
            encoded.push(c as char);
        } else {
            use std::fmt::Write;
            // `write!` into a String only fails on allocator failure,
            // which would already have aborted before now.
            let _ = write!(encoded, "%{c:02X}");
        }
    }
    format!("\x1b]7;file://{encoded}\x1b\\")
}

/// OSC 133 prompt-start marker (`A` — semantic prompt boundary).
///
/// Building block for [`osc133_command_start`]. Hosts that parse only
/// the `A` marker (some older `VSCode` shell-integration probes) can use
/// this directly.
#[must_use]
pub fn osc133_prompt_start() -> &'static str {
    "\x1b]133;A\x07"
}

/// OSC 133 command-line-start marker (`B` — end of PS1, start of the
/// editable command line).
#[must_use]
pub fn osc133_prompt_end() -> &'static str {
    "\x1b]133;B\x07"
}

/// Concatenated OSC 133 `A` + `B` for the prompt boundary.
///
/// Emitted by the binary at the head of the rendered prompt: `A` marks
/// where PS1 begins, `B` is appended at the tail of PS1 by the renderer
/// in a separate call to [`osc133_prompt_end`]. This helper exists for
/// the common "emit both at once" case the render path doesn't use, plus
/// future statusline paths that don't have a split point.
#[must_use]
pub fn osc133_command_start() -> String {
    let mut s = String::with_capacity(16);
    s.push_str(osc133_prompt_start());
    s.push_str(osc133_prompt_end());
    s
}

/// OSC 133 command-end marker carrying the exit code (`D;<exit>`).
///
/// Emitted by the shell's `precmd` hook with `$?` — the prompt itself
/// never sees the exit code at the right point to emit this, so the
/// pure-function shape here is for the init script to embed and unit
/// tests to pin the format.
#[must_use]
pub fn osc133_command_end(exit: i32) -> String {
    format!("\x1b]133;D;{exit}\x07")
}

#[cfg(test)]
mod tests {
    use super::{
        detect_from_env, osc133_command_end, osc133_command_start, osc7_emit, render_statusline,
        HostKind,
    };
    use std::path::Path;

    #[test]
    fn render_statusline_stub_returns_empty_without_panicking() {
        // The function is a public stub for the AI statusline phase. The
        // contract today is "returns an empty string, never panics" so a
        // misrouted CLI call (`statusline --host …` before the per-host
        // payload format is wired) degrades gracefully instead of taking
        // the process down. Pin the contract.
        assert_eq!(render_statusline(HostKind::None, b""), "");
        assert_eq!(render_statusline(HostKind::ClaudeCode, b"{}"), "");
        assert_eq!(render_statusline(HostKind::Aider, b"\x00\xff"), "");
    }

    /// Build a fake env lookup from a list of key/value pairs. Anything
    /// not listed returns `None`.
    fn env_with<'a>(
        pairs: &'a [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> Option<String> + 'a {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn detect_none_when_env_is_empty() {
        let kind = detect_from_env(env_with(&[]));
        assert_eq!(kind, HostKind::None);
    }

    #[test]
    fn detect_claude_code_from_claudecode_env() {
        // `$CLAUDECODE` is the canonical fingerprint. Any non-empty
        // value flips us into the Claude-Code branch — Anthropic sets
        // it to `"1"` today but we don't pin the value.
        let kind = detect_from_env(env_with(&[("CLAUDECODE", "1")]));
        assert_eq!(kind, HostKind::ClaudeCode);

        let kind = detect_from_env(env_with(&[("CLAUDECODE", "anything")]));
        assert_eq!(kind, HostKind::ClaudeCode);
    }

    #[test]
    fn detect_aider_from_aider_api_key() {
        let kind = detect_from_env(env_with(&[("AIDER_API_KEY", "sk-…")]));
        assert_eq!(kind, HostKind::Aider);

        let kind = detect_from_env(env_with(&[("AIDER_MODEL", "gpt-4o")]));
        assert_eq!(kind, HostKind::Aider);
    }

    #[test]
    fn detect_cursor_from_cursor_trace_id() {
        let kind = detect_from_env(env_with(&[("CURSOR_TRACE_ID", "abc123")]));
        assert_eq!(kind, HostKind::Cursor);

        let kind = detect_from_env(env_with(&[("CURSOR_SESSION_ID", "abc123")]));
        assert_eq!(kind, HostKind::Cursor);
    }

    #[test]
    fn claudecode_wins_over_other_hosts() {
        // If a user happens to have Aider/Cursor vars exported AND
        // they're inside Claude Code, the outermost host wins — Claude
        // Code is checked first.
        let kind = detect_from_env(env_with(&[
            ("CLAUDECODE", "1"),
            ("AIDER_API_KEY", "sk-…"),
            ("CURSOR_TRACE_ID", "abc"),
        ]));
        assert_eq!(kind, HostKind::ClaudeCode);
    }

    #[test]
    fn aider_wins_over_cursor() {
        let kind = detect_from_env(env_with(&[
            ("AIDER_MODEL", "gpt-4o"),
            ("CURSOR_TRACE_ID", "abc"),
        ]));
        assert_eq!(kind, HostKind::Aider);
    }

    #[test]
    fn osc7_encodes_simple_path() {
        // ASCII unreserved + `/`: pass-through, wrapped in the OSC
        // envelope with an empty host (Claude Code accepts `file:///…`).
        let s = osc7_emit(Path::new("/home/seaburdz"));
        assert_eq!(s, "\x1b]7;file:///home/seaburdz\x1b\\");
    }

    #[test]
    fn osc7_percent_encodes_spaces_and_special_chars() {
        // Spaces become `%20`. Capital hex per RFC 3986.
        let s = osc7_emit(Path::new("/tmp/foo bar"));
        assert_eq!(s, "\x1b]7;file:///tmp/foo%20bar\x1b\\");
    }

    #[test]
    fn osc7_preserves_unreserved_set() {
        // The unreserved set (`A-Z a-z 0-9 - _ . ~`) must not be
        // encoded — a `~` in a path stays a `~`, not `%7E`.
        let s = osc7_emit(Path::new("/home/~user/a.b-c_d"));
        assert_eq!(s, "\x1b]7;file:///home/~user/a.b-c_d\x1b\\");
    }

    #[test]
    fn osc133_command_start_emits_a_then_b() {
        // The "prompt" boundary the binary emits at the head of PS1 is
        // the A+B pair so a host that snapshots only one of them still
        // gets a useful signal.
        assert_eq!(osc133_command_start(), "\x1b]133;A\x07\x1b]133;B\x07");
    }

    #[test]
    fn osc133_command_end_includes_exit_code() {
        assert_eq!(osc133_command_end(0), "\x1b]133;D;0\x07");
        assert_eq!(osc133_command_end(130), "\x1b]133;D;130\x07");
        // Negative codes: zsh stores `$?` as unsigned, but the function
        // is `i32` for ergonomic interop with `RenderCtx::last_status`.
        // Round-trip the literal so hosts that parse signed get what
        // they expect.
        assert_eq!(osc133_command_end(-1), "\x1b]133;D;-1\x07");
    }
}
