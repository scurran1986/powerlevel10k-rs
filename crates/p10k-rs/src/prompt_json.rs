//! `p10k-rs prompt --json` payload assembly (v0.2 Phase 5).
//!
//! Emits a structured snapshot of one render so AI hosts, integration
//! partners, and debugging tools can consume the prompt state without
//! shelling out to a second invocation or screen-scraping ANSI text.
//!
//! Schema version: `p10krs.prompt/v1`. The JSON Schema document lives at
//! `docs/schema/p10krs.prompt.v1.json`. A reference page rendered into
//! the mdBook lives at `docs/src/reference/prompt-json.md`. Changes to
//! the wire shape must bump the schema version (and ship a new schema
//! file alongside the old one for at least one minor cycle).
//!
//! All untrusted text fields (`branch`, `cwd`, per-segment `plain`)
//! flow through the same `SafeText` boundary the styled-text render
//! path uses — sanitisation happens before this module ever sees the
//! bytes (see `crates/p10k-rs-core/CLAUDE.md` § "`SafeText` invariant").
//! ANSI-bearing fields (`ansi_text`, per-segment `ansi`) round-trip the
//! raw rendered bytes verbatim because that *is* the wire contract for
//! consumers that want to paint the prompt themselves.

use std::time::SystemTime;

use p10k_rs_config::Color;
use p10k_rs_core::{Config, GitState, HostKind, RenderCtx, Segment, SegmentOutput, Shell};

/// Stable schema identifier emitted into every payload.
///
/// The `<namespace>/v<n>` shape mirrors the conventions used by
/// `claude-code` and Cursor statusline contracts so a consumer parsing
/// multiple p10k-rs subcommand outputs can switch on a single field.
const SCHEMA_VERSION: &str = "p10krs.prompt/v1";

/// Top-level wire shape for `prompt --json`. Field order is fixed by
/// derive order so `serde_json::to_string_pretty` produces a stable
/// human-readable line ordering — useful for snapshot diffing.
#[derive(Debug, serde::Serialize)]
pub(crate) struct PromptJson {
    /// Stable schema identifier — always [`SCHEMA_VERSION`].
    pub schema_version: &'static str,
    /// Unix seconds at the moment the binary started assembling the
    /// payload. Lets a consumer detect stale snapshots vs. the live
    /// shell.
    pub produced_at_unix: u64,
    /// `Cargo.toml`'s `version` for the running binary. Same value the
    /// `version --json` subcommand reports.
    pub p10k_rs_version: &'static str,
    /// Which shell asked for the prompt (`zsh`, `bash`, `fish`).
    pub shell: &'static str,
    /// Exit status the shell forwarded via `--last-status`. `0` when the
    /// shell didn't pass one (consistent with the default in clap).
    pub exit_status: i32,
    /// Detected AI / agent host the shell is running inside, if any.
    /// Mirrors `HostKind::Display`'s short labels (`claude-code`,
    /// `cursor`, `aider`, `goose`, `warp`, or the generic agent name);
    /// `null` when no host was detected.
    pub host: Option<String>,
    /// Current working directory at render time, plain UTF-8 string
    /// (lossy for non-UTF-8 paths, same convention as the styled
    /// `dir` segment).
    pub cwd: String,
    /// Full rendered prompt for the requested side, ANSI escapes
    /// included. This is the same string `prompt` (without `--json`)
    /// would print to stdout for `--render-side left`.
    pub ansi_text: String,
    /// `ansi_text` with ANSI SGR / OSC escapes stripped — drop-in for
    /// log lines, file names, and other plain-text consumers.
    pub plain_text: String,
    /// Per-segment breakdown for the left ribbon (`layout.left` after
    /// `disabled` / `show_in_dir` / `show_on_command` gates).
    pub left: Vec<SegmentJson>,
    /// Same as [`PromptJson::left`] but for the right ribbon.
    pub right: Vec<SegmentJson>,
    /// Git state for the cwd, when available. `null` outside a git
    /// repo OR when the backend chain (gitstatusd → `ShellOut` →
    /// `GixBackend`) couldn't produce a state.
    pub git: Option<GitJson>,
}

