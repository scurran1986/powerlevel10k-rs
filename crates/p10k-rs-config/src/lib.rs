//! Declarative TOML configuration for `p10k-rs`.
//!
//! This crate owns the schema. Loading, validation, defaulting, and the
//! Powerlevel9k importer (`p10k-rs import`) all live here, but the crate is
//! intentionally pure: no I/O for parsing — [`Config::from_toml`] takes a
//! string. [`Config::load_default`] is the only function in this crate that
//! touches the filesystem or environment, and it is opt-in (the binary calls
//! it; library consumers can drive `from_toml` directly).
//!
//! See `ARCHITECTURE.md` § 2.2 and `05-config-parameters.md` in the planning
//! bundle for the upstream-key mapping.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use thiserror::Error;

// `sanitize_for_terminal` is intentionally inlined here (see the `safety`
// submodule below) rather than imported from `p10k-rs-core`. The dependency
// direction is `core → config` (core re-exports `Config`); reaching back to
// `core::safety` would create a cycle. Six lines of duplication are cheaper
// than a third "common safety" crate, and both copies stay in sync via the
// shared invariant test in `crate::safety::tests`.
use crate::safety::sanitize_for_terminal;

pub mod import;

mod safety {
    //! Local copy of [`p10k_rs_core::safety::sanitize_for_terminal`].
    //!
    //! See the comment at the import site in `lib.rs` for why this is
    //! duplicated rather than imported. The behaviour must match the
    //! upstream copy byte-for-byte; a cross-crate diff test could be
    //! added later if drift becomes a real concern.

    use std::borrow::Cow;

    fn is_unsafe(c: char) -> bool {
        if c == '\t' {
            return false;
        }
        c.is_control() || c == '\u{007F}'
    }

    /// Strip every Unicode control codepoint from `s`, except horizontal
    /// tab (`\t`); also strips DEL (`U+007F`).
    ///
    /// Mirror of [`p10k_rs_core::safety::sanitize_for_terminal`]. Keep in
    /// sync — returns `Cow::Borrowed` on the no-strip fast path so the
    /// allocation only fires when at least one byte actually needs to be
    /// removed.
    pub(super) fn sanitize_for_terminal(s: &str) -> Cow<'_, str> {
        let Some((split, _)) = s.char_indices().find(|&(_, c)| is_unsafe(c)) else {
            return Cow::Borrowed(s);
        };
        let mut out = String::with_capacity(s.len());
        out.push_str(&s[..split]);
        for c in s[split..].chars() {
            if !is_unsafe(c) {
                out.push(c);
            }
        }
        Cow::Owned(out)
    }

    #[cfg(test)]
    mod tests {
        use super::sanitize_for_terminal;

        #[test]
        fn strips_cr_keeps_tab_and_unicode() {
            // Same invariants as the canonical copy in p10k-rs-core.
            assert_eq!(&*sanitize_for_terminal("a\rb"), "ab");
            assert_eq!(&*sanitize_for_terminal("a\tb"), "a\tb");
            assert_eq!(&*sanitize_for_terminal("café"), "café");
            assert_eq!(&*sanitize_for_terminal("a\x1b]0;EVIL\x07b"), "a]0;EVILb");
        }
    }
}

/// Errors produced by [`Config::from_toml`] and [`Config::load_default`].
///
/// `Io` and `Parse` are the two failure shapes a caller cares about. Both
/// embed enough context that a `tracing::warn!("{e}")` line is enough for
/// the user to find the offending file or line.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The config file could not be read.
    #[error("config I/O error reading {path}: {source}")]
    Io {
        /// The path the loader tried to read.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The config file was read but failed to parse as TOML against the schema.
    #[error("config parse error in {path}: {source}")]
    Parse {
        /// The path the loader read from.
        path: PathBuf,
        /// The underlying TOML deserialiser error.
        #[source]
        source: toml::de::Error,
    },
    /// `from_toml` was called directly (no path context) and parsing failed.
    #[error("config parse error: {0}")]
    ParseString(#[from] toml::de::Error),
    /// `to_toml` failed to serialise the config. Should never happen for
    /// a `Config` built via the schema's typed constructors — included for
    /// completeness so consumers can surface a clear error if it does.
    #[error("config serialise error: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// The config file exists and is readable but fails a load-time
    /// safety invariant (TOCTOU: symlinked, foreign-owned, or
    /// world/group-writable). Surfaces from [`Config::load_from_path`].
    ///
    /// The binary treats this identically to `Io` / `Parse` — log once
    /// at warn-level and fall back to `factory_default_config`. The
    /// distinct variant exists so the operator can see *why* their
    /// config was ignored (a permissions error reads very differently
    /// from a parse error in the log).
    #[error("config security check failed for {path}: {reason}")]
    Insecure {
        /// The path the loader refused to read.
        path: PathBuf,
        /// Human-readable explanation (foreign-owner / world-writable / symlink / …).
        reason: String,
    },
}

/// Result alias for this crate's fallible API.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Top-level configuration object — the deserialised `~/.config/p10k-rs/config.toml`.
///
/// Field shapes mirror `ARCHITECTURE.md` § 2.2.
///
/// # Sanitisation contract
///
/// [`Config::from_toml`] strips every Unicode control codepoint
/// (except `\t`) from every prompt-bound string — separator glyphs,
/// frame/ruler glyphs, segment icons, and per-state icons. The
/// on-disk shape stays `String`; sanitisation happens at the parse
/// boundary so segments can hand the value straight into the
/// rendered prompt without re-checking. See the field rustdoc on
/// [`Layout::separators`], [`Layout::frame`], [`Layout::ruler`],
/// [`SegmentConfig::icon`], and [`StateOverrides::icon`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct Config {
    /// Schema version of this config file. Currently `1`.
    pub schema_version: u32,
    /// Glyph mode: ASCII, `NerdFont`, etc.
    pub mode: Mode,
    /// Color emission mode for this session.
    pub colors: ColorMode,
    /// Layout (left/right segment lists, frame, ruler).
    pub layout: Layout,
    /// Instant-prompt behaviour.
    pub instant_prompt: InstantPromptMode,
    /// Transient-prompt behaviour.
    pub transient_prompt: TransientPromptMode,
    /// Per-segment configuration, keyed by segment name.
    #[serde(rename = "segment")]
    pub segments: HashMap<String, SegmentConfig>,
    /// AI integration toggles (host detection, OSC, statusline).
    pub ai: AiConfig,
    /// Terminal shell-integration emission (OSC 133 A/B/C/D and friends).
    ///
    /// Default [`ShellIntegrationMode::Auto`] emits when the renderer
    /// detects a modern terminal or AI host via environment heuristics
    /// (see binary-side `resolve_shell_integration`). `Always` forces
    /// emission regardless of environment; `Off` suppresses entirely.
    /// Warp is always suppressed even under `Always` — see warp#6718.
    pub shell_integration: ShellIntegration,
}

