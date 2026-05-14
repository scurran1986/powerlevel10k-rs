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

pub mod proc;
pub mod safety;
pub mod style;
pub mod term_query;

pub use proc::output_with_deadline;

use safety::SafeText;

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
///             ..SegmentOutput::default()
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
///
/// Fields are `pub` and the struct is **not** `#[non_exhaustive]` by design:
/// the binary, every segment, and every test fixture builds one of these,
/// so locking the constructor would create churn for every field addition.
/// Adding a field is a `SemVer` minor for downstream segment crates; we accept
/// that contract.
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
    /// Pre-computed Jujutsu (`jj`) state, or `None` if outside a jj repo
    /// or `jj` isn't on `$PATH`. Produced by `p10k-rs-jj::detect_jj` so
    /// segments can render jj info without re-shelling out per prompt.
    pub jj: Option<&'a JjState>,
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
    /// The command line about to be (or just) run, used by the
    /// `show_on_command` segment gate.
    ///
    /// Empty string means "no command" — segments with a
    /// `show_on_command` filter set are then hidden. The binary's zsh
    /// init hooks populate this with the *last* command (a cheap
    /// approximation of the upstream "upcoming command" notion; see
    /// the init script comment for the trade-off).
    pub upcoming_command: &'a str,
}

/// Result of [`Segment::render`].
///
/// `text` is already styled with ANSI escapes — segments must not perform a
/// second pass of styling at the renderer level. `plain_len` is the visual
/// width in columns, used by the renderer for ruler / frame math; it is the
/// segment's responsibility to count grapheme clusters correctly.
#[derive(Debug, Clone, Default)]
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
    /// Background colour this segment painted itself on, exposed so the
    /// renderer can emit powerline transition arrows (`\u{e0b0}`) between
    /// adjacent bg-bearing segments and a closing arrow into the terminal
    /// default after the last one. `None` opts the segment out — it
    /// renders inline with a plain separator and no surrounding arrows.
    /// Must match whatever bg the segment emitted inside `text`; the
    /// renderer trusts this field for arrow colouring without re-parsing
    /// the segment output.
    pub background: Option<style::Color>,
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

/// Build an OSC 7 sequence reporting `cwd` to the host terminal.
///
/// Inlined copy of `p10k_rs_ai::osc7_emit` — the dependency direction
/// is `ai → core`, so this crate can't reach into the AI crate for
/// the helper. Both implementations share the same encoder shape; the
/// `p10k-rs-ai` version is the canonical public entry point and what
/// statusline / external callers reach for.
///
/// Encoding: RFC-3986-style percent-encoder over the path's lossy UTF-8
/// representation. The unreserved set (`A-Z a-z 0-9 - _ . ~`) and `/`
/// pass through; every other byte becomes `%XX`. Empty hostname
/// (`file:///path`) — Claude Code, `VSCode`, and `Cursor` accept that
/// form, and probing a hostname would push us off the I/O-free render
/// path.
fn osc7_for_cwd(cwd: &Path) -> String {
    let raw = cwd.to_string_lossy();
    let mut encoded = String::with_capacity(raw.len() + 16);
    for b in raw.as_bytes() {
        let c = *b;
        let unreserved = c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'~' | b'/');
        if unreserved {
            encoded.push(c as char);
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{c:02X}");
        }
    }
    format!("\x1b]7;file://{encoded}\x1b\\")
}

/// OSC 133 prompt-start marker (`A`).
const OSC133_A: &str = "\x1b]133;A\x07";

/// OSC 133 command-line-start marker (`B`).
const OSC133_B: &str = "\x1b]133;B\x07";

/// Powerline right-pointing solid arrow (Nerd Font / Powerline glyph).
/// Drawn between segments with differing background colours so the
/// transition reads as a single continuous ribbon.
const POWERLINE_ARROW: char = '\u{e0b0}';

/// Powerline left-pointing solid arrow (mirror of [`POWERLINE_ARROW`]).
/// Used by the right-prompt renderer so the ribbon's slant points
/// "into" the prompt (towards the cursor) rather than away from it.
const POWERLINE_ARROW_LEFT: char = '\u{e0b2}';

