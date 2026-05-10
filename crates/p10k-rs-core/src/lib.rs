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

pub mod safety;
pub mod style;

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
/// Walks `segments` in order, calls `enabled` then `render`, joins outputs
/// with a single space, and post-processes the assembled string for the
/// target shell (zsh wants ANSI escapes wrapped in `%{…%}` so it can track
/// prompt width correctly).
///
/// Pure: given identical `segments` and `ctx`, returns identical output.
#[must_use]
pub fn render_prompt(segments: &[Box<dyn Segment>], ctx: &RenderCtx<'_>) -> Prompt {
    let mut left = String::new();
    for seg in segments {
        if !seg.enabled(ctx) {
            continue;
        }
        let out = seg.render(ctx);
        if !left.is_empty() {
            left.push(' ');
        }
        left.push_str(&out.text);
    }
    let left = wrap_for_shell(&left, ctx.shell);
    Prompt {
        left,
        right: String::new(),
        transient: None,
    }
}

/// Per-shell escape-wrapping for the assembled prompt string.
///
/// - **zsh**: each `\x1b[…m` SGR escape is wrapped in `%{…%}` so the prompt
///   width is computed correctly. Outside of those wrapped escapes, every
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
            // Scan to terminating SGR byte ('m' for the only escapes we emit).
            let mut j = i + 2;
            while j < bytes.len() && bytes[j] != b'm' {
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
        // A stray ESC[ with no terminator: copy bytes through, don't loop.
        let raw = "\x1b[34mok\x1b[broken";
        let out = wrap_for_shell(raw, Shell::Zsh);
        // The well-formed escape gets wrapped; the broken tail is preserved.
        assert!(out.starts_with("%{\x1b[34m%}ok"));
        assert!(out.ends_with("\x1b[broken"));
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
}

/// Snapshot of environment variables relevant to segments.
///
/// Built by the binary at the start of each prompt and held in `RenderCtx`.
/// Segments read through this rather than calling [`std::env::var`] so unit
/// tests can substitute fixtures.
#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct EnvSnapshot {}