/// Per-segment wire shape inside [`PromptJson::left`] / [`PromptJson::right`].
#[derive(Debug, serde::Serialize)]
pub(crate) struct SegmentJson {
    /// Segment identifier — matches the TOML config key
    /// (`Segment::name`).
    pub name: &'static str,
    /// Segment's styled output as it would appear in the prompt
    /// ribbon: ANSI escapes included, no surrounding separators /
    /// powerline arrows (those are owned by the renderer, not the
    /// segment).
    pub ansi: String,
    /// Same as `ansi` with ANSI escapes stripped.
    pub plain: String,
    /// Optional state tag the segment reported (`"ok"`, `"error"`,
    /// `"writable"`, …). `null` when the segment doesn't carry state.
    pub state: Option<&'static str>,
    /// Glyph the segment chose for its icon slot, when applicable.
    /// `null` when the segment has no icon.
    pub icon: Option<&'static str>,
    /// Background colour the segment painted itself on, surfaced from
    /// `SegmentOutput::background`. `null` when the segment opted out
    /// of a background (inline rendering). Foreground is not surfaced
    /// per-segment today — see the module's `prompt-json.md`
    /// documentation for the rationale.
    pub background: Option<Color>,
}

/// Git state mirror exposed in [`PromptJson::git`].
///
/// Field set matches [`p10k_rs_core::GitState`] except for `commit`,
/// `tag`, `stash`: those exist in the core type but most consumers
/// asking for a JSON prompt snapshot only care about the
/// branch / divergence / dirty triangulation, so we surface that
/// subset by default. Future additions go behind feature flags or a
/// schema version bump.
#[derive(Debug, serde::Serialize)]
pub(crate) struct GitJson {
    /// Current branch name. Empty string when detached or unknown.
    pub branch: String,
    /// Commits ahead of the configured upstream.
    pub ahead: u32,
    /// Commits behind the configured upstream.
    pub behind: u32,
    /// Number of staged changes.
    pub staged: u32,
    /// Number of unstaged changes.
    pub unstaged: u32,
    /// Number of untracked files.
    pub untracked: u32,
    /// `true` if any path is in conflict.
    pub has_conflicts: bool,
    /// In-progress repo action (`merge`, `rebase`, …), `null` when the
    /// working tree is in a normal state.
    pub action: Option<String>,
}

