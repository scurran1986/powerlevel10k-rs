//! `dir` — current working directory segment.
//!
//! Cwd painted black-on-blue (P10K-classic palette), with `$HOME` collapsed
//! to `~`. Truncation policies and the writable / read-only state will land
//! later. ANSI escapes are emitted raw; the renderer post-processes them for
//! the target shell (e.g. zsh's `%{…%}` bracketing) and uses the declared
//! `background` colour to paint powerline transition arrows.

use std::path::Path;

use p10k_rs_config::{DirTruncate, DirTruncateStrategy};
use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

const DEFAULT_ICON: &str = "\u{f07b}"; // Nerd Font v3: folder (FA-style)
const NOT_WRITABLE_ICON: &str = "\u{f023}"; // Nerd Font v3: padlock

/// Ellipsis glyph used as the truncation marker.
///
/// `U+2026` (HORIZONTAL ELLIPSIS) — a single visual column under
/// East-Asian-wide-aware terminals; passes [`sanitize_for_terminal`]
/// (not a control codepoint).
const ELLIPSIS: &str = "\u{2026}";

/// Current-directory segment.
///
/// Reads [`RenderCtx::cwd`] and emits its display string with the user's home
/// directory abbreviated to `~`.
#[derive(Debug, Default)]
pub struct Dir;

impl Segment for Dir {
    fn name(&self) -> &'static str {
        "dir"
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Sanitise before home-collapse so a malicious cwd containing
        // control bytes can't ride the unfiltered path into PROMPT (C2).
        let raw = sanitize_for_terminal(&ctx.cwd.display().to_string());
        let home = std::env::var("HOME").ok();
        let collapsed = home_collapse(&raw, home.as_deref());
        let truncate = ctx
            .config
            .segments
            .get(self.name())
            .map(|s| s.truncate.clone())
            .unwrap_or_default();
        let collapsed = truncate_path(&collapsed, &truncate);
        // Probe write permission on the *real* cwd path before we paint.
        // On any errno (broken cwd, EACCES on the parent, etc.) we treat
        // the directory as not writable — that's the safer default for a
        // visual cue (matches P10K's behaviour: when in doubt, show the
        // padlock). State-keyed TOML overrides flow through the
        // `style::*` helpers below.
        let state_tag = writability_state(ctx.cwd);
        let (default_bg, default_fg) = default_palette_for(state_tag);
        let default_icon = if state_tag == "not_writable" {
            NOT_WRITABLE_ICON
        } else {
            DEFAULT_ICON
        };
        let icon = style::resolve_icon(ctx.config, self.name(), Some(state_tag), default_icon);
        let bg = style::render_bg(ctx.config, self.name(), Some(state_tag), default_bg.clone());
        let fg = style::render_fg(ctx.config, self.name(), Some(state_tag), default_fg);
        let text = format!(
            "{bg}{fg}{icon} {collapsed}{}{}",
            style::reset_fg(),
            style::reset_bg()
        );
        // plain_len: chars-of-collapsed + 1 (icon, single Nerd Font codepoint
        // renders as 1 visual col) + 1 (space). saturating_add guards the
        // u16::MAX overflow path.
        let plain_len = u16::try_from(collapsed.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        SegmentOutput {
            text,
            plain_len,
            state: Some(state_tag),
            icon: Some(default_icon),
            background: Some(default_bg),
        }
    }
}

/// Per-state default `(background, foreground)` pair.
///
/// - `writable` — blue/black (P10K-classic default, unchanged from slice
///   28A).
/// - `not_writable` — yellow/black (P10K's `DIR_NOT_WRITABLE_*` "warning"
///   hue).
///
/// Pulled out as a free function so we can pick the default *before*
/// calling [`style::render_bg`] / [`style::render_fg`] — those helpers
/// take a single default and don't know about state defaults. Mirrors
/// the `vi_mode.rs` slice 34 pattern.
fn default_palette_for(state: &str) -> (Color, Color) {
    match state {
        "not_writable" => (Color::Named("yellow".into()), Color::Named("black".into())),
        // `writable` and any future variant fall back to the
        // P10K-classic blue/black.
        _ => (Color::Named("blue".into()), Color::Named("black".into())),
    }
}

/// Probe `cwd` for write permission and return the state tag.
///
/// Uses `rustix::fs::access(cwd, Access::WRITE_OK)` — the POSIX `access(2)`
/// shim — which performs the same real-UID/GID check the kernel would
/// apply to a subsequent `open(O_WRONLY)`. We don't actually open the
/// directory; this is a cheap permission probe that doesn't perturb
/// atime.
///
/// Fallback: on *any* errno (broken cwd that no longer exists, EACCES on
/// a parent, ENOTDIR if the cwd was racily replaced with a file, etc.)
/// we report `"not_writable"`. That's the safer visual default — a
/// padlock is a strictly better warning than a silently-green prompt on
/// a directory that's actually broken.
fn writability_state(cwd: &Path) -> &'static str {
    match rustix::fs::access(cwd, rustix::fs::Access::WRITE_OK) {
        Ok(()) => "writable",
        Err(_) => "not_writable",
    }
}

