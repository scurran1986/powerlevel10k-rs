//! `anaconda` — Conda environment segment.
//!
//! Shows `conda:<name>` in green when `$CONDA_DEFAULT_ENV` is set and
//! non-empty. Conda's activation hooks export this variable whenever an
//! environment is active; we read it directly rather than threading it
//! through [`RenderCtx`] because it's a hot-path lookup that doesn't
//! warrant a new snapshot field for one segment.
//!
//! Conda env names are usually bare strings (`base`, `myproject`), but
//! when an environment is created with `conda create -p /some/path`,
//! `$CONDA_DEFAULT_ENV` carries the full path instead. In either case
//! the value is attacker-influenced in the same way `cwd` is — an env
//! at `/tmp/evil\rOVERWRITE/` would otherwise let a CR ride into the
//! prompt line. Sanitisation happens before the value lands in `text`
//! (see `dir.rs` for the same pattern).

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (python alt; matches conda's snake branding).
/// Override via `[segment.anaconda].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{e73c}";

/// Conda environment segment.
///
/// Reads `$CONDA_DEFAULT_ENV` and emits `conda:<name>`, where `<name>`
/// is either the bare env name or the basename of the env directory
/// when conda exported a full path. Hidden when the var is unset or
/// empty.
#[derive(Debug, Default)]
pub struct Anaconda;

impl Segment for Anaconda {
    fn name(&self) -> &'static str {
        "anaconda"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        current_conda_env().is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same pattern as `virtualenv.rs` does
        // for a missing env var.
        let Some(name) = current_conda_env() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
                background: None,
            };
        };

        let plain = format!("conda:{name}");
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
            background: Some(Color::Named("green".into())),
        }
    }
}

/// Read `$CONDA_DEFAULT_ENV` and return the sanitised env name, or
/// `None` when unset / empty / basename-less.
///
/// Kept as a free function so the env-var read is in one place. Tests
/// exercise [`extract_env_name`] directly to avoid mutating
/// process-global env state (`std::env::set_var` is `unsafe` since Rust
/// 1.85, and parallel test threads would race on it anyway).
fn current_conda_env() -> Option<String> {
    let raw = std::env::var("CONDA_DEFAULT_ENV").ok()?;
    if raw.is_empty() {
        return None;
    }
    let basename = extract_env_name(&raw);
    if basename.is_empty() {
        None
    } else {
        Some(basename)
    }
}

/// Conda env names are usually bare strings (`base`, `myproject`), but
/// can be absolute paths when created with `conda create -p /path/to/env`.
/// Take the basename in either case, then sanitise for terminal output.
///
/// Returns an empty string when `raw` has no usable basename (e.g. when
/// it's a path-shaped value with non-UTF-8 components).
fn extract_env_name(raw: &str) -> String {
    let name = if raw.contains('/') {
        std::path::Path::new(raw)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
    } else {
        raw
    };
    sanitize_for_terminal(name)
}

#[cfg(test)]
mod tests {
    use super::extract_env_name;

    #[test]
    fn extract_bare_name() {
        assert_eq!(extract_env_name("base"), "base");
    }

    #[test]
    fn extract_path_basename() {
        assert_eq!(extract_env_name("/home/x/conda/myproj"), "myproj");
    }

    #[test]
    fn extract_strips_trailing_slash() {
        assert_eq!(extract_env_name("/home/x/conda/myproj/"), "myproj");
    }

    #[test]
    fn extract_strips_control_chars() {
        // Bare-name path: no slash, so we never hit `Path::file_name`;
        // sanitisation still has to scrub the CR before it lands in the
        // prompt line.
        assert_eq!(extract_env_name("dev\rEVIL"), "devEVIL");
    }

    #[test]
    fn extract_path_with_control_chars() {
        // C2 reproducer: a conda env at a path whose basename contains
        // `\r` would otherwise let an attacker overwrite the start of
        // the prompt line on render.
        assert_eq!(extract_env_name("/home/x/conda/dev\rEVIL"), "devEVIL");
    }

    #[test]
    fn extract_empty_is_empty() {
        assert_eq!(extract_env_name(""), "");
    }

    #[test]
    fn renders_with_default_icon() {
        // Env-var driven: skip when unset so parallel test threads don't race
        // on `set_var` (unsafe since Rust 1.85). Contributors running
        // `CONDA_DEFAULT_ENV=base cargo test` still exercise the icon path.
        use std::path::Path;
        use std::time::{Duration, SystemTime};

        use p10k_rs_core::style::Color;
        use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

        use super::Anaconda;

        if std::env::var("CONDA_DEFAULT_ENV")
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
            jj: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env: &env,
            upcoming_command: "",
        };
        let out = Anaconda.render(&ctx);
        assert!(
            out.text.contains('\u{e73c}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{e73c}"));
        assert_eq!(out.background, Some(Color::Named("green".into())));
        assert!(
            out.text.contains("\x1b[42m") || out.text.contains("48;"),
            "bg SGR missing: {:?}",
            out.text
        );
    }
}