/// Render the configured prompt for the given context.
///
/// Walks `segments` in order, calls `enabled` then `render`, and weaves
/// the outputs into a Powerlevel10k-style ribbon:
///
/// - Adjacent segments with backgrounds get a powerline arrow
///   (`\u{e0b0}`) between them, coloured `fg=prev_bg`, `bg=next_bg`, so the
///   handoff reads as a single ribbon.
/// - A closing arrow into terminal default follows the last
///   bg-bearing segment on line 1.
/// - When the layout has a `frame.glyph` configured AND `prompt_char`
///   is the trailing segment, `prompt_char` jumps to a second line
///   prefixed by a `╰─` corner (matching the top-left `╭─` the user
///   set as `frame.glyph`). Otherwise everything stays on one line.
/// - Segments with `background = None` keep the legacy behaviour:
///   joined with the configured left separator (`" "` by default).
///
/// The caller is responsible for assembling the segment list for the
/// side it wants to render (`layout.left` for the left ribbon,
/// `layout.right` for `Prompt::right`). The renderer is symmetric in
/// the segments themselves — it's the *arrow* direction and colouring
/// that differs between sides.
///
/// Output is post-processed for the target shell (zsh wants ANSI
/// escapes wrapped in `%{…%}` so PROMPT-width tracking stays right).
///
/// Pure: given identical `segments` and `ctx`, returns identical output.
#[must_use]
pub fn render_prompt(
    segments: &[Box<dyn Segment>],
    right_segments: &[Box<dyn Segment>],
    ctx: &RenderCtx<'_>,
) -> Prompt {
    let separator = ctx.config.layout.separators.left.as_deref().unwrap_or(" ");
    let mode = ctx.config.colors;
    let mut left = String::new();

    // Slice 55 — shell-integration sequences for AI / IDE hosts.
    //
    // OSC 7 reports the cwd so the host can update its embedded shell
    // navigator. OSC 133 `A` marks the semantic prompt boundary; the
    // matching `B` is appended at the very end of `left` below.
    //
    // Gate on `HostKind::None`: vanilla terminals don't parse these
    // sequences and would render nothing visible either way, but
    // keeping the prompt byte-identical to the historical output when
    // no host is detected eliminates a class of "did the prompt
    // change?" support questions. Hosts that benefit (Claude Code,
    // Cursor, VSCode agent terminals) opt in via their env var.
    if ctx.host != HostKind::None {
        left.push_str(&osc7_for_cwd(ctx.cwd));
        left.push_str(OSC133_A);
    }

    let (frame_fg, frame_active) = append_ruler_and_frame_top(&mut left, ctx, mode);

    // Resolve enabled segments up front so we can split off the
    // line-2 trailing segment without iterating twice. `mut` so the
    // line-1/line-2 partition can consume the Vec via `into_iter`
    // or `split_off` rather than cloning every `SegmentOutput`.
    let mut enabled: Vec<(SegmentOutput, &str)> = segments
        .iter()
        .filter(|s| s.enabled(ctx))
        .map(|s| (s.render(ctx), s.name()))
        .collect();

    // Partition into line 1 and line 2. Two triggers, in order:
    //   1. `layout.left_top_only` (slice 57) — explicit user list of
    //      segments that must stay on line 1. Honoured only when the
    //      frame is active (line 2 needs the bottom corner to anchor it).
    //   2. Otherwise fall back to slice 28: when the frame is active AND
    //      the trailing segment is `prompt_char`, send only prompt_char
    //      to line 2 behind a `╰─` corner.
    // Conservative: layouts without a frame keep the historical
    // single-line shape regardless of either configuration.
    let top_only = &ctx.config.layout.left_top_only;
    let (line1_owned, line2_owned): (Vec<_>, Vec<_>) = if frame_active && !top_only.is_empty() {
        let top_names: std::collections::HashSet<&str> =
            top_only.iter().map(|r| r.0.as_str()).collect();
        enabled
            .into_iter()
            .partition(|(_, name)| top_names.contains(*name))
    } else {
        let split_at = if frame_active
            && enabled
                .last()
                .is_some_and(|(_, name)| *name == "prompt_char")
        {
            enabled.len() - 1
        } else {
            enabled.len()
        };
        let tail = enabled.split_off(split_at);
        (enabled, tail)
    };

    append_ribbon(&mut left, &line1_owned, ctx, mode, separator);

    append_line2(
        &mut left,
        &line2_owned,
        ctx,
        mode,
        separator,
        frame_active,
        &frame_fg,
    );

    // Closing OSC 133 `B`: end of PS1, start of the editable command
    // line. Paired with the `A` emitted at the top of `left` above.
    if ctx.host != HostKind::None {
        left.push_str(OSC133_B);
    }

    let left = wrap_for_shell(&left, ctx.shell);
    let right = render_right(right_segments, ctx);
    let right = wrap_for_shell(&right, ctx.shell);
    let transient = Some(render_transient(segments, ctx));
    Prompt {
        left,
        right,
        transient,
    }
}

/// Emit the optional ruler line + top-left frame corner glyph and
/// return `(frame_fg_sgr, frame_active)` so the caller can reuse them
/// for line 2's bottom corner. Extracted from `render_prompt` to keep
/// that function under clippy's `too_many_lines` threshold.
fn append_ruler_and_frame_top(
    left: &mut String,
    ctx: &RenderCtx<'_>,
    mode: style::ColorMode,
) -> (std::borrow::Cow<'static, str>, bool) {
    if let Some(ruler) = ctx.config.layout.ruler.as_ref() {
        if let Some(glyph) = ruler.glyph.as_deref().filter(|g| !g.is_empty()) {
            let fg_color = ruler
                .foreground
                .clone()
                .unwrap_or_else(|| style::Color::Named("white".into()));
            let fg = style::sgr_fg(&fg_color, mode);
            let width = terminal_width();
            left.push_str(&fg);
            left.push_str(&glyph.repeat(width));
            left.push_str(style::reset_fg());
            left.push('\n');
        }
    }
    let frame_fg_color = ctx
        .config
        .layout
        .frame
        .as_ref()
        .and_then(|f| f.foreground.clone())
        .unwrap_or_else(|| style::Color::Named("white".into()));
    let frame_fg = style::sgr_fg(&frame_fg_color, mode);
    let frame_active = ctx
        .config
        .layout
        .frame
        .as_ref()
        .and_then(|f| f.glyph.as_deref())
        .is_some_and(|g| !g.is_empty());
    if frame_active {
        let glyph = ctx
            .config
            .layout
            .frame
            .as_ref()
            .and_then(|f| f.glyph.as_deref())
            .unwrap_or("");
        left.push_str(&frame_fg);
        left.push_str(glyph);
        left.push_str(style::reset_fg());
    }
    (frame_fg, frame_active)
}