/// Strip ANSI SGR (`ESC [ … m`) and OSC (`ESC ] … BEL` or
/// `ESC ] … ESC \`) sequences from `s`, returning a plain-text view.
///
/// Also drops zsh's `%{` / `%}` "non-printing region" markers — the
/// render pipeline wraps every escape in them when the target shell
/// is zsh (so its PROMPT-width tracker stays right), and once the
/// escapes inside are stripped the markers are pure transport noise
/// for a plain-text consumer.
///
/// Pure: no allocations beyond the returned `String`. Conservative by
/// design — we recognise the two escape families the render pipeline
/// emits (SGR for colour, OSC for shell integration / cwd reporting)
/// and pass everything else through. Powerline glyphs (`U+E0B0` etc.)
/// stay in the plain text because they're visible characters; a
/// consumer that wants a no-glyph view can post-process.
///
/// Not exported. Tests live alongside in the `#[cfg(test)]` block at
/// the bottom of this file so the regression markers move with the
/// implementation if it ever migrates to a helper crate.
fn strip_ansi(s: &str) -> String {
    // Char-level state machine: ANSI sequences are pure ASCII so a
    // `chars()` walk preserves UTF-8 in the surrounding content. The
    // string is short (hundreds of bytes per prompt) — readability
    // beats the micro-optimisation of a byte-indexed loop.
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{001b}' {
            match chars.peek().copied() {
                Some('[') => {
                    // CSI: ESC [ <params> <final byte in 0x40..=0x7e>.
                    // Consume `[` then drop chars until a final byte.
                    chars.next();
                    for nc in chars.by_ref() {
                        let n = nc as u32;
                        if (0x40..=0x7e).contains(&n) {
                            break;
                        }
                    }
                    continue;
                }
                Some(']') => {
                    // OSC: ESC ] <payload> ST, where ST is BEL (0x07)
                    // or ESC \\ (0x1b 0x5c).
                    chars.next();
                    while let Some(nc) = chars.next() {
                        if nc == '\u{0007}' {
                            break;
                        }
                        if nc == '\u{001b}' && chars.peek().copied() == Some('\\') {
                            chars.next();
                            break;
                        }
                    }
                    continue;
                }
                _ => {
                    // Unknown / truncated ESC: drop the ESC plus the
                    // next char (if any) and move on.
                    chars.next();
                    continue;
                }
            }
        }
        if c == '%' {
            // zsh "non-printing region" markers `%{` / `%}` wrap every
            // ANSI sequence the render pipeline emits when the target
            // shell is zsh. With the inner escapes already stripped,
            // the markers themselves are transport noise — drop the
            // matched pair.
            if matches!(chars.peek().copied(), Some('{' | '}')) {
                chars.next();
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Resolve the short label for `host`, returning `None` for
/// [`HostKind::None`].
///
/// The wire contract is "no host detected → JSON null", not the literal
/// string `"none"`. Distinguishing them lets a consumer cheaply branch
/// on `host !== null` without an extra string compare.
fn host_label(host: &HostKind) -> Option<String> {
    match host {
        HostKind::None => None,
        other => Some(other.to_string()),
    }
}

/// Translate the core [`Shell`] enum to its CLI string label.
fn shell_label(shell: Shell) -> &'static str {
    match shell {
        Shell::Zsh => "zsh",
        Shell::Fish => "fish",
        Shell::Bash => "bash",
    }
}

/// Walk `segments`, calling `render` on each enabled segment, and
/// return a [`SegmentJson`] entry per element.
///
/// Matches the production renderer's "skip when `enabled` returns
/// false" behaviour. Segments that report empty output are still
/// included — a hidden segment carrying its name + empty `ansi` /
/// `plain` is useful debugging context.
fn build_segments_json(segments: &[Box<dyn Segment>], ctx: &RenderCtx<'_>) -> Vec<SegmentJson> {
    let mut out = Vec::with_capacity(segments.len());
    for seg in segments {
        if !seg.enabled(ctx) {
            continue;
        }
        let SegmentOutput {
            text,
            state,
            icon,
            background,
            ..
        } = seg.render(ctx);
        out.push(SegmentJson {
            name: seg.name(),
            plain: strip_ansi(&text),
            ansi: text,
            state,
            icon,
            background,
        });
    }
    out
}

/// Project a [`GitState`] into the JSON-wire [`GitJson`] shape.
fn build_git_json(git: &GitState) -> GitJson {
    let action = git.action.as_str();
    GitJson {
        branch: git.branch.as_str().to_owned(),
        ahead: git.ahead,
        behind: git.behind,
        staged: git.staged,
        unstaged: git.unstaged,
        untracked: git.untracked,
        has_conflicts: git.has_conflicts,
        action: if action.is_empty() {
            None
        } else {
            Some(action.to_owned())
        },
    }
}

/// Assemble the full [`PromptJson`] payload for the requested side.
///
/// `left_ansi` and `right_ansi` are the rendered ribbons from
/// [`p10k_rs_core::render_prompt`]; the caller passes them in so this
/// function stays I/O-free and the render only happens once per
/// invocation. `side_is_left` selects which of the two becomes
/// [`PromptJson::ansi_text`] — both ribbons' segment breakdowns ride
/// in [`PromptJson::left`] / [`PromptJson::right`] regardless of
/// which side the shell asked for, because a `--json` consumer
/// typically wants the full picture.
pub(crate) fn build_prompt_json(
    config: &Config,
    ctx: &RenderCtx<'_>,
    left_segments: &[Box<dyn Segment>],
    right_segments: &[Box<dyn Segment>],
    left_ansi: &str,
    right_ansi: &str,
    side_is_left: bool,
) -> PromptJson {
    let _ = config; // reserved for future foreground resolution path
    let produced_at_unix = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let ansi_text = if side_is_left {
        left_ansi.to_owned()
    } else {
        right_ansi.to_owned()
    };
    let plain_text = strip_ansi(&ansi_text);

    PromptJson {
        schema_version: SCHEMA_VERSION,
        produced_at_unix,
        p10k_rs_version: env!("CARGO_PKG_VERSION"),
        shell: shell_label(ctx.shell),
        exit_status: ctx.last_status,
        host: host_label(&ctx.host),
        cwd: ctx.cwd.to_string_lossy().into_owned(),
        ansi_text,
        plain_text,
        left: build_segments_json(left_segments, ctx),
        right: build_segments_json(right_segments, ctx),
        git: ctx.git.map(build_git_json),
    }
}

/// Serialize `payload` to a pretty JSON string with a trailing newline.
///
/// `to_string_pretty` keeps the payload readable on a terminal; the
/// trailing newline matches the convention used by `version --json`
/// and `daemon-health --json` (their `println!` adds the same byte).
pub(crate) fn to_pretty_json(payload: &PromptJson) -> serde_json::Result<String> {
    let mut s = serde_json::to_string_pretty(payload)?;
    s.push('\n');
    Ok(s)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_drops_sgr_escapes() {
        let input = "\x1b[31mred\x1b[39m";
        assert_eq!(strip_ansi(input), "red");
    }

    #[test]
    fn strip_ansi_drops_osc_with_bel_terminator() {
        // OSC 7 cwd report shape.
        let input = "\x1b]7;file:///tmp\x07rest";
        assert_eq!(strip_ansi(input), "rest");
    }

    #[test]
    fn strip_ansi_drops_osc_with_st_terminator() {
        // OSC with ESC \\ string terminator.
        let input = "\x1b]7;file:///tmp\x1b\\rest";
        assert_eq!(strip_ansi(input), "rest");
    }

    #[test]
    fn strip_ansi_preserves_unicode_and_powerline_glyphs() {
        // Powerline arrows and unicode pass through; only escape
        // sequences are dropped.
        let input = "\x1b[34m\u{e0b0}café\u{e0b0}\x1b[39m";
        assert_eq!(strip_ansi(input), "\u{e0b0}café\u{e0b0}");
    }

    #[test]
    fn strip_ansi_handles_no_escapes() {
        assert_eq!(strip_ansi("hello"), "hello");
    }

    #[test]
    fn strip_ansi_drops_zsh_non_printing_markers() {
        // The render path wraps every escape in `%{…%}` for zsh; once
        // the escapes inside are stripped the markers are pure
        // transport noise.
        let input = "%{\x1b[31m%}red%{\x1b[39m%}";
        assert_eq!(strip_ansi(input), "red");
    }

    #[test]
    fn strip_ansi_preserves_literal_percent_outside_marker_pair() {
        // A `%` not followed by `{` or `}` is content and must survive.
        assert_eq!(strip_ansi("50%"), "50%");
        // Doubled `%%` from zsh-side `%`-escaping in `wrap_for_shell`
        // is left intact — the contract is "strip transport noise",
        // not "undo zsh's PROMPT-expansion escaping". A consumer that
        // wants the un-escaped form can post-process.
        assert_eq!(strip_ansi("%%"), "%%");
    }

    #[test]
    fn strip_ansi_handles_truncated_csi() {
        // A truncated CSI at EOF must not panic; the parser drops the
        // unterminated payload.
        let input = "\x1b[31";
        let out = strip_ansi(input);
        // Anything that doesn't panic is acceptable; the bytes are
        // technically garbage already.
        assert!(out.len() <= input.len());
    }

    #[test]
    fn host_label_maps_none_to_json_null() {
        assert_eq!(host_label(&HostKind::None), None);
        assert_eq!(
            host_label(&HostKind::ClaudeCode),
            Some("claude-code".to_owned())
        );
        assert_eq!(host_label(&HostKind::Cursor), Some("cursor".to_owned()));
        assert_eq!(
            host_label(&HostKind::Generic("custom".to_owned())),
            Some("custom".to_owned())
        );
    }

    #[test]
    fn shell_label_pins_cli_strings() {
        assert_eq!(shell_label(Shell::Zsh), "zsh");
        assert_eq!(shell_label(Shell::Bash), "bash");
        assert_eq!(shell_label(Shell::Fish), "fish");
    }
}