/// Glyph mode for the prompt.
///
/// Determines which icon set the segment library reaches for. The wizard
/// detects the most appropriate default based on the running terminal.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// 7-bit ASCII icons. Always works.
    Ascii,
    /// Awesome-terminal-fonts icons.
    Awesome,
    /// Nerd Fonts v2 codepoints.
    NerdFontV2,
    /// Nerd Fonts v3 codepoints.
    #[default]
    NerdFontV3,
    /// Compatibility set sourced from upstream Powerlevel10k.
    Compatible,
}

/// Color emission mode.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ColorMode {
    /// 8-color ANSI.
    Ansi8,
    /// 256-color ANSI (indexed).
    #[default]
    Ansi256,
    /// 24-bit truecolor.
    TrueColor,
    /// Probe the terminal's 16-color palette via OSC 4 (`\x1b]4;<i>;?\x1b\\`)
    /// and emit truecolor SGRs against the queried RGB values for the 16
    /// standard named colours.
    ///
    /// Best-effort: many terminals (including most muxers, tmux without
    /// passthrough, some embedded shells) do not respond to OSC 4 queries.
    /// The probe runs once per process with an 800 ms wall-clock budget;
    /// if it fails or returns nothing the renderer transparently falls
    /// back to [`ColorMode::Ansi256`] so the prompt still paints.
    ///
    /// Serde tag: `"follow_terminal"`.
    #[serde(rename = "follow_terminal")]
    FollowTerminal,
}

/// Layout for left and right prompts plus frame / ruler decoration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Layout {
    /// Ordered list of segments rendered on the left side.
    #[serde(default)]
    pub left: Vec<SegmentRef>,
    /// Ordered list of segments rendered on the right side.
    #[serde(default)]
    pub right: Vec<SegmentRef>,
    /// Segments from [`Self::left`] that render ONLY on the top line of
    /// a multi-line prompt. Segments listed here stay on line 1; segments
    /// in `left` but NOT here drop to line 2 (alongside the trailing
    /// `prompt_char` when the frame is active).
    ///
    /// Empty (the default) keeps the slice 28 behaviour: only the
    /// trailing `prompt_char` is sent to line 2 when a frame is active.
    /// Honoured only when the layout's frame is active — a single-line
    /// prompt has no line 2 to split into.
    #[serde(default)]
    pub left_top_only: Vec<SegmentRef>,
    /// Analogue of [`Self::left_top_only`] for the right prompt.
    ///
    /// Ignored when [`Self::right`] would render on a single line. The
    /// right prompt has no native multi-line frame today, so this field
    /// is reserved for symmetry with the left side; future slices may
    /// drive RPROMPT splitting from it.
    #[serde(default)]
    pub right_top_only: Vec<SegmentRef>,
    /// Optional decorative frame style.
    ///
    /// `frame.glyph`, when present, is sanitised by [`Config::from_toml`].
    #[serde(default)]
    pub frame: Option<FrameStyle>,
    /// Optional ruler (horizontal divider above the prompt).
    ///
    /// `ruler.glyph`, when present, is sanitised by [`Config::from_toml`].
    #[serde(default)]
    pub ruler: Option<RulerStyle>,
    /// Glyphs that join segments and subsegments.
    ///
    /// All three glyph fields are sanitised by [`Config::from_toml`].
    #[serde(default)]
    pub separators: Separators,
}

/// A reference to a segment by name in [`Layout::left`] / [`Layout::right`].
///
/// Stays a newtype rather than a bare [`String`] so future syntax (e.g.
/// `"vcs?max=3"`) lands without a breaking change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct SegmentRef(pub String);