/// Emit the multi-line trailing segment(s) behind the bottom-corner
/// frame glyph, using the same powerline arrow state machine as line 1
/// so richer line-2 layouts (slice 57: `layout.left_top_only`) render
/// transitions correctly when their segments carry backgrounds.
///
/// Extracted from `render_prompt` to keep that function under clippy's
/// `too_many_lines` threshold.
fn append_line2(
    left: &mut String,
    line2: &[(SegmentOutput, &str)],
    ctx: &RenderCtx<'_>,
    mode: style::ColorMode,
    separator: &str,
    frame_active: bool,
    frame_fg: &str,
) {
    if line2.is_empty() {
        return;
    }
    left.push('\n');
    if frame_active {
        let bottom_glyph = ctx
            .config
            .layout
            .frame
            .as_ref()
            .and_then(|f| f.bottom_glyph.as_deref())
            .unwrap_or("\u{2570}\u{2500}");
        left.push_str(frame_fg);
        left.push_str(bottom_glyph);
        left.push_str(style::reset_fg());
    }
    append_ribbon(left, line2, ctx, mode, separator);
}

/// Weave a slice of rendered segments into the left-side powerline
/// ribbon. The state machine is identical to the body of `render_prompt`
/// pre-slice-57 — extracted so line 1 and line 2 share one
/// implementation now that line 2 can carry bg-bearing segments.
fn append_ribbon(
    left: &mut String,
    items: &[(SegmentOutput, &str)],
    ctx: &RenderCtx<'_>,
    mode: style::ColorMode,
    separator: &str,
) {
    let mut prev_bg: Option<&style::Color> = None;
    for (i, (out, name)) in items.iter().enumerate() {
        let (pad_left, pad_right) = ctx
            .config
            .segments
            .get(*name)
            .map_or((0, 0), |sc| (sc.padding.left, sc.padding.right));
        match (prev_bg, out.background.as_ref()) {
            (Some(prev), Some(next)) => {
                left.push_str(&style::sgr_fg(prev, mode));
                left.push_str(&style::sgr_bg(next, mode));
                left.push(POWERLINE_ARROW);
                left.push_str(style::reset_fg());
            }
            (Some(prev), None) => {
                left.push_str(&style::sgr_fg(prev, mode));
                left.push_str(style::reset_bg());
                left.push(POWERLINE_ARROW);
                left.push_str(style::reset_fg());
                left.push(' ');
            }
            (None, _) => {
                // No previous bg: emit the configured separator between
                // segments (whether the next segment has a bg or not —
                // the bg-bearing segment will paint over the separator's
                // bg on the next cell).
                if i != 0 {
                    left.push_str(separator);
                }
            }
        }
        for _ in 0..pad_left {
            left.push(' ');
        }
        left.push_str(&out.text);
        for _ in 0..pad_right {
            left.push(' ');
        }
        prev_bg = out.background.as_ref();
    }

    // Final powerline arrow into terminal-default bg after the last
    // bg-bearing segment, matching slice 28's closing behaviour.
    if let Some(prev) = prev_bg {
        left.push_str(&style::sgr_fg(prev, mode));
        left.push_str(style::reset_bg());
        left.push(POWERLINE_ARROW);
        left.push_str(style::reset_fg());
    }
}

/// Render the transient (collapsed) prompt.
///
/// Always returns the rendered bytes — the policy decision ("should
/// the shell actually swap PROMPT for this collapsed form?") is the
/// init script's, gated on [`p10k_rs_config::TransientPromptMode`].
/// Returning the bytes unconditionally lets the binary expose
/// `--render-side transient` as a pure render request, regardless of
/// the user's mode setting.
///
/// The transient render walks no segments other than `prompt_char`,
/// emits no frame, ruler, segments, or trailing newline — one line of
/// `<fg>❯</fg>`. `TransientPromptMode::Quiet`,
/// `TransientPromptMode::SameDir`, and `TransientPromptMode::UniqueDir`
/// are presently indistinguishable at the render layer; a future slice
/// can branch on the mode without a schema change.
///
/// The result is wrapped for the target shell so its SGRs survive
/// zsh's prompt-width tracker exactly like `left`/`right`. When the
/// configured layout has no `prompt_char` segment the result is an
/// empty (post-wrap) string — the caller then assigns `PROMPT=""`,
/// which is the least surprising fallback.
fn render_transient(segments: &[Box<dyn Segment>], ctx: &RenderCtx<'_>) -> String {
    let mut out = String::new();
    for seg in segments {
        if seg.name() == "prompt_char" && seg.enabled(ctx) {
            let rendered = seg.render(ctx);
            out.push_str(&rendered.text);
            break;
        }
    }
    wrap_for_shell(&out, ctx.shell)
}

