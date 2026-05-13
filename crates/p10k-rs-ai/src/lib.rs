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
/// # Panics
///
/// Currently unimplemented.
#[must_use]
pub fn render_statusline(_host: HostKind, _json_in: &[u8]) -> String {
    unimplemented!("render_statusline lands with the AI integration phase")
}

/// Emit an OSC 7 sequence reporting `cwd` to the host terminal.
#[must_use]
pub fn osc7_emit(_cwd: &Path) -> String {
    unimplemented!("osc7_emit lands with the AI integration phase")
}

/// OSC 133 prompt-start marker (semantic prompts).
#[must_use]
pub fn osc133_prompt_start() -> &'static str {
    "\x1b]133;A\x07"
}

/// OSC 133 prompt-end marker.
#[must_use]
pub fn osc133_prompt_end() -> &'static str {
    "\x1b]133;B\x07"
}

/// OSC 133 command-start marker.
#[must_use]
pub fn osc133_command_start() -> &'static str {
    "\x1b]133;C\x07"
}

/// OSC 133 command-end marker carrying the exit code.
#[must_use]
pub fn osc133_command_end(_exit: i32) -> String {
    unimplemented!("osc133_command_end lands with the AI integration phase")
}

#[cfg(test)]
mod tests {
    use super::{detect_from_env, HostKind};

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
}