/// Apply the configured truncation strategy to a (home-collapsed) path string.
///
/// Pure function over the already-sanitised `path` and the parsed
/// [`DirTruncate`] config. Operates on the textual form so a leading `~`
/// counts as the first component (matches upstream P10K behaviour).
///
/// Algorithm:
/// - Splits on `/` and keeps the empty leading element (for absolute paths
///   like `/a/b/c` → `["", "a", "b", "c"]`) so the leading slash is preserved
///   by re-joining.
/// - `length = 0` is normalised to `1` so a misconfiguration can't render an
///   empty cwd.
/// - Paths with fewer non-empty components than `length` are returned
///   unchanged (no marker needed — nothing was elided).
///
/// Returns the input unchanged when `strategy == None`.
fn truncate_path(path: &str, cfg: &DirTruncate) -> String {
    if matches!(cfg.strategy, DirTruncateStrategy::None) {
        return path.to_owned();
    }
    let length = cfg.length.max(1) as usize;
    // Split into the leading-slash marker (if any) plus the components.
    let (leading, body) = if let Some(rest) = path.strip_prefix('/') {
        ("/", rest)
    } else {
        ("", path)
    };
    let parts: Vec<&str> = body.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() <= length {
        return path.to_owned();
    }
    match cfg.strategy {
        DirTruncateStrategy::None => path.to_owned(),
        DirTruncateStrategy::ToLast => {
            // …/<last `length` components>. The marker stands in for both
            // the elided components and any leading slash.
            let tail = &parts[parts.len() - length..];
            format!("{ELLIPSIS}/{}", tail.join("/"))
        }
        DirTruncateStrategy::Middle => {
            // <first>/…/<last `length - 1` components>. When `length == 1`
            // there's no tail; degenerates to `<first>/…`.
            let first = parts[0];
            if length == 1 {
                format!("{leading}{first}/{ELLIPSIS}")
            } else {
                let tail = &parts[parts.len() - (length - 1)..];
                format!("{leading}{first}/{ELLIPSIS}/{}", tail.join("/"))
            }
        }
        // `DirTruncateStrategy` is `#[non_exhaustive]` so future variants
        // (e.g. `truncate_to_unique`) compile without breaking this match.
        // Unknown strategy falls back to the safe no-op path.
        _ => path.to_owned(),
    }
}