/// Assemble the right-side ribbon.
///
/// Mirrors the left-side renderer, with two differences:
///
/// 1. The powerline transition glyph is the LEFT-pointing arrow
///    (`\u{e0b2}`) so the slant tip points "into" the prompt instead
///    of away from it — matching how Powerlevel10k draws RPROMPT.
/// 2. Arrow colouring is mirrored. On the left, an arrow between two
///    bg-bearing segments is `fg=prev_bg, bg=next_bg` because the
///    slant tip is on the *right* of the cell, blending into the
///    upcoming segment. On the right, the slant tip is on the *left*
///    of the cell, so the arrow is `fg=next_bg, bg=prev_bg`: the tip
///    blends into the segment we're entering, the rest of the cell
///    blends into the segment we're leaving.
///
/// The leading transition (terminal-default → first bg-bearing
/// segment) is also mirrored: `fg=first_bg, bg=default` paints a
/// solid left-pointing arrow that fades from terminal background into
/// the segment's colour.
///
/// Segments with `background = None` join with the configured right
/// separator (`layout.separators.right`); if that field is unset we
/// fall back to `layout.separators.left` for backward compatibility
/// with users who only ever set the left separator, then to a single
/// space as the floor.
///
/// Returns an empty string when no segments survive `enabled()` — the
/// binary then assigns `RPROMPT=""` cleanly.
fn render_right(segments: &[Box<dyn Segment>], ctx: &RenderCtx<'_>) -> String {
    let mut out = String::new();
    let separator = ctx
        .config
        .layout
        .separators
        .right
        .as_deref()
        .or(ctx.config.layout.separators.left.as_deref())
        .unwrap_or(" ");
    let mode = ctx.config.colors;

    let enabled: Vec<(SegmentOutput, &str)> = segments
        .iter()
        .filter(|s| s.enabled(ctx))
        .map(|s| (s.render(ctx), s.name()))
        .collect();

    if enabled.is_empty() {
        return out;
    }

    let mut prev_bg: Option<&style::Color> = None;
    for (i, (seg_out, name)) in enabled.iter().enumerate() {
        let (pad_left, pad_right) = ctx
            .config
            .segments
            .get(*name)
            .map_or((0, 0), |sc| (sc.padding.left, sc.padding.right));
        match (prev_bg, seg_out.background.as_ref()) {
            (None, Some(next)) => {
                // Leading transition: fg=next_bg on default bg, then
                // left-pointing arrow into the segment.
                out.push_str(&style::sgr_fg(next, mode));
                out.push_str(style::reset_bg());
                out.push(POWERLINE_ARROW_LEFT);
                out.push_str(style::reset_fg());
            }
            (Some(prev), Some(next)) => {
                // Mirrored powerline transition: fg=next_bg (tip of the
                // left-pointing arrow, blending into the segment we're
                // entering), bg=prev_bg (rest of the cell, blending
                // into the segment we're leaving).
                out.push_str(&style::sgr_fg(next, mode));
                out.push_str(&style::sgr_bg(prev, mode));
                out.push(POWERLINE_ARROW_LEFT);
                out.push_str(style::reset_fg());
            }
            (_, None) => {
                if i != 0 {
                    out.push_str(separator);
                }
            }
        }
        for _ in 0..pad_left {
            out.push(' ');
        }
        out.push_str(&seg_out.text);
        for _ in 0..pad_right {
            out.push(' ');
        }
        prev_bg = seg_out.background.as_ref();
    }

    out
}

/// Best-effort terminal width for the ruler decoration.
///
/// Probe order:
/// 1. `ioctl(TIOCGWINSZ)` via `rustix::termios::tcgetwinsize` against
///    stdout, then stderr, then stdin. Any of the three may be the
///    controlling TTY when this binary runs as a subprocess of the
///    shell (stdout is most commonly redirected for command-substitution
///    callers, so we try it first but fall through to the other two).
/// 2. `$COLUMNS`. zsh / bash / fish populate this in interactive shells
///    but do **not** export it to subprocesses, so it is only useful
///    when the user's init script explicitly `export`s it.
/// 3. Hard fallback of 80 columns.
///
/// Architectural note: `p10k-rs-core` is otherwise I/O-free (see this
/// crate's `CLAUDE.md`). `terminal_width` is the documented exception —
/// previously it read `$COLUMNS`, and as of slice 29 it also performs
/// an `ioctl` syscall. Both are read-only probes of the host the
/// renderer is producing output for; no other render code reaches out
/// to the OS.
fn terminal_width() -> usize {
    if let Some(w) = tty_winsize_cols() {
        return w;
    }
    std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&w: &usize| w > 0)
        .unwrap_or(80)
}

/// Try `TIOCGWINSZ` against the standard streams in priority order.
///
/// Returns `Some(cols)` for the first stream that is a TTY *and* reports
/// a positive `ws_col`. A pipe or closed fd yields `ENOTTY` from rustix,
/// which we treat as "not a winsize source, try the next" via `.ok()`.
fn tty_winsize_cols() -> Option<usize> {
    use rustix::stdio::{stderr, stdin, stdout};
    use rustix::termios::tcgetwinsize;

    for fd in [stdout(), stderr(), stdin()] {
        if let Ok(ws) = tcgetwinsize(fd) {
            if ws.ws_col > 0 {
                return Some(usize::from(ws.ws_col));
            }
        }
    }
    None
}

