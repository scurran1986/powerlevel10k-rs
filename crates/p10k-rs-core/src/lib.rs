//! Core types and the render pipeline for `p10k-rs`.
//!
//! This crate is intentionally I/O-free. It owns:
//!
//! - The [`Segment`] trait every prompt segment implements.
//! - [`RenderCtx`], the bundle of per-prompt inputs handed to each segment.
//! - [`SegmentOutput`], the typed result a segment returns to the renderer.
//! - [`render_prompt`], the pure function that walks the configured layout
//!   and produces a [`Prompt`].
//!
//! Higher-level crates plug into these types: `p10k-rs-config` deserialises
//! the [`Config`] enums referenced here, `p10k-rs-segments` provides
//! [`Segment`] implementations, `p10k-rs-shell` owns the per-shell init and
//! escape conventions, and the binary in `p10k-rs` glues them together.
//!
//! See `ARCHITECTURE.md` § 2.1 for the contract this crate enforces.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::Path;
use std::time::{Duration, SystemTime};

pub mod style;

/// A single prompt segment.
///
/// Implementations must be cheap to construct and side-effect free outside of
/// [`Segment::render`]. The renderer may call [`Segment::enabled`] before
/// [`Segment::render`] and skip the segment entirely if it returns `false`.
///
/// # Example
///
/// ```ignore
/// use p10k_rs_core::{Segment, RenderCtx, SegmentOutput};
///
/// struct Hello;
///
/// impl Segment for Hello {
///     fn name(&self) -> &'static str { "hello" }
///     fn render(&self, _ctx: &RenderCtx<'_>) -> SegmentOutput {
///         SegmentOutput {
///             text: "hello".into(),
///             plain_len: 5,
///             state: None,
///             icon: None,
///         }
///     }
/// }
/// ```
pub trait Segment: Send + Sync {
    /// The segment's stable identifier, used as the TOML config key.
    ///
    /// Must be a static, lowercase, `snake_case` string. Must not change across
    /// releases without a deprecation cycle.
    fn name(&self) -> &'static str;

    /// Render the segment for the given context.
    ///
    /// Implementations must not write to stdout, stderr, or the filesystem;
    /// all output flows through the returned [`SegmentOutput`].
    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput;

    /// Cheap precondition check.
    ///
    /// If `false`, the renderer skips the segment entirely without calling
    /// [`Segment::render`]. Default returns `true`. Use this to gate
    /// auto-detected segments (e.g. `kubecontext` only when `kubectl` is on
    /// `PATH`) without paying the full render cost.
    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        true
    }

    /// Whether this segment computes fast enough to run synchronously.
    ///
    /// `false` means the segment will be dispatched to a worker thread when
    /// the post-MVP daemon ships. Default `true`. Conservative segments
    /// (anything that may block on a network or disk syscall) should return
    /// `false`.
    fn is_fast(&self) -> bool {
        true
    }
}

/// Per-prompt context handed to every [`Segment`] during render.
///
/// Borrowed throughout — segments must not retain any reference past the
/// scope of their `render` call.
#[non_exhaustive]
pub struct RenderCtx<'a> {
    /// The parsed configuration for this prompt invocation.
    pub config: &'a Config,
    /// Which shell asked for the prompt.
    pub shell: Shell,
    /// Detected AI host environment, if any. See `p10k-rs-ai`.
    pub host: HostKind,
    /// Current working directory.
    pub cwd: &'a Path,
    /// Pre-computed git state, or `None` if outside a repository.
    pub git: Option<&'a GitState>,
    /// Exit code of the last command. `0` means success.
    pub last_status: i32,
    /// Wall-clock duration of the last command.
    pub last_duration: Duration,
    /// Number of background jobs in the calling shell.
    pub jobs: u32,
    /// Wall-clock time captured at the start of this prompt render.
    pub now: SystemTime,
    /// Snapshot of environment variables relevant to segments.
    pub env: &'a EnvSnapshot,
}

/// Result of [`Segment::render`].
///
/// `text` is already styled with ANSI escapes — segments must not perform a
/// second pass of styling at the renderer level. `plain_len` is the visual
/// width in columns, used by the renderer for ruler / frame math; it is the
/// segment's responsibility to count grapheme clusters correctly.
#[derive(Debug, Clone)]
pub struct SegmentOutput {
    /// Rendered text with ANSI styling already applied.
    pub text: String,
    /// Visual width in columns (terminal cells), excluding ANSI escapes.
    pub plain_len: u16,
    /// Optional state tag the config can target with per-state overrides
    /// (e.g. `"ok"`, `"error"`, `"writable"`).
    pub state: Option<&'static str>,
    /// Current icon, exposed for features like `show_on_command` that need
    /// to round-trip the original glyph.
    pub icon: Option<&'static str>,
}

/// Output of [`render_prompt`].
///
/// The `transient` field carries the alternate, collapsed prompt used by the
/// transient-prompt feature; it is `None` when transient mode is disabled.
#[derive(Debug, Clone)]
pub struct Prompt {
    /// Left-side prompt content with ANSI styling.
    pub left: String,
    /// Right-side prompt content with ANSI styling.
    pub right: String,
    /// Pre-rendered transient prompt, if configured.
    pub transient: Option<String>,
}

/// Render the configured prompt for the given context.
///
/// This is a pure function: given identical `cfg` and `ctx`, it returns
/// identical output. Implementations of [`Segment`] are responsible for the
/// I/O that builds `ctx` ahead of time.
///
/// # Panics
///
/// Currently unimplemented; calls panic with `unimplemented!()`. Wired in by
/// the segment-buildout phase. See `ROADMAP.md`.
#[must_use]
pub fn render_prompt(_cfg: &Config, _ctx: &RenderCtx<'_>) -> Prompt {
    unimplemented!("render_prompt is wired in during the segment buildout phase")
}

// -- Shared placeholder types ------------------------------------------------
//
// These are the minimum surface other crates compile against. They will grow
// real fields as crates land. Each one is documented; the field-level docs
// will arrive with the implementation.

/// Top-level configuration.
///
/// The full shape lives in `p10k-rs-config`; this re-export point exists so
/// `p10k-rs-core` can reference `Config` in trait signatures without a
/// dependency cycle.
///
// TODO(adviser): once `p10k-rs-config` lands, replace this with a re-export
// (`pub use p10k_rs_config::Config;`) or invert the dependency. The current
// duplication keeps `-core` build-clean before `-config` exists.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct Config {}

/// Which shell is asking for a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shell {
    /// Z shell.
    Zsh,
    /// Friendly Interactive Shell.
    Fish,
    /// Bourne Again Shell.
    Bash,
}

/// Detected AI host environment.
///
/// The expanded enum (model strings, context sizing, etc.) is owned by
/// `p10k-rs-ai`; this is the I/O-free placeholder for use inside
/// [`RenderCtx`].
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub enum HostKind {
    /// No AI host detected.
    #[default]
    None,
    /// Some host detected; see `p10k-rs-ai` for the fully-typed variant.
    Some,
}

/// Pre-computed git state for the current cwd.
///
/// Owned by `p10k-rs-git`; placeholder here until the spike crate decides
/// the final shape.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct GitState {}

/// Snapshot of environment variables relevant to segments.
///
/// Built by the binary at the start of each prompt and held in `RenderCtx`.
/// Segments read through this rather than calling [`std::env::var`] so unit
/// tests can substitute fixtures.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct EnvSnapshot {}
