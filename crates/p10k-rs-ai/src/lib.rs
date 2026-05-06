//! AI host detection, OSC emission, and `--host` statusline rendering.
//!
//! Three responsibilities:
//!
//! 1. Detect the AI host the shell is running inside (Claude Code, Cursor,
//!    Aider, Warp, ...) from environment heuristics.
//! 2. Emit the OSC sequences that semantically demarcate prompt boundaries
//!    and signal cwd to host terminals (OSC 7, OSC 133).
//! 3. Render the `p10k-rs statusline --host claude-code` payload that AI
//!    hosts shell out to.
//!
//! See `ARCHITECTURE.md` § 2.6 and `10-ai-integration.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::Path;

/// Identifies the AI host the prompt is rendering inside, if any.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum HostKind {
    /// No AI host detected.
    #[default]
    None,
    /// Anthropic Claude Code CLI.
    ClaudeCode {
        /// Reported model identifier.
        model: String,
        /// Tokens consumed in the active context window.
        context_used: u64,
        /// Maximum tokens for the active context window.
        context_max: u64,
    },
    /// Cursor editor terminal.
    Cursor,
    /// `GitHub` Copilot CLI.
    GitHubCopilotCli,
    /// Aider.
    Aider {
        /// Reported model identifier.
        model: String,
    },
    /// `OpenCode`.
    OpenCode,
    /// Warp terminal.
    Warp,
    /// Google Gemini terminal integration.
    Gemini,
}

/// Detect which AI host (if any) is wrapping the shell.
///
/// Receives a snapshot rather than reading the environment directly so the
/// detection logic stays pure and testable.
///
/// # Panics
///
/// Currently unimplemented.
#[must_use]
pub fn detect_host(_env: &EnvSnapshot) -> HostKind {
    unimplemented!("detect_host lands with the AI integration phase")
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

/// Lightweight environment snapshot consumed by [`detect_host`].
///
/// The full snapshot type lives in the binary so it can capture more
/// variables than this crate cares about; this is the minimum interface.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct EnvSnapshot {}