/// Per-shell escape-wrapping for the assembled prompt string.
///
/// - **zsh**: each `\x1b[…m` SGR escape is wrapped in `%{…%}` so the prompt
///   width is computed correctly. OSC sequences (`\x1b]…<BEL>` or
///   `\x1b]…\x1b\\`) are also bracketed — zsh's prompt-width tracker
///   doesn't know they're zero-width without the `%{…%}` markers, and
///   slice 55 started emitting OSC 7 / OSC 133 inside the prompt for
///   shell-integration. Outside of those wrapped escapes, every
///   literal `%` is doubled to `%%` because zsh treats `%` in PROMPT as the
///   start of a prompt-expansion escape (`%n` → username, `%/` → cwd, …),
///   and that is how an attacker who controls a branch name or directory
///   name can leak information through the prompt — or, with `PROMPT_SUBST`
///   set, execute commands. SGR escape bodies emitted by segments contain
///   no literal `%`, so the doubling pass only ever fires on text content.
/// - **fish / bash**: pass-through. Bash uses `\[…\]` which we'll add when
///   bash support lands; fish handles ANSI natively in its prompt fns.
///   Neither shell interprets `%` in its prompt, so no doubling is needed.
fn wrap_for_shell(s: &str, shell: Shell) -> String {
    if shell != Shell::Zsh {
        return s.to_owned();
    }
    if !s.contains('\x1b') && !s.contains('%') {
        return s.to_owned();
    }
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + 16);
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // CSI: scan to the ECMA-48 final byte (0x40..=0x7E). SGR
            // ('m') is the only sequence shipped today; accepting any
            // CSI final byte is defence-in-depth so a future segment
            // that emits, e.g., cursor positioning (`H`) or device-
            // status (`n`) still gets bracketed in `%{…%}` instead of
            // leaking unbracketed bytes that zsh's width tracker would
            // miscount.
            let mut j = i + 2;
            while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                j += 1;
            }
            if j < bytes.len() {
                out.push_str("%{");
                out.push_str(&s[i..=j]);
                out.push_str("%}");
                i = j + 1;
                continue;
            }
        }
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b']' {
            // OSC: terminated by either `\x07` (BEL) or `\x1b\\` (ST).
            // Both terminators exist in practice — our OSC 7 uses ST,
            // OSC 133 uses BEL — so scan for whichever comes first.
            let mut j = i + 2;
            let mut end_inclusive: Option<usize> = None;
            while j < bytes.len() {
                if bytes[j] == 0x07 {
                    end_inclusive = Some(j);
                    break;
                }
                if bytes[j] == 0x1b && j + 1 < bytes.len() && bytes[j + 1] == b'\\' {
                    end_inclusive = Some(j + 1);
                    break;
                }
                j += 1;
            }
            if let Some(end) = end_inclusive {
                out.push_str("%{");
                out.push_str(&s[i..=end]);
                out.push_str("%}");
                i = end + 1;
                continue;
            }
        }
        if bytes[i] == b'%' {
            out.push_str("%%");
            i += 1;
            continue;
        }
        // Unrecognised byte at i — copy one char's worth and advance.
        let ch_end = next_char_boundary(s, i);
        out.push_str(&s[i..ch_end]);
        i = ch_end;
    }
    out
}

