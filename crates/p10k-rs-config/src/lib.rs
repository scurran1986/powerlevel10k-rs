//! Declarative TOML configuration for `p10k-rs`.
//!
//! This crate owns the schema. Loading, validation, defaulting, and the
//! Powerlevel9k importer (`p10k-rs import`) all live here, but the crate is
//! intentionally pure: no I/O, no shell out, no env probes. The binary in
//! `p10k-rs` reads bytes from disk and hands them to `Config::from_toml`
//! (to be added in the foundation phase).
//!
//! See `ARCHITECTURE.md` § 2.2 and `05-config-parameters.md` in the planning
//! bundle for the upstream-key mapping.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Top-level configuration object — the deserialised `~/.config/p10k-rs/config.toml`.
///
/// Field shapes mirror `ARCHITECTURE.md` § 2.2. Validation logic and default
/// merging are deferred to the foundation phase; this struct is currently a
/// pure data carrier.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    #[serde(default, rename = "segment")]
    pub segments: HashMap<String, SegmentConfig>,
    /// AI integration toggles (host detection, OSC, statusline).
    #[serde(default)]
    pub ai: AiConfig,
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
pub enum ColorMode {
    /// 8-color ANSI.
    Ansi8,
    /// 256-color ANSI (indexed).
    #[default]
    Ansi256,
    /// 24-bit truecolor.
    TrueColor,
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
    /// When `true`, only the top line shows the left side.
    #[serde(default)]
    pub left_top_only: bool,
    /// When `true`, only the top line shows the right side.
    #[serde(default)]
    pub right_top_only: bool,
    /// Optional decorative frame style.
    #[serde(default)]
    pub frame: Option<FrameStyle>,
    /// Optional ruler (horizontal divider above the prompt).
    #[serde(default)]
    pub ruler: Option<RulerStyle>,
    /// Glyphs that join segments and subsegments.
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
    pub icon: Option<String>,
    /// Padding on either side of the segment.
    pub padding: Padding,
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
    pub icon: Option<String>,
}

/// A color value: either a Powerlevel9k-style name or an ANSI/truecolor index.
///
/// The string variant retains `red`, `darkred`, `wheat4`, etc. The numeric
/// variants land truecolor and 256-color values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Color {
    /// Named color (Powerlevel9k compat).
    Named(String),
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