/// Collapse a leading `home` directory in `path` to `~`. Returns the input
/// unchanged if `home` is `None` or doesn't prefix the path.
///
/// `home` is taken explicitly so this is a pure function we can unit-test
/// without mutating process-global env state — `std::env::set_var` is
/// `unsafe` since Rust 1.85 and this crate forbids unsafe blocks anyway.
fn home_collapse(path: &str, home: Option<&str>) -> String {
    let Some(home) = home else {
        return path.to_owned();
    };
    if let Some(rest) = path.strip_prefix(home) {
        // Only collapse on a directory boundary: `/home/sean/x` → `~/x`,
        // not `/home/seanson/x` → `~son/x`.
        if rest.is_empty() || rest.starts_with('/') {
            return format!("~{rest}");
        }
    }
    path.to_owned()
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::*;

    #[test]
    fn home_collapse_exact() {
        let home = Some("/home/sean");
        assert_eq!(home_collapse("/home/sean", home), "~");
        assert_eq!(home_collapse("/home/sean/code", home), "~/code");
        assert_eq!(
            home_collapse("/home/seanson/code", home),
            "/home/seanson/code"
        );
        assert_eq!(home_collapse("/etc/passwd", home), "/etc/passwd");
        assert_eq!(home_collapse("/etc/passwd", None), "/etc/passwd");
    }

    /// Construct a [`DirTruncate`] config with the given strategy / length.
    fn trunc(strategy: DirTruncateStrategy, length: u8) -> DirTruncate {
        DirTruncate { strategy, length }
    }

    #[test]
    fn truncate_to_last_keeps_only_n_components() {
        let cfg = trunc(DirTruncateStrategy::ToLast, 2);
        assert_eq!(truncate_path("/a/b/c/d/e", &cfg), "\u{2026}/d/e");
    }

    #[test]
    fn truncate_middle_keeps_first_and_last_n() {
        let cfg = trunc(DirTruncateStrategy::Middle, 2);
        assert_eq!(truncate_path("/a/b/c/d/e", &cfg), "/a/\u{2026}/e");
        let cfg3 = trunc(DirTruncateStrategy::Middle, 3);
        assert_eq!(truncate_path("/a/b/c/d/e", &cfg3), "/a/\u{2026}/d/e");
    }

    #[test]
    fn truncate_none_passes_through() {
        let cfg = trunc(DirTruncateStrategy::None, 2);
        assert_eq!(truncate_path("/a/b/c/d/e", &cfg), "/a/b/c/d/e");
    }

    #[test]
    fn truncate_short_path_no_op() {
        // Component count (2: "a", "b") <= length (3) — return unchanged.
        let cfg = trunc(DirTruncateStrategy::ToLast, 3);
        assert_eq!(truncate_path("/a/b", &cfg), "/a/b");
        let mid = trunc(DirTruncateStrategy::Middle, 3);
        assert_eq!(truncate_path("/a/b", &mid), "/a/b");
    }

    #[test]
    fn truncate_with_home_collapse() {
        // The truncator runs after `home_collapse`, so `~` is the first
        // component. With ToLast/length=2 only the last two survive.
        let collapsed = home_collapse("/home/sean/proj/sub/deep", Some("/home/sean"));
        assert_eq!(collapsed, "~/proj/sub/deep");
        let cfg = trunc(DirTruncateStrategy::ToLast, 2);
        assert_eq!(truncate_path(&collapsed, &cfg), "\u{2026}/sub/deep");
    }

    #[test]
    fn truncate_length_zero_normalises_to_one() {
        // A misconfigured `length = 0` must not render an empty cwd.
        let cfg = trunc(DirTruncateStrategy::ToLast, 0);
        assert_eq!(truncate_path("/a/b/c", &cfg), "\u{2026}/c");
    }

    fn ctx<'a>(cfg: &'a Config, env: &'a EnvSnapshot, cwd: &'a Path) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd,
            git: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
        }
    }

    /// Path the platform guarantees is writable for the running process.
    /// Used by tests that need a known-writable cwd so the new slice-39
    /// `not_writable` probe can't trip them. `std::env::temp_dir()`
    /// returns `$TMPDIR` (Unix) or `%TEMP%` (Windows) and is created with
    /// the running user as owner — so `access(W_OK)` will succeed.
    fn writable_scratch() -> std::path::PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn renders_with_default_folder_icon() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let path = writable_scratch();
        let out = Dir.render(&ctx(&cfg, &env, &path));
        assert!(
            out.text.contains('\u{f07b}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{f07b}"));
        // Slice 28A: blue background SGR present and declared on the output
        // so the renderer can paint matching powerline arrows.
        assert!(
            out.text.contains("\x1b[48;5;4m"),
            "blue bg SGR missing: {:?}",
            out.text
        );
        assert_eq!(out.background, Some(Color::Named("blue".into())));
    }

    #[test]
    fn cwd_with_carriage_return_is_stripped() {
        // C2 reproducer: a directory whose name contains `\r` would
        // otherwise let an attacker overwrite the start of the prompt
        // line on render.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let path = Path::new("/tmp/start\rEVIL");
        let out = Dir.render(&ctx(&cfg, &env, path));
        assert!(!out.text.contains('\r'), "CR survived: {:?}", out.text);
        assert!(out.text.contains("/tmp/startEVIL"));
    }

    #[test]
    fn cwd_with_osc_escape_is_stripped() {
        // C2 reproducer: a directory whose name contains an OSC 0
        // sequence would relabel the user's terminal tab. The segment
        // legitimately emits its own SGR escapes (`\x1b[34m`, `\x1b[39m`)
        // for the blue colour — what must be gone is the attacker's OSC
        // introducer (`\x1b]`) and the `\x07` BEL terminator.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let path = Path::new("/tmp/main\x1b]0;TARS-OWNED\x07");
        let out = Dir.render(&ctx(&cfg, &env, path));
        assert!(
            !out.text.contains("\x1b]"),
            "OSC introducer survived: {:?}",
            out.text
        );
        assert!(!out.text.contains('\x07'), "BEL survived: {:?}", out.text);
        // Visible payload is preserved (no escapes around it now).
        assert!(out.text.contains("/tmp/main]0;TARS-OWNED"));
    }

    #[test]
    fn writable_state_keeps_blue() {
        // The scratch dir is owner-writable; the writability probe must
        // return `Ok(())` and we should land on the P10K-classic blue
        // palette plus the writable state tag.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let path = writable_scratch();
        let out = Dir.render(&ctx(&cfg, &env, &path));
        assert_eq!(out.state, Some("writable"));
        assert_eq!(out.icon, Some("\u{f07b}"));
        assert!(
            out.text.contains("\x1b[48;5;4m"),
            "blue bg SGR missing: {:?}",
            out.text
        );
        assert_eq!(out.background, Some(Color::Named("blue".into())));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn not_writable_state_shifts_palette() {
        // `/proc/1` exists for every Linux process tree and is *never*
        // writable by an unprivileged user (root-owned, mode 555 on the
        // pid dir). access(W_OK) returns EACCES → the segment lands on
        // the not_writable state and the yellow warning palette.
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let path = Path::new("/proc/1");
        let out = Dir.render(&ctx(&cfg, &env, path));
        assert_eq!(out.state, Some("not_writable"));
        // Padlock glyph swaps in for the folder default.
        assert_eq!(out.icon, Some("\u{f023}"));
        assert!(
            out.text.contains('\u{f023}'),
            "padlock icon missing: {:?}",
            out.text
        );
        // Yellow bg (`48;5;3` in Ansi256) — the P10K warning hue.
        assert!(
            out.text.contains("\x1b[48;5;3m"),
            "yellow bg SGR missing: {:?}",
            out.text
        );
        assert_eq!(out.background, Some(Color::Named("yellow".into())));
    }
}