/// Find the byte index of the char boundary strictly after `i`.
fn next_char_boundary(s: &str, i: usize) -> usize {
    let mut j = i + 1;
    while j < s.len() && !s.is_char_boundary(j) {
        j += 1;
    }
    j
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn wrap_for_zsh_brackets_each_sgr() {
        let raw = "\x1b[34mhello\x1b[39m";
        let wrapped = wrap_for_shell(raw, Shell::Zsh);
        assert_eq!(wrapped, "%{\x1b[34m%}hello%{\x1b[39m%}");
    }

    #[test]
    fn wrap_for_zsh_handles_unicode_between_escapes() {
        let raw = "\x1b[34m~/code/é\x1b[39m";
        let wrapped = wrap_for_shell(raw, Shell::Zsh);
        assert_eq!(wrapped, "%{\x1b[34m%}~/code/é%{\x1b[39m%}");
    }

    #[test]
    fn wrap_for_zsh_passes_through_plain_text() {
        let raw = "no escapes here";
        assert_eq!(wrap_for_shell(raw, Shell::Zsh), raw);
    }

    #[test]
    fn wrap_for_non_zsh_is_passthrough() {
        let raw = "\x1b[34mhello\x1b[39m";
        assert_eq!(wrap_for_shell(raw, Shell::Fish), raw);
        assert_eq!(wrap_for_shell(raw, Shell::Bash), raw);
    }

    #[test]
    fn wrap_for_zsh_handles_unterminated_escape_gracefully() {
        // A stray ESC[ with parameter bytes but no ECMA-48 final byte
        // (0x40..=0x7E): copy bytes through, don't infinite-loop. Digits
        // are parameter bytes per ECMA-48 so `\x1b[12345` has no final
        // byte and is genuinely unterminated.
        let raw = "\x1b[34mok\x1b[12345";
        let out = wrap_for_shell(raw, Shell::Zsh);
        // The well-formed escape gets wrapped; the broken tail is preserved.
        assert!(out.starts_with("%{\x1b[34m%}ok"));
        assert!(out.ends_with("\x1b[12345"));
    }

    #[test]
    fn wrap_for_zsh_doubles_literal_percent_in_text() {
        // The C1 fix: `%n` in segment output (e.g. a branch named "%n@%m")
        // must come out as `%%n@%%m` so zsh PROMPT-expansion sees a literal
        // `%` instead of the user-name escape.
        let raw = "\x1b[33m%n@%m\x1b[39m";
        let wrapped = wrap_for_shell(raw, Shell::Zsh);
        assert_eq!(wrapped, "%{\x1b[33m%}%%n@%%m%{\x1b[39m%}");
    }

    #[test]
    fn wrap_for_zsh_doubles_percent_with_no_sgr() {
        // No ANSI escapes anywhere — only the `%` doubling fires.
        let raw = "branch%n";
        assert_eq!(wrap_for_shell(raw, Shell::Zsh), "branch%%n");
    }

    #[test]
    fn wrap_for_zsh_does_not_double_brackets_we_emit() {
        // `wrap_for_shell` itself emits `%{` and `%}` around SGRs. Those
        // come from the wrapper, not the input — they must not be doubled
        // again on the way out.
        let raw = "\x1b[34mhello\x1b[39m";
        let wrapped = wrap_for_shell(raw, Shell::Zsh);
        assert_eq!(wrapped, "%{\x1b[34m%}hello%{\x1b[39m%}");
        // Quick guard: count `%{` / `%}` should match the SGR count, not
        // be doubled.
        assert_eq!(wrapped.matches("%{").count(), 2);
        assert_eq!(wrapped.matches("%}").count(), 2);
    }

    #[test]
    fn wrap_for_non_zsh_leaves_percent_alone() {
        // Bash and fish don't expand `%` in their prompts — doubling
        // would render literal `%%` in the user's terminal.
        let raw = "branch%n";
        assert_eq!(wrap_for_shell(raw, Shell::Bash), raw);
        assert_eq!(wrap_for_shell(raw, Shell::Fish), raw);
    }

    #[test]
    fn wrap_for_zsh_brackets_csi_with_non_m_final_byte() {
        // CSI sequences end on any ECMA-48 final byte (0x40..=0x7E), not
        // just `m`. SGR is the only one shipped today, but the bracketing
        // pass must not leak unbracketed bytes if a future segment emits
        // cursor positioning (`H`), device status (`n`), erase-in-line
        // (`K`), or similar — otherwise zsh's width tracker undercounts
        // the prompt by the CSI body's byte length.
        let raw = "\x1b[2Khello\x1b[5;10H";
        let wrapped = wrap_for_shell(raw, Shell::Zsh);
        assert_eq!(wrapped, "%{\x1b[2K%}hello%{\x1b[5;10H%}");
    }

    #[test]
    fn wrap_for_zsh_does_not_double_percent_inside_osc_brackets() {
        // OSC content is already wrapped in `%{…%}` because zsh treats
        // those bytes as zero-width. A `%` inside the OSC body (e.g. a
        // percent-encoded space in an OSC 7 URI: `file:///tmp/foo%20bar`)
        // must NOT be doubled by the `%`-expansion pass — that would
        // corrupt the URI seen by host terminals parsing OSC 7.
        let raw = "\x1b]7;file:///tmp/foo%20bar\x1b\\";
        let wrapped = wrap_for_shell(raw, Shell::Zsh);
        assert_eq!(wrapped, "%{\x1b]7;file:///tmp/foo%20bar\x1b\\%}");
        // The literal `%` should appear exactly once (the `%20`), with
        // the only extra `%{`/`%}` coming from the OSC wrapper itself.
        assert_eq!(wrapped.matches('%').count(), 3);
    }

    #[test]
    fn terminal_width_is_at_least_one_column() {
        // Sanity check, not a byte assertion — width is environmental.
        // Under `cargo test` stdio is typically a pipe (so ioctl ENOTTYs
        // out), `$COLUMNS` is not exported, and we land on the 80-column
        // fallback. If a developer runs the suite from a real TTY we
        // get the live width instead. Either path is `>= 1`.
        assert!(terminal_width() >= 1);
    }

    #[test]
    fn osc7_for_cwd_encodes_simple_path() {
        let s = osc7_for_cwd(Path::new("/home/seaburdz"));
        assert_eq!(s, "\x1b]7;file:///home/seaburdz\x1b\\");
    }

    #[test]
    fn osc7_for_cwd_percent_encodes_spaces() {
        let s = osc7_for_cwd(Path::new("/tmp/foo bar"));
        assert_eq!(s, "\x1b]7;file:///tmp/foo%20bar\x1b\\");
    }

    #[test]
    fn wrap_for_zsh_brackets_osc_bel_terminated() {
        // OSC 133 ends with BEL (`\x07`). The wrap pass must mark the
        // whole sequence zero-width so zsh's prompt-width tracker
        // doesn't count it as visible columns.
        let raw = "\x1b]133;A\x07hello";
        let wrapped = wrap_for_shell(raw, Shell::Zsh);
        assert_eq!(wrapped, "%{\x1b]133;A\x07%}hello");
    }

    #[test]
    fn wrap_for_zsh_brackets_osc_st_terminated() {
        // OSC 7 uses the `\x1b\\` (ST) terminator. Same width-tracking
        // problem; same fix.
        let raw = "\x1b]7;file:///tmp\x1b\\done";
        let wrapped = wrap_for_shell(raw, Shell::Zsh);
        assert_eq!(wrapped, "%{\x1b]7;file:///tmp\x1b\\%}done");
    }

    fn render_ctx_for_host<'a>(
        cfg: &'a Config,
        env: &'a EnvSnapshot,
        cwd: &'a Path,
        host: HostKind,
    ) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Bash, // bypass `wrap_for_shell` so we can grep raw bytes
            host,
            cwd,
            git: None,
            jj: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
            upcoming_command: "",
        }
    }

    #[test]
    fn render_prompt_emits_osc_when_host_is_claude_code() {
        let cfg = Config::default();
        let env = EnvSnapshot::default();
        let cwd = Path::new("/tmp/work");
        let ctx = render_ctx_for_host(&cfg, &env, cwd, HostKind::ClaudeCode);
        let prompt = render_prompt(&[], &[], &ctx);
        assert!(
            prompt.left.contains("\x1b]7;file:///tmp/work\x1b\\"),
            "expected OSC 7 cwd report in left: {:?}",
            prompt.left
        );
        assert!(
            prompt.left.contains("\x1b]133;A\x07"),
            "expected OSC 133 A in left: {:?}",
            prompt.left
        );
        assert!(
            prompt.left.contains("\x1b]133;B\x07"),
            "expected OSC 133 B in left: {:?}",
            prompt.left
        );
    }

    /// Minimal no-background `Segment` fixture so we can drive
    /// `render_right` directly. Real segments either own a background
    /// (powerline-arrow path) or are the singular `prompt_char`
    /// (one-of-a-kind, can't be listed twice in a TOML layout); a
    /// fixture is the only way to exercise the no-bg → no-bg join the
    /// right-separator path covers.
    #[derive(Debug)]
    struct NoBgText(&'static str);
    impl Segment for NoBgText {
        fn name(&self) -> &'static str {
            "test_no_bg"
        }
        fn render(&self, _ctx: &RenderCtx<'_>) -> SegmentOutput {
            SegmentOutput {
                text: self.0.to_string(),
                plain_len: u16::try_from(self.0.chars().count()).unwrap_or(u16::MAX),
                state: None,
                icon: None,
                background: None,
            }
        }
    }

    #[test]
    fn render_right_uses_right_separator_between_no_bg_segments() {
        // Slice 56: `[layout.separators].right` takes precedence over
        // `.left` on the right side, joining adjacent no-bg segments.
        let cfg = Config::from_toml(
            "schema_version = 1\n\
             [layout.separators]\n\
             right = \" >> \"\n",
        )
        .expect("fixture parses");
        let env = EnvSnapshot::default();
        let cwd = Path::new("/tmp");
        let ctx = render_ctx_for_host(&cfg, &env, cwd, HostKind::None);
        let segs: Vec<Box<dyn Segment>> = vec![Box::new(NoBgText("A")), Box::new(NoBgText("B"))];
        let out = render_right(&segs, &ctx);
        assert!(
            out.contains("A >> B"),
            "right separator must join adjacent no-bg segments: {out:?}",
        );
    }

    #[test]
    fn render_right_falls_back_to_left_separator_when_right_unset() {
        // Backward-compat: users who only set `separators.left` keep
        // their old right-side spacing for free.
        let cfg = Config::from_toml(
            "schema_version = 1\n\
             [layout.separators]\n\
             left = \" | \"\n",
        )
        .expect("fixture parses");
        let env = EnvSnapshot::default();
        let cwd = Path::new("/tmp");
        let ctx = render_ctx_for_host(&cfg, &env, cwd, HostKind::None);
        let segs: Vec<Box<dyn Segment>> = vec![Box::new(NoBgText("A")), Box::new(NoBgText("B"))];
        let out = render_right(&segs, &ctx);
        assert!(
            out.contains("A | B"),
            "left-separator fallback must apply when right is None: {out:?}",
        );
    }

    #[test]
    fn render_right_right_separator_wins_over_left() {
        // When both are set, `.right` takes precedence on the right side.
        let cfg = Config::from_toml(
            "schema_version = 1\n\
             [layout.separators]\n\
             left = \" | \"\n\
             right = \" >> \"\n",
        )
        .expect("fixture parses");
        let env = EnvSnapshot::default();
        let cwd = Path::new("/tmp");
        let ctx = render_ctx_for_host(&cfg, &env, cwd, HostKind::None);
        let segs: Vec<Box<dyn Segment>> = vec![Box::new(NoBgText("A")), Box::new(NoBgText("B"))];
        let out = render_right(&segs, &ctx);
        assert!(
            out.contains("A >> B"),
            "right separator must override left when both set: {out:?}",
        );
        assert!(
            !out.contains(" | "),
            "left separator must not leak into right-side render: {out:?}",
        );
    }

    #[test]
    fn render_prompt_omits_osc_when_host_is_none() {
        // Vanilla terminals get the byte-identical historical output.
        let cfg = Config::default();
        let env = EnvSnapshot::default();
        let cwd = Path::new("/tmp/work");
        let ctx = render_ctx_for_host(&cfg, &env, cwd, HostKind::None);
        let prompt = render_prompt(&[], &[], &ctx);
        assert!(
            !prompt.left.contains("\x1b]7;"),
            "OSC 7 must not appear without a host: {:?}",
            prompt.left
        );
        assert!(
            !prompt.left.contains("\x1b]133;"),
            "OSC 133 must not appear without a host: {:?}",
            prompt.left
        );
    }
}