/// Per-segment configuration block.
///
/// Mirrors upstream Powerlevel10k's `POWERLEVEL9K_<SEG>_*` knobs into a
/// nested table. Per-state overrides (e.g. `dir.states.not_writable`) live
/// in [`Self::states`] keyed by the segment-defined state tag.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct SegmentConfig {
    /// When `true`, the segment is skipped entirely.
    pub disabled: bool,
    /// Optional foreground color (named or numeric).
    pub foreground: Option<Color>,
    /// Optional background color (named or numeric).
    pub background: Option<Color>,
    /// Override the default icon glyph for this segment.
    ///
    /// Sanitised by [`Config::from_toml`]: control bytes (`\r`, `\x1b`, …)
    /// are stripped before the value lands in the parsed `Config`, so the
    /// renderer can hand it straight to the segment.
    pub icon: Option<String>,
    /// Padding on either side of the segment.
    pub padding: Padding,
    /// Truncation policy applied to the rendered cwd.
    ///
    /// Only consulted by the `dir` segment — other segments accept the field
    /// (the schema is shared across every segment) but ignore it. Off by
    /// default; see [`DirTruncate`] for the strategies.
    pub truncate: DirTruncate,
    /// Render only when one of these commands is on the upcoming buffer.
    pub show_on_command: Option<Vec<String>>,
    /// Render only when the cwd matches one of these globs.
    pub show_in_dir: Option<Vec<Glob>>,
    /// Disable the segment when the cwd matches this glob.
    pub disabled_dir_pattern: Option<Glob>,
    /// Per-state overrides, keyed by segment-defined state tag
    /// (e.g. `"error"`, `"writable"`).
    pub states: HashMap<String, StateOverrides>,
}

/// Style overrides scoped to a particular segment state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct StateOverrides {
    /// Foreground for this state.
    pub foreground: Option<Color>,
    /// Background for this state.
    pub background: Option<Color>,
    /// Icon override for this state.
    ///
    /// Sanitised by [`Config::from_toml`]; same contract as
    /// [`SegmentConfig::icon`].
    pub icon: Option<String>,
}

/// A color value: either a Powerlevel9k-style name or an ANSI/truecolor index.
///
/// The string variant retains `red`, `darkred`, `wheat4`, etc. The numeric
/// variants land truecolor and 256-color values.
///
/// The named variant is `Cow<'static, str>` rather than `String` so that
/// segment defaults can supply zero-allocation static literals
/// (`Color::Named("blue".into())` becomes `Cow::Borrowed("blue")` via
/// `From<&'static str>`). User-supplied values from TOML deserialize as
/// `Cow::Owned` and behave like a `String` — same memory cost, same API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Color {
    /// Named color (Powerlevel9k compat).
    Named(Cow<'static, str>),
    /// Indexed 0–255 ANSI color.
    Indexed(u8),
    /// Truecolor `[r, g, b]`.
    Rgb([u8; 3]),
}

/// Padding around a segment.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Padding {
    /// Whitespace cells to the left.
    pub left: u8,
    /// Whitespace cells to the right.
    pub right: u8,
}

/// Cwd truncation policy for the `dir` segment.
///
/// Lives on every [`SegmentConfig`] for schema uniformity (matching the
/// pattern set by [`SegmentConfig::foreground`] / [`SegmentConfig::padding`]),
/// but only the `dir` segment reads it. Other segments silently ignore the
/// field.
///
/// See [`DirTruncateStrategy`] for the supported strategies. Default is
/// [`DirTruncateStrategy::None`] — full path, current behaviour.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct DirTruncate {
    /// Strategy: `"none"`, `"to_last"`, or `"middle"`.
    pub strategy: DirTruncateStrategy,
    /// How many trailing components to keep (default `3`).
    ///
    /// A value of `0` is treated as `1` at render time to keep the cwd from
    /// disappearing entirely.
    pub length: u8,
}

/// Strategies recognised by [`DirTruncate::strategy`].
///
/// Mirrors a subset of upstream Powerlevel10k's `POWERLEVEL9K_SHORTEN_STRATEGY`
/// values.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DirTruncateStrategy {
    /// No truncation. Render the full (home-collapsed) path.
    #[default]
    None,
    /// Keep only the trailing `length` components, prepend `…`.
    ///
    /// Example: `/a/b/c/d/e` with `length = 2` becomes `…/d/e`.
    ToLast,
    /// Keep the first component and the trailing `length - 1` components,
    /// replacing the middle with `…`.
    ///
    /// Example: `/a/b/c/d/e` with `length = 2` becomes `/a/…/d/e`. With
    /// `length = 3`: `/a/…/c/d/e`.
    Middle,
    /// For each non-final component, keep the shortest prefix that
    /// uniquely identifies it among its siblings in the parent directory;
    /// the final component is always preserved in full.
    ///
    /// Example (given a filesystem where every component's siblings start
    /// with distinct letters): `/home/me/github/p10k-rs/crates/p10k-rs-segments`
    /// becomes `/h/m/g/p10k-rs/c/p10k-rs-segments`.
    ///
    /// **Performance:** this strategy issues one `read_dir` per non-final
    /// component, so it is meaningfully more expensive than [`Self::ToLast`]
    /// / [`Self::Middle`]. On slow filesystems (NFS, FUSE, very large
    /// directories) the cost is visible in prompt latency. Opt-in only; the
    /// directory listing is also capped to 200 entries per parent to bound
    /// pathological cases.
    ///
    /// If a parent directory cannot be listed (EACCES, ENOENT, etc.), the
    /// component falls back to a single-character prefix rather than
    /// aborting the truncation.
    ToUnique,
}

