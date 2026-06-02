//! `nodenv` — Node.js version-manager segment.
//!
//! Shows `node:<version>` in green when `$NODENV_VERSION` is set and
//! non-empty. `nodenv` exports this variable when a shim resolves to a
//! specific version (per-directory `.node-version`, `nodenv shell`, or a
//! global default), so we read it directly rather than shelling out to
//! `nodenv version-name` on every prompt — that would re-introduce the
//! latency tax this project exists to avoid.
//!
//! The value is attacker-controlled in the same sense `cwd` is — a
//! `.node-version` file containing a CR byte would otherwise let an
//! attacker overwrite the prompt line. Sanitisation happens before the
//! value lands in `text` (see `dir.rs` and `virtualenv.rs` for the same
//! pattern).

use p10k_rs_core::safety::{sanitize_for_terminal, SafeText};
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (mdi-nodejs). Override via
/// `[segment.nodenv].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{e718}";

/// `nodenv` Node.js version segment.
///
/// Reads `$NODENV_VERSION` and emits `node:<version>`. Hidden when the
/// var is unset or empty.
#[derive(Debug, Default)]
pub struct Nodenv;

impl Segment for Nodenv {
    fn name(&self) -> &'static str {
        "nodenv"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        current_nodenv_version().is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same pattern as `virtualenv.rs` and
        // `vcs.rs`.
        let Some(version) = current_nodenv_version() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
                background: None,
            };
        };

        let plain = format!("node:{version}");
        let icon = style::resolve_icon(ctx.config, self.name(), None, DEFAULT_ICON);
        let bg = style::render_bg(ctx.config, self.name(), None, Color::Named("green".into()));
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("black".into()));
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
            background: Some(style::resolve_bg(
                ctx.config,
                self.name(),
                None,
                Color::Named("green".into()),
            )),
        }
    }
}

/// Read `$NODENV_VERSION` and return the sanitised version string, or
/// `None` when unset / empty / sanitised-to-empty.
///
/// Kept as a free function so the env-var read is in one place. Tests
/// exercise [`sanitise_version`] directly to avoid mutating
/// process-global env state (`std::env::set_var` is `unsafe` since Rust
/// 1.85, and parallel test threads would race on it anyway).
fn current_nodenv_version() -> Option<SafeText> {
    let raw = std::env::var("NODENV_VERSION").ok()?;
    sanitise_version(&raw).map(|s| SafeText::from_untrusted(&s))
}

/// Sanitise a raw `$NODENV_VERSION` value for terminal output.
///
/// Returns `None` for empty input or input that sanitises to the empty
/// string (e.g. a value consisting entirely of control characters).
/// Otherwise returns the cleaned version string — `20.10.0`, `system`,
/// `lts/iron`, etc. all pass through unchanged.
fn sanitise_version(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let clean = sanitize_for_terminal(raw).into_owned();
    if clean.is_empty() {
        None
    } else {
        Some(clean)
    }
}

#[cfg(test)]
mod tests {
    use super::sanitise_version;
    use p10k_rs_core::safety::SafeText;

    #[test]
    fn sanitise_version_strips_control_chars() {
        // A `.node-version` file with a trailing CR would otherwise let
        // an attacker overwrite the start of the prompt line.
        assert_eq!(sanitise_version("20.10.0\r"), Some("20.10.0".to_owned()));
    }

    #[test]
    fn sanitise_version_empty_is_none() {
        assert_eq!(sanitise_version(""), None);
    }

    #[test]
    fn sanitise_version_passes_normal() {
        assert_eq!(sanitise_version("20.10.0"), Some("20.10.0".to_owned()));
    }

    #[test]
    fn sanitise_version_passes_lts_alias() {
        assert_eq!(sanitise_version("lts/iron"), Some("lts/iron".to_owned()));
    }

    #[test]
    fn renders_with_default_icon() {
        // Env-var driven: skip when unset so parallel test threads don't race
        // on `set_var` (unsafe since Rust 1.85). Contributors running
        // `NODENV_VERSION=20.10.0 cargo test` still exercise the icon path.
        use std::path::Path;
        use std::time::{Duration, SystemTime};

        use p10k_rs_core::style::Color;
        use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

        use super::Nodenv;

        if std::env::var("NODENV_VERSION")
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
            sync_output: false,
        };
        let out = Nodenv.render(&ctx);
        assert!(
            out.text.contains('\u{e718}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{e718}"));
        assert_eq!(out.background, Some(Color::Named("green".into())));
        assert!(
            out.text.contains("\x1b[42m") || out.text.contains("48;"),
            "bg SGR missing: {:?}",
            out.text
        );
    }
}