// -- Shared placeholder types ------------------------------------------------
//
// These are the minimum surface other crates compile against. They will grow
// real fields as crates land. Each one is documented; the field-level docs
// will arrive with the implementation.

// Top-level configuration.
//
// The full shape lives in `p10k-rs-config`; we re-export it here so
// `p10k-rs-core` can reference `Config` in `RenderCtx` and trait signatures
// without consumers needing to depend on `p10k-rs-config` directly.
//
// Dependency direction is one-way: `p10k-rs-core` depends on
// `p10k-rs-config`, never the other way around. The sanitiser
// `p10k-rs-config` runs at parse time is a small inlined copy of
// `crate::safety::sanitize_for_terminal` — see that module's docs.
pub use p10k_rs_config::Config;

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
/// Carried by [`RenderCtx::host`] so segments can react to running inside
/// an AI shell. Detection lives in `p10k-rs-ai::detect_host_kind`; this
/// crate stays I/O-free per its `CLAUDE.md` and only owns the enum that
/// flows through the render pipeline.
///
/// The variants are deliberately unit-only for the MVP — per-host
/// structured data (model strings, context tokens, etc.) can be added
/// in a future slice without breaking the segment surface, thanks to
/// `#[non_exhaustive]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum HostKind {
    /// No AI host detected.
    #[default]
    None,
    /// Anthropic Claude Code CLI.
    ClaudeCode,
    /// Aider AI pair-programmer.
    Aider,
    /// Cursor editor terminal.
    Cursor,
}

