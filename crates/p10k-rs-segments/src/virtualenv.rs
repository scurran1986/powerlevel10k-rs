//! `virtualenv` — Python virtual-environment segment.
//!
//! Shows `(<basename>)` in yellow when `$VIRTUAL_ENV` is set and non-empty.
//! Python's `venv` activator and `pyenv-virtualenv` both export this var on
//! activation; we read it directly rather than threading it through
//! [`RenderCtx`] because it's a hot-path lookup that doesn't warrant a new
//! snapshot field for one segment.
//!
//! The basename comes from the filesystem and is attacker-controlled in the
//! same way `cwd` is — a venv at `/tmp/evil\rOVERWRITE/` would otherwise let
//! a CR ride into the prompt line. Sanitisation happens before the value
//! lands in `text` (see `dir.rs` for the same pattern).

use std::path::Path;

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (python; matches pyenv). Override via
/// `[segment.virtualenv].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{e235}";

/// Python virtualenv segment.
///
/// Reads `$VIRTUAL_ENV` and emits the basename of the activated environment
/// wrapped in parentheses. Hidden when the var is unset or empty.
#[derive(Debug, Default)]
pub struct Virtualenv;

impl Segment for Virtualenv {
    fn name(&self) -> &'static str {
        "virtualenv"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        current_virtualenv().is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same pattern as `vcs.rs` does for a
        // missing `ctx.git`.
        let Some(basename) = current_virtualenv() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
            };
        };

        let plain = format!("({basename})");
        let icon = style::resolve_icon(ctx.config, self.name(), None, DEFAULT_ICON);
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("yellow".into()));
        let text = format!("{fg}{icon} {plain}{}", style::reset_fg());
        let plain_len = u16::try_from(plain.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2); // icon + space
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: Some(DEFAULT_ICON),
        }
    }
}

/// Read `$VIRTUAL_ENV` and return the sanitised basename, or `None` when
/// unset / empty / basename-less.
///
/// Kept as a free function so the env-var read is in one place. Tests
/// exercise [`extract_basename`] directly to avoid mutating process-global
/// env state (`std::env::set_var` is `unsafe` since Rust 1.85, and parallel
/// test threads would race on it anyway).
fn current_virtualenv() -> Option<String> {
    let raw = std::env::var("VIRTUAL_ENV").ok()?;
    if raw.is_empty() {
        return None;
    }
    let basename = extract_basename(&raw);
    if basename.is_empty() {
        None
    } else {
        Some(basename)
    }
}

/// Pull the trailing path component out of `path` and sanitise it for
/// terminal output. Returns an empty string when `path` has no usable
/// basename (empty input, root, etc.).
///
/// Handles trailing slashes by deferring to [`Path::file_name`], which
/// already strips them. Non-UTF-8 components are dropped.
fn extract_basename(path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    sanitize_for_terminal(name)
}

#[cfg(test)]
mod tests {
    use super::extract_basename;

    #[test]
    fn basic_unix_path() {
        assert_eq!(extract_basename("/home/x/.venv"), ".venv");
    }

    #[test]
    fn trailing_slash_is_stripped() {
        assert_eq!(extract_basename("/home/x/projects/.venv/"), ".venv");
    }

    #[test]
    fn control_chars_are_sanitised() {
        // C2 reproducer: a venv directory whose name contains `\r` would
        // otherwise let an attacker overwrite the start of the prompt
        // line on render.
        assert_eq!(
            extract_basename("/home/x/proj/control\rEVIL"),
            "controlEVIL"
        );
    }

    #[test]
    fn empty_input() {
        assert_eq!(extract_basename(""), "");
    }

    #[test]
    fn renders_with_default_icon() {
        // Env-var driven: skip when unset so parallel test threads don't race
        // on `set_var` (unsafe since Rust 1.85). Contributors running
        // `VIRTUAL_ENV=/tmp/.venv cargo test` still exercise the icon path.
        use std::path::Path;
        use std::time::{Duration, SystemTime};

        use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

        use super::Virtualenv;

        if std::env::var("VIRTUAL_ENV")
            .map(|v| v.is_empty())
            .unwrap_or(true)
        {
            // Segment hidden when env var absent — covered by other unit tests.
            return;
        }

        let cfg = Config::default();
        let env = EnvSnapshot::default();
        let cwd = Path::new("/tmp/example");
        let ctx = RenderCtx {
            config: &cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd,
            git: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env: &env,
        };
        let out = Virtualenv.render(&ctx);
        assert!(
            out.text.contains('\u{e235}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{e235}"));
    }
}