/// Glob string used by `show_in_dir` and friends. Validated lazily.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct Glob(pub String);

/// Frame decoration around the prompt.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct FrameStyle {
    /// Glyph used for the frame.
    pub glyph: Option<String>,
    /// Frame foreground color.
    pub foreground: Option<Color>,
    /// Bottom-left frame glyph emitted on the line that carries the
    /// trailing line-2 segment (typically `prompt_char`). Defaults to
    /// `╰─` to match the slice-28 hardcoded look; users who want a
    /// different shape (e.g. `└─`, `└`, or nothing) override here.
    ///
    /// Sanitised by [`Config::from_toml`] like the other prompt-bound
    /// strings on this struct.
    #[serde(default)]
    pub bottom_glyph: Option<String>,
}

/// Ruler decoration above the prompt.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct RulerStyle {
    /// Glyph used to draw the ruler.
    pub glyph: Option<String>,
    /// Ruler foreground color.
    pub foreground: Option<Color>,
}

/// Glyphs that join segments and subsegments.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct Separators {
    /// Glyph between segments on the left side.
    pub left: Option<String>,
    /// Glyph between segments on the right side.
    pub right: Option<String>,
    /// Glyph between subsegments within one segment.
    pub subsegment: Option<String>,
}

/// Instant-prompt behaviour mode.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InstantPromptMode {
    /// Disable instant prompt entirely.
    Off,
    /// Show a quiet placeholder while loading.
    Quiet,
    /// Show the cached real prompt (the upstream default).
    #[default]
    Verbose,
}

/// Transient-prompt behaviour mode.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TransientPromptMode {
    /// Disable transient prompt.
    #[default]
    Off,
    /// Always collapse past prompts.
    Always,
    /// Collapse only when the next prompt is in the same directory.
    SameDir,
    /// Collapse only when the next prompt is in a unique directory.
    UniqueDir,
}

/// Terminal shell-integration emission policy.
///
/// Controls whether the renderer emits OSC 133 A/B (prompt boundaries)
/// and OSC 7 (cwd) around the prompt, and whether the zsh init hooks
/// emit OSC 133 C/D around command execution. The decision is binary
/// (emit or don't); the *what* is fixed at the protocol layer.
///
/// Routes through [`Config::shell_integration`]. Default `auto`: the
/// binary inspects `$TERM_PROGRAM`, `$WT_SESSION`, `$GHOSTTY_RESOURCES_DIR`,
/// `$KITTY_WINDOW_ID`, and the AI-host env vars `$CLAUDECODE`,
/// `$AIDER_*`, `$CURSOR_*` — if any are set, integration is on.
///
/// Warp (`TERM_PROGRAM=WarpTerminal`) is always suppressed even under
/// `Always`: Warp's block model breaks on OSC 133 A. See warp#6718.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ShellIntegrationMode {
    /// Suppress every OSC 133 / OSC 7 emission. The prompt becomes
    /// byte-identical to the pre-slice-55 historical output.
    Off,
    /// Auto-detect via environment heuristics. The default.
    #[default]
    Auto,
    /// Always emit (still gated off for Warp). Useful when the user
    /// runs under a terminal we don't have a fingerprint for.
    Always,
}

/// Top-level `[shell_integration]` block.
///
/// Holds [`Self::mode`] for now. Kept as a struct rather than a bare
/// enum so future fields (`emit_osc7`, `emit_osc133_c_d`, `vscode_extended`,
/// …) can land without a schema break.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct ShellIntegration {
    /// How aggressively to emit shell-integration sequences.
    pub mode: ShellIntegrationMode,
}

/// AI integration configuration.
///
/// Each host is opt-in (deny-by-default) per the threat model in
/// `08-security.md`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct AiConfig {
    /// Emit OSC 7 (current working directory) sequences.
    pub osc7: bool,
    /// Emit OSC 133 (semantic prompt) sequences.
    pub osc133: bool,
    /// Per-host opt-in flags. Key is the host identifier ("claude-code", ...).
    #[serde(rename = "host")]
    pub hosts: HashMap<String, HostConfig>,
}

/// Per-host AI integration toggle.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct HostConfig {
    /// `true` enables status JSON ingestion for this host.
    pub enabled: bool,
}

impl Config {
    /// Parse a TOML string into a [`Config`], then sanitise every prompt-bound
    /// string field.
    ///
    /// Pure: no I/O, no environment access. Errors carry the underlying
    /// `toml::de::Error` so the caller can surface line/column context.
    ///
    /// # Sanitisation
    ///
    /// After successful parse, the control-byte stripper runs over:
    /// - `layout.separators.{left,right,subsegment}`
    /// - `layout.frame.glyph`
    /// - `layout.frame.bottom_glyph`
    /// - `layout.ruler.glyph`
    /// - every `segment.<name>.icon`
    /// - every `segment.<name>.states.<state>.icon`
    ///
    /// Mutates the parsed `Config` in place; control bytes (`\r`, `\x1b`,
    /// `\x07`, …) are dropped before the value reaches segment render code.
    /// See `crate::Config` rustdoc for the contract.
    pub fn from_toml(s: &str) -> Result<Self> {
        let mut cfg: Self = toml::from_str(s)?;
        cfg.sanitize_in_place();
        Ok(cfg)
    }