/// Pre-computed git state for the current cwd.
///
/// Held by `RenderCtx::git` so segments can render git info without each
/// of them re-spawning `git`. Producers live in `p10k-rs-git`. Fields
/// match the upstream p10k `VCS_STATUS_*` set we currently consume; new
/// fields (stash count, push-remote ahead/behind, etc.) land as segments
/// start needing them.
///
/// Backends that can't cheaply populate every field leave the others at
/// their default. The `ShellOut` fallback only fills `branch` and `dirty`,
/// for instance — getting ahead/behind out of plain `git` would need
/// extra subprocess invocations and the fallback is already the slow path.
#[derive(Debug, Default, Clone)]
pub struct GitState {
    /// Current branch name. `"HEAD"` for detached. Empty if unknown.
    ///
    /// Wrapped in [`SafeText`] so the type system enforces the
    /// "this string has passed `sanitize_for_terminal`" invariant —
    /// segments cannot accidentally re-introduce control bytes that
    /// would steer the terminal or zsh's prompt-expansion engine.
    pub branch: SafeText,
    /// `true` if the working tree has any uncommitted changes (modified,
    /// staged, untracked, or conflicted).
    pub dirty: bool,
    /// Commits ahead of the configured upstream. 0 if no upstream or unknown.
    pub ahead: u32,
    /// Commits behind the configured upstream. 0 if no upstream or unknown.
    pub behind: u32,
    /// Number of staged changes (tree-vs-index).
    pub staged: u32,
    /// Number of unstaged changes (index-vs-worktree).
    pub unstaged: u32,
    /// Number of untracked files.
    pub untracked: u32,
    /// `true` if any path is in conflict (unmerged stage).
    pub has_conflicts: bool,
    /// HEAD commit OID as full hex. Empty if unknown / unborn branch.
    /// Same `SafeText` invariant as [`GitState::branch`].
    pub commit: SafeText,
    /// Number of stashed changes in the current repo. 0 if no stashes or
    /// unknown.
    pub stash: u32,
    /// Repository action currently in progress, if any: one of
    /// `"merge"`, `"rebase"`, `"cherry-pick"`, `"revert"`, `"bisect"`.
    /// Empty when the working tree is in a normal state.
    ///
    /// Wrapped in [`SafeText`] for the same reason as `branch` — the value
    /// could ride through from a malicious `.git/MERGE_MSG` etc.
    pub action: SafeText,
    /// Greatest (lexicographic) tag pointing at HEAD, per gitstatusd's
    /// `VCS_STATUS_TAG` (wire-format field 18). Empty when HEAD isn't on
    /// a tag, when the backend doesn't surface tags (`ShellOut`), or when
    /// the daemon's wire format predates the tag field.
    ///
    /// `SafeText` invariant: tag names ride straight off the wire and into
    /// the prompt next to the branch / SHA, so the type system pins down
    /// the sanitisation pass for the same reason as [`GitState::branch`].
    pub tag: SafeText,
}

/// Pre-computed Jujutsu (`jj`) state for the current cwd.
///
/// Mirror of [`GitState`] for the Jujutsu VCS — held by
/// [`RenderCtx::jj`] so segments can render jj info without re-shelling
/// out per prompt. Producer lives in `p10k-rs-jj` (`detect_jj`).
///
/// jj's data model differs from git's in two load-bearing ways the
/// fields capture:
///
/// - `change_id` is jj's primary identifier (replaces git's commit
///   SHA as the identity of a working state); `commit_id` is the
///   git-equivalent SHA, populated when jj's backend is the git
///   compat one (the default for new repos).
/// - `bookmark` is jj's closest analogue to a git branch — a movable
///   named pointer. Empty when the change has no bookmark; jj allows
///   that whereas git always parks HEAD on *something*.
///
/// Backends that can't cheaply populate every field leave the others
/// at their default; the MVP `detect_jj` shell-out fills the first
/// five and leaves `conflicts` / `divergent` at `false`.
#[derive(Debug, Default, Clone)]
pub struct JjState {
    /// Working-copy change identifier (`@` in jj parlance). `SafeText`
    /// invariant — same reason as [`GitState::branch`].
    pub change_id: SafeText,
    /// Git-equivalent commit SHA. Empty when the backend isn't git.
    pub commit_id: SafeText,
    /// First bookmark pointing at `@`. Empty when the change has none.
    pub bookmark: SafeText,
    /// First line of the commit description. Empty for the implicit
    /// "no description set" placeholder.
    pub description: SafeText,
    /// `true` when the working copy has any uncommitted change.
    pub dirty: bool,
    /// `true` when the working copy or its parents are in conflict.
    pub conflicts: bool,
    /// `true` when multiple changes share this `change_id`.
    pub divergent: bool,
}

/// Snapshot of environment variables relevant to segments.
///
/// Built by the binary at the start of each prompt and held in `RenderCtx`.
/// Segments read through this rather than calling [`std::env::var`] so unit
/// tests can substitute fixtures.
///
/// Fields land segment-by-segment as need arises (the producer-discipline
/// pattern flagged in the architecture review). Today: just `home`, used
/// by the `dir` segment to collapse `$HOME` → `~` without paying a
/// global-mutex `getenv` per render.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct EnvSnapshot {
    /// Value of `$HOME`, captured once per prompt at construction. `None`
    /// when the variable is unset or contains invalid Unicode; segments
    /// that depend on home-collapse fall back to the raw cwd in that case.
    pub home: Option<String>,
}

impl EnvSnapshot {
    /// Build an `EnvSnapshot` by reading the relevant `std::env` keys
    /// once. Use this in the binary's prompt path; tests construct
    /// fixtures via [`EnvSnapshot::default`] (everything `None`) and
    /// set the fields they need by hand.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            home: std::env::var("HOME").ok(),
        }
    }
}
