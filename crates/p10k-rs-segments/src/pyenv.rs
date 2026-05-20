//! `pyenv` — active pyenv Python version segment.
//!
//! Shows `py:<version>` in yellow when `$PYENV_VERSION` is set and non-empty.
//! `pyenv` exports this variable when a shim resolves a version (shell, local,
//! or global); we read it directly rather than shelling out to `pyenv version`
//! because the env var is the same source pyenv's own prompt integration uses
//! and avoids a fork on every prompt.
//!
//! The version string is attacker-controlled in the same sense as a venv path:
//! `$PYENV_VERSION` could in principle contain control bytes (a malicious
//! `.python-version` file, or a hostile parent process). Sanitisation happens
//! before the value lands in `text` (see `virtualenv.rs` and `dir.rs` for the
//! same pattern).

use p10k_rs_core::safety::{sanitize_for_terminal, SafeText};
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (font-awesome python). Override via
/// `[segment.pyenv].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{e235}";

/// pyenv segment.
///
/// Reads `$PYENV_VERSION` and emits `py:<version>` (e.g. `py:3.12.1`,
/// `py:system`, `py:2.7.18/envs/legacy`). Hidden when the var is unset or
/// empty.
#[derive(Debug, Default)]
pub struct Pyenv;

impl Segment for Pyenv {
    fn name(&self) -> &'static str {
        "pyenv"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        current_pyenv_version().is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same pattern as `virtualenv.rs`.
        let Some(version) = current_pyenv_version() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
                background: None,
            };
        };

        let plain = format!("py:{version}");
        let icon = style::resolve_icon(ctx.config, self.name(), None, DEFAULT_ICON);
        let bg = style::render_bg(ctx.config, self.name(), None, Color::Named("blue".into()));
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("white".into()));
        let text = format!(
            "{bg}{fg}{icon} {plain}{}{}",
            style::reset_fg(),
            style::reset_bg()
        );
        let plain_len = u16::try_from(plain.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2); // icon + space
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: Some(DEFAULT_ICON),
            background: Some(Color::Named("blue".into())),
        }
    }
}

/// Read `$PYENV_VERSION` and return the sanitised version string, or `None`
/// when unset / empty / scrubbed-to-empty.
///
/// Kept as a free function so the env-var read is in one place. Tests
/// exercise [`sanitise_version`] directly to avoid mutating process-global
/// env state (`std::env::set_var` is `unsafe` since Rust 1.85, and parallel
/// test threads would race on it anyway).
fn current_pyenv_version() -> Option<SafeText> {
    let raw = std::env::var("PYENV_VERSION").ok()?;
    sanitise_version(&raw).map(|s| SafeText::from_untrusted(&s))
}

/// Sanitise a raw `$PYENV_VERSION` value for terminal output.
///
/// Returns `None` for an empty input, or when the sanitiser strips the input
/// down to nothing. Otherwise returns the cleaned version string suitable for
/// landing in the prompt.
fn sanitise_version(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let clean = sanitize_for_terminal(raw);
    if clean.is_empty() {
        None
    } else {
        Some(clean.into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::sanitise_version;
    use p10k_rs_core::safety::SafeText;

    #[test]
    fn sanitise_version_strips_control_chars() {
        assert_eq!(sanitise_version("3.12.1\r"), Some("3.12.1".to_owned()));
    }

    #[test]
    fn sanitise_version_empty_is_none() {
        assert_eq!(sanitise_version(""), None);
    }

    #[test]
    fn sanitise_version_passes_normal() {
        assert_eq!(sanitise_version("3.12.1"), Some("3.12.1".to_owned()));
    }

    #[test]
    fn sanitise_version_passes_system_keyword() {
        assert_eq!(sanitise_version("system"), Some("system".to_owned()));
    }

    #[test]
    fn renders_with_default_icon() {
        // Env-var driven: skip when unset so parallel test threads don't race
        // on `set_var` (unsafe since Rust 1.85). Contributors running
        // `PYENV_VERSION=3.12 cargo test` still exercise the icon path.
        use std::path::Path;
        use std::time::{Duration, SystemTime};

        use p10k_rs_core::style::Color;
        use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

        use super::Pyenv;

        if std::env::var("PYENV_VERSION")
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
            cwd_display: SafeText::default(),
            git: None,
            jj: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env: &env,
            upcoming_command: "",
            shell_integration_active: false,
        };
        let out = Pyenv.render(&ctx);
        assert!(
            out.text.contains('\u{e235}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{e235}"));
        assert_eq!(out.background, Some(Color::Named("blue".into())));
        assert!(
            out.text.contains("\x1b[44m") || out.text.contains("48;"),
            "bg SGR missing: {:?}",
            out.text
        );
    }
}