    /// Serialise this config to a pretty-printed TOML string.
    ///
    /// Used by the importer to emit the result of `p10k-rs import` — the
    /// inverse of [`Self::from_toml`] modulo whitespace and key order.
    /// Sanitisation has already run by the time the config reaches this
    /// method, so the emitted TOML is safe to round-trip through
    /// `from_toml` without changing the rendered prompt.
    pub fn to_toml(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    /// Sanitise every prompt-bound string field in place.
    ///
    /// Idempotent — running twice produces the same result. Called by
    /// [`Self::from_toml`]; exposed `pub(crate)` style only via this method
    /// (no separate public helper) so callers can't construct a Config that
    /// bypasses sanitisation.
    fn sanitize_in_place(&mut self) {
        sanitize_opt(&mut self.layout.separators.left);
        sanitize_opt(&mut self.layout.separators.right);
        sanitize_opt(&mut self.layout.separators.subsegment);
        if let Some(frame) = self.layout.frame.as_mut() {
            sanitize_opt(&mut frame.glyph);
            sanitize_opt(&mut frame.bottom_glyph);
        }
        if let Some(ruler) = self.layout.ruler.as_mut() {
            sanitize_opt(&mut ruler.glyph);
        }
        for seg in self.segments.values_mut() {
            sanitize_opt(&mut seg.icon);
            for state in seg.states.values_mut() {
                sanitize_opt(&mut state.icon);
            }
        }
    }

    /// Discover and load a config from the standard search path.
    ///
    /// Discovery order (first existing file wins):
    ///
    /// 1. `$P10K_RS_CONFIG` if set.
    /// 2. `$XDG_CONFIG_HOME/p10k-rs/config.toml`.
    /// 3. `$HOME/.config/p10k-rs/config.toml`.
    ///
    /// Returns:
    ///
    /// - `Ok(Config)` on a successful read + parse.
    /// - `Err(ConfigError::Io)` when no candidate file exists, or the
    ///   first matched candidate can't be read. The "no file found" case
    ///   carries the last-tried path and an `ErrorKind::NotFound` source
    ///   so the binary can `tracing::warn!` once and fall back to the
    ///   factory default.
    /// - `Err(ConfigError::Parse)` when a file was read but failed
    ///   schema validation.
    ///
    /// This is the only function in this crate that touches the filesystem
    /// or environment; everything else is pure over a `&str`.
    pub fn load_default() -> Result<Self> {
        let path = discover_config_path().ok_or_else(|| ConfigError::Io {
            path: PathBuf::from("<no config path resolved>"),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no p10k-rs config file found in $P10K_RS_CONFIG, \
                 $XDG_CONFIG_HOME/p10k-rs/config.toml, or \
                 $HOME/.config/p10k-rs/config.toml",
            ),
        })?;
        Self::load_from_path(&path)
    }

    /// Read and parse a config from an explicit path.
    ///
    /// Used by [`Self::load_default`] after the search-path resolves; also
    /// useful in tests that want to point the loader at a fixture without
    /// going through env-var discovery.
    ///
    /// # Safety checks (T1.13)
    ///
    /// The open is wrapped in a TOCTOU-safe pattern that mirrors the
    /// `open_owned_mode_safely` helper lifted into
    /// `p10k_rs_core::safety` (this crate cannot depend on `core`
    /// because the workspace dep direction is `core → config`; see the
    /// inlined `safety` submodule at the top of this file for the
    /// established duplication pattern):
    ///
    /// - `O_NOFOLLOW` on the open: a symlink (or a mid-flight symlink
    ///   swap) turns into `ELOOP`, surfaces as [`ConfigError::Io`] and
    ///   the binary falls back to factory default.
    /// - `fstat` on the fd: refuse files that aren't regular files, that
    ///   are owned by a uid other than the current effective uid, or
    ///   whose permission bits exceed `0o644` (any group- or
    ///   world-writable bit). Failures surface as
    ///   [`ConfigError::Insecure`].
    ///
    /// The `0o644` cap is the more permissive of the two realistic
    /// choices (`0o600` vs `0o644`): both `chmod 600` and the default
    /// `umask 022 → 0o644` install paths are accepted; the gate is
    /// "no one outside the owner can write the file." A future slice
    /// can tighten this to `0o600` once we know whether any real
    /// dotfiles workflow needs the group/world read bits.
    ///
    /// Non-unix targets fall back to the historical `read_to_string`
    /// (no fd-level fstat is available there); the FIFO IPC the gate
    /// protects in production is also unix-only.
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let bytes = read_config_bytes(path)?;
        toml::from_str::<Self>(&bytes)
            .map(|mut cfg| {
                cfg.sanitize_in_place();
                cfg
            })
            .map_err(|source| ConfigError::Parse {
                path: path.to_owned(),
                source,
            })
    }
}

/// One-shot guard so a hostile config that fires the warn path on every
/// prompt invocation doesn't spam the user's stderr. The flag flips on
/// the first refusal and stays flipped for the lifetime of the process.
static INSECURE_CONFIG_WARNED: AtomicBool = AtomicBool::new(false);

/// TOCTOU-safe wrapper around [`std::fs::read_to_string`] for config files.
///
/// On unix: open with `O_NOFOLLOW`, `fstat` the resulting fd, refuse
/// non-regular / foreign-owner / mode `& !0o644 != 0`, then read from
/// the fd. On rejection emits a one-time stderr warning so the user
/// notices their config was ignored, and returns
/// [`ConfigError::Insecure`].
///
/// On non-unix (Windows): falls back to plain `read_to_string`. The
/// production hot path that this gate protects (`gitstatusd` FIFOs,
/// shell-init) is unix-only.
///
/// This is the inlined twin of `p10k_rs_core::safety::open_owned_mode_safely`
/// (intentional broken intra-doc link: this crate cannot depend on core).
/// The workspace dep direction is `core → config`, so the same duplication
/// carve-out the [`safety`] submodule above uses for `sanitize_for_terminal`
/// applies here. Behaviour must match the canonical helper bit-for-bit.
#[cfg(unix)]
fn read_config_bytes(path: &Path) -> Result<String> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

    let mut opts = OpenOptions::new();
    opts.read(true);
    #[allow(clippy::cast_possible_wrap)]
    let nofollow = rustix::fs::OFlags::NOFOLLOW.bits() as i32;
    opts.custom_flags(nofollow);
    let mut f = opts.open(path).map_err(|source| ConfigError::Io {
        path: path.to_owned(),
        source,
    })?;

    // `File::metadata` invokes `fstat(2)` on the fd we already hold, so
    // the type/owner/mode checks below act on the inode behind the fd —
    // race-free relative to any further path manipulation.
    let md = f.metadata().map_err(|source| ConfigError::Io {
        path: path.to_owned(),
        source,
    })?;
    if !md.file_type().is_file() {
        let reason = "not a regular file".to_string();
        warn_once_insecure(path, &reason);
        return Err(ConfigError::Insecure {
            path: path.to_owned(),
            reason,
        });
    }
    let my_uid = rustix::process::geteuid().as_raw();
    if md.uid() != my_uid {
        let reason = format!("foreign owner uid {} (we are uid {})", md.uid(), my_uid);
        warn_once_insecure(path, &reason);
        return Err(ConfigError::Insecure {
            path: path.to_owned(),
            reason,
        });
    }
    let mode = md.mode() & 0o7777;
    if mode & !0o644 != 0 {
        let reason = format!("mode {mode:#o} exceeds 0o644 (group- or world-writable)");
        warn_once_insecure(path, &reason);
        return Err(ConfigError::Insecure {
            path: path.to_owned(),
            reason,
        });
    }

    let mut s = String::new();
    f.read_to_string(&mut s).map_err(|source| ConfigError::Io {
        path: path.to_owned(),
        source,
    })?;
    Ok(s)
}

#[cfg(not(unix))]
fn read_config_bytes(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.to_owned(),
        source,
    })
}

/// One-shot stderr warning that we ignored a user's config. Keeps the
/// prompt loud-once but quiet thereafter; a wedged config would
/// otherwise spam every prompt invocation.
#[cfg(unix)]
fn warn_once_insecure(path: &Path, reason: &str) {
    if !INSECURE_CONFIG_WARNED.swap(true, Ordering::Relaxed) {
        eprintln!(
            "p10k-rs: refusing to load config at {}: {reason}. \
             Using built-in default. Fix permissions with: chmod 0644 {0} && chown $USER {0}",
            path.display()
        );
    }
}

/// Apply the control-byte stripper (`safety::sanitize_for_terminal`)
/// to an `Option<String>` in place.
///
/// Skips `None` and the empty string (sanitisation is a no-op for empty input
/// and the allocation isn't worth it).
fn sanitize_opt(field: &mut Option<String>) {
    if let Some(s) = field.as_ref() {
        if !s.is_empty() {
            *field = Some(sanitize_for_terminal(s).into_owned());
        }
    }
}

/// Walk the discovery order and return the first candidate that exists.
///
/// `$P10K_RS_CONFIG` is honoured as-is even if the file is missing — the
/// loader will surface the resulting `Io` error so the user notices a
/// typo'd override instead of silently falling through to the XDG path.
fn discover_config_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("P10K_RS_CONFIG") {
        return Some(PathBuf::from(p));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let candidate = PathBuf::from(xdg).join("p10k-rs").join("config.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = PathBuf::from(home)
            .join(".config")
            .join("p10k-rs")
            .join("config.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn from_toml_roundtrips_minimal_config() {
        // Five-line minimal config: schema_version + a one-segment layout.
        let src = r#"
schema_version = 1
mode = "ascii"
colors = "ansi8"
[layout]
left = ["dir"]
"#;
        let cfg = Config::from_toml(src).expect("parse minimal config");
        assert_eq!(cfg.schema_version, 1);
        assert_eq!(cfg.mode, Mode::Ascii);
        assert_eq!(cfg.colors, ColorMode::Ansi8);
        assert_eq!(cfg.layout.left.len(), 1);
        assert_eq!(cfg.layout.left[0].0, "dir");
    }

    #[test]
    fn from_toml_strips_control_bytes_in_separators() {
        // Embed `\r` in a separator. Sanitiser must strip it before the
        // value lands in the parsed Config.
        let src = "schema_version = 1\n\
                   [layout.separators]\n\
                   left = \"a\\rb\"\n";
        let cfg = Config::from_toml(src).expect("parse");
        assert_eq!(
            cfg.layout.separators.left.as_deref(),
            Some("ab"),
            "CR must be stripped from separator"
        );
    }

    #[test]
    fn from_toml_strips_control_bytes_in_segment_icons() {
        let src = "schema_version = 1\n\
                   [segment.dir]\n\
                   icon = \"\\u001b]0;EVIL\\u0007\"\n\
                   [segment.dir.states.error]\n\
                   icon = \"!\\rEVIL\"\n";
        let cfg = Config::from_toml(src).expect("parse");
        let dir = cfg.segments.get("dir").expect("dir segment");
        assert_eq!(dir.icon.as_deref(), Some("]0;EVIL"));
        let err_state = dir.states.get("error").expect("error state");
        assert_eq!(err_state.icon.as_deref(), Some("!EVIL"));
    }

    #[test]
    fn from_toml_accepts_follow_terminal_color_mode() {
        // Slice 53: `colors = "follow_terminal"` opts into the OSC 4
        // palette probe at render time. The schema must accept the
        // string tag and the value must round-trip through `to_toml`
        // unchanged so editors saving the file don't subtly rewrite it.
        let src = "schema_version = 1\ncolors = \"follow_terminal\"\n";
        let cfg = Config::from_toml(src).expect("parse follow_terminal");
        assert_eq!(cfg.colors, ColorMode::FollowTerminal);

        let serialised = cfg.to_toml().expect("serialise follow_terminal");
        assert!(
            serialised.contains("colors = \"follow_terminal\""),
            "expected follow_terminal in roundtrip, got: {serialised}"
        );
        let reparsed = Config::from_toml(&serialised).expect("reparse");
        assert_eq!(reparsed.colors, ColorMode::FollowTerminal);
    }

    #[test]
    fn shell_integration_defaults_to_auto() {
        // Bundle T1.5/T1.9: schema must default `mode` to `auto` so
        // existing configs keep working and brand-new installs emit the
        // shell-integration sequences without an explicit opt-in.
        let cfg = Config::from_toml("schema_version = 1\n").expect("parse minimal");
        assert_eq!(cfg.shell_integration.mode, ShellIntegrationMode::Auto);
    }

    #[test]
    fn shell_integration_parses_always_off_auto() {
        // Pin the three accepted spellings — `auto`, `always`, `off`.
        // Kebab-case via serde rename_all; any other value should fail.
        for (toml, expected) in [
            (
                "schema_version = 1\n[shell_integration]\nmode = \"always\"\n",
                ShellIntegrationMode::Always,
            ),
            (
                "schema_version = 1\n[shell_integration]\nmode = \"off\"\n",
                ShellIntegrationMode::Off,
            ),
            (
                "schema_version = 1\n[shell_integration]\nmode = \"auto\"\n",
                ShellIntegrationMode::Auto,
            ),
        ] {
            let cfg = Config::from_toml(toml).expect("parse shell_integration");
            assert_eq!(cfg.shell_integration.mode, expected);
        }
    }

    #[test]
    fn shell_integration_rejects_unknown_mode() {
        // A typo (`"on"`, `"yes"`, …) must surface loud instead of
        // silently degrading to the default.
        let src = "schema_version = 1\n[shell_integration]\nmode = \"on\"\n";
        let err = Config::from_toml(src).expect_err("must reject unknown mode");
        let msg = format!("{err}");
        assert!(
            msg.contains("unknown variant") || msg.contains("expected one of"),
            "got: {msg}"
        );
    }

    #[test]
    fn from_toml_rejects_unknown_fields() {
        // `deny_unknown_fields` is on the Config — surface a typo loud.
        let src = "schema_version = 1\nnot_a_field = true\n";
        let err = Config::from_toml(src).expect_err("must reject unknown");
        let msg = format!("{err}");
        assert!(msg.contains("unknown field"), "got: {msg}");
    }

    /// Cheap unique suffix (PID + nanos) for temp paths — avoids pulling
    /// `tempfile` in just for two tests.
    fn unique_suffix() -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{}-{}", std::process::id(), nanos)
    }

    /// Restore an env var to its pre-test value (or remove if it was unset).
    fn restore_env(key: &str, prev: Option<std::ffi::OsString>) {
        if let Some(p) = prev {
            std::env::set_var(key, p);
        } else {
            std::env::remove_var(key);
        }
    }

    /// Serialises the two tests that mutate the process-global
    /// `P10K_RS_CONFIG` env var. `cargo test` runs tests in parallel
    /// on a thread-pool, and `std::env::set_var` is process-global, so
    /// without this guard the parse-error test can win the env in the
    /// window between the missing-file test's `set_var` and `load_default`
    /// — flipping the missing-test's `Io` expectation into a `Parse` failure.
    /// Poison is ignored: a panic in one test must not silently skip the
    /// other.
    static P10K_RS_CONFIG_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn load_default_falls_back_when_missing() {
        let _guard = P10K_RS_CONFIG_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Point env at a path that doesn't exist; loader must return an
        // Io error (binary translates that to "use factory default").
        let missing = std::env::temp_dir()
            .join(format!("p10krs-load-missing-{}", unique_suffix()))
            .join("definitely-not-here.toml");
        let prev = std::env::var_os("P10K_RS_CONFIG");
        std::env::set_var("P10K_RS_CONFIG", &missing);
        let result = Config::load_default();
        restore_env("P10K_RS_CONFIG", prev);
        match result {
            Err(ConfigError::Io { path, .. }) => {
                assert_eq!(path, missing, "Io error must carry the configured path");
            }
            other => panic!("expected Io error, got {other:?}"),
        }
    }

    #[test]
    fn load_default_falls_back_on_parse_error() {
        let _guard = P10K_RS_CONFIG_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Write a garbage file, point env at it, expect Parse error.
        let path =
            std::env::temp_dir().join(format!("p10krs-load-garbage-{}.toml", unique_suffix()));
        std::fs::write(&path, "this is definitely [not valid toml = \n")
            .expect("write garbage fixture");
        let prev = std::env::var_os("P10K_RS_CONFIG");
        std::env::set_var("P10K_RS_CONFIG", &path);
        let result = Config::load_default();
        restore_env("P10K_RS_CONFIG", prev);
        let _ = std::fs::remove_file(&path);
        match result {
            Err(ConfigError::Parse { path: p, .. }) => {
                assert_eq!(p, path, "Parse error must carry the offending path");
            }
            Err(other) => panic!("expected Parse error, got {other:?}"),
            Ok(cfg) => panic!("expected error, got Ok({cfg:?})"),
        }
    }

    // ---------- T1.13: mode + ownership safety checks ----------

    #[cfg(unix)]
    fn chmod(p: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))
            .unwrap_or_else(|e| panic!("chmod {p:?} {mode:o}: {e}"));
    }

    #[test]
    #[cfg(unix)]
    fn load_from_path_accepts_owner_only_0o600() {
        // Happy path: a config we own at mode 0o600 (`chmod 600`) loads
        // cleanly. The cap is 0o644, so 0o600 is comfortably under it.
        let path = std::env::temp_dir().join(format!("p10krs-cfg-0600-{}.toml", unique_suffix()));
        std::fs::write(&path, "schema_version = 1\n").expect("write config");
        chmod(&path, 0o600);
        let result = Config::load_from_path(&path);
        let _ = std::fs::remove_file(&path);
        assert!(
            result.is_ok(),
            "0o600 owned config must load, got {result:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn load_from_path_accepts_owner_only_0o644() {
        // 0o644 is the realistic "umask 022" default for files dropped
        // into ~/.config. Must pass.
        let path = std::env::temp_dir().join(format!("p10krs-cfg-0644-{}.toml", unique_suffix()));
        std::fs::write(&path, "schema_version = 1\n").expect("write config");
        chmod(&path, 0o644);
        let result = Config::load_from_path(&path);
        let _ = std::fs::remove_file(&path);
        assert!(
            result.is_ok(),
            "0o644 owned config must load, got {result:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn load_from_path_refuses_world_writable() {
        // World-writable (0o666 or 0o646) must trip Insecure. The
        // attacker would otherwise edit the config and steer the
        // rendered prompt (which is `eval`'d by zsh through PROMPT).
        let path =
            std::env::temp_dir().join(format!("p10krs-cfg-world-write-{}.toml", unique_suffix()));
        std::fs::write(&path, "schema_version = 1\n").expect("write config");
        chmod(&path, 0o666);
        let result = Config::load_from_path(&path);
        let _ = std::fs::remove_file(&path);
        match result {
            Err(ConfigError::Insecure { reason, .. }) => {
                assert!(
                    reason.contains("mode") && reason.contains("0o644"),
                    "expected mode-exceeds-0o644 reason, got: {reason:?}"
                );
            }
            other => panic!("expected Insecure, got {other:?}"),
        }
    }

    #[test]
    #[cfg(unix)]
    fn load_from_path_refuses_group_writable() {
        // 0o664: owner-rw, group-rw, other-r. Group-write is the
        // realistic co-tenant attack vector (shared group on a server).
        let path =
            std::env::temp_dir().join(format!("p10krs-cfg-group-write-{}.toml", unique_suffix()));
        std::fs::write(&path, "schema_version = 1\n").expect("write config");
        chmod(&path, 0o664);
        let result = Config::load_from_path(&path);
        let _ = std::fs::remove_file(&path);
        assert!(
            matches!(result, Err(ConfigError::Insecure { .. })),
            "expected Insecure, got {result:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn load_from_path_refuses_symlink() {
        // A symlink to a perfectly fine file we own must be refused.
        // O_NOFOLLOW on the open turns it into ELOOP, surfaces as Io.
        let real =
            std::env::temp_dir().join(format!("p10krs-cfg-sym-real-{}.toml", unique_suffix()));
        let link =
            std::env::temp_dir().join(format!("p10krs-cfg-sym-link-{}.toml", unique_suffix()));
        std::fs::write(&real, "schema_version = 1\n").expect("write real");
        chmod(&real, 0o600);
        std::os::unix::fs::symlink(&real, &link).expect("create symlink");
        let result = Config::load_from_path(&link);
        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_file(&real);
        match result {
            Err(ConfigError::Io { .. }) => {
                // ELOOP / ENOTDIR / ENXIO depending on libc; either way
                // Io is the correct shape — we never opened the target.
            }
            other => panic!("expected Io (ELOOP), got {other:?}"),
        }
    }

    #[test]
    fn load_from_path_missing_file_surfaces_io_not_found() {
        // The missing-file silent fallback contract: `Io { source }`
        // with `ErrorKind::NotFound`, which the binary's main loop
        // translates to "use factory default". No Insecure on a
        // not-yet-created config.
        let path =
            std::env::temp_dir().join(format!("p10krs-cfg-missing-{}.toml", unique_suffix()));
        let result = Config::load_from_path(&path);
        match result {
            Err(ConfigError::Io { source, .. }) => {
                assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
            }
            other => panic!("expected Io(NotFound), got {other:?}"),
        }
    }
}
