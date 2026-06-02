//! `pixi` — [Pixi] environment segment.
//!
//! Shows `pixi:<name>` in green when `$PIXI_PROJECT_NAME` is set and
//! non-empty. Pixi is Prefix's lockfile-driven, Rust-native alternative
//! to conda; its activation hooks (`pixi shell`, `pixi run`) export
//! `$PIXI_PROJECT_NAME` and `$PIXI_PROJECT_MANIFEST` whenever a project
//! environment is active. We read the env var directly rather than
//! threading it through [`RenderCtx`] — same trade-off as `anaconda.rs`.
//!
//! Project names are usually bare identifiers (`my-project`,
//! `data-pipeline`), but pixi doesn't validate them strongly, so the
//! value is attacker-influenceable the same way `cwd` is. The name
//! passes through [`sanitize_for_terminal`] before landing in `text` to
//! strip CR/LF/ANSI escapes that would otherwise ride into the prompt
//! line.
//!
//! Upstream context: Powerlevel10k issue #2798 — community demand for
//! pixi parity with the conda segment.
//!
//! [Pixi]: https://pixi.sh

use p10k_rs_core::safety::{sanitize_for_terminal, SafeText};
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (mdi-package — reads as "package manager",
/// matching pixi's role). Override via `[segment.pixi].icon = "..."` in
/// the TOML config.
const DEFAULT_ICON: &str = "\u{f487}";

/// Pixi project environment segment.
///
/// Reads `$PIXI_PROJECT_NAME` and emits `pixi:<name>`. Hidden when the
/// var is unset or empty. Project name is sanitised before render to
/// neutralise terminal-control injection.
#[derive(Debug, Default)]
pub struct Pixi;

impl Segment for Pixi {
    fn name(&self) -> &'static str {
        "pixi"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        current_pixi_project().is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call standalone — same pattern as `anaconda.rs`.
        let Some(name) = current_pixi_project() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
                background: None,
            };
        };

        let plain = format!("pixi:{name}");
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

/// Read `$PIXI_PROJECT_NAME` and return the sanitised project name, or
/// `None` when unset / empty / sanitises to empty.
///
/// Kept as a free function so the env-var read is in one place. Tests
/// exercise [`sanitise_project`] directly to avoid mutating process-global
/// env state (`std::env::set_var` is `unsafe` since Rust 1.85, and
/// parallel test threads would race on it anyway).
fn current_pixi_project() -> Option<SafeText> {
    let raw = std::env::var("PIXI_PROJECT_NAME").ok()?;
    sanitise_project(&raw).map(|s| SafeText::from_untrusted(&s))
}

/// Sanitise a candidate pixi project name. Returns `None` for empty
/// input or values that sanitise down to nothing (e.g. a name that was
/// entirely control characters).
///
/// Unlike conda, pixi never exports a path here — `$PIXI_PROJECT_NAME`
/// is always the bare project name from the manifest's `[project]`
/// table. No basename extraction required.
fn sanitise_project(raw: &str) -> Option<String> {
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
    use super::sanitise_project;

    #[test]
    fn sanitise_project_strips_control_chars() {
        // C2 reproducer: a pixi project whose manifest declared a name
        // containing `\r` would otherwise let an attacker overwrite the
        // start of the prompt line on render.
        assert_eq!(sanitise_project("my\rEVIL").as_deref(), Some("myEVIL"));
    }

    #[test]
    fn sanitise_project_empty_is_none() {
        assert_eq!(sanitise_project(""), None);
    }

    #[test]
    fn sanitise_project_passes_normal() {
        assert_eq!(
            sanitise_project("data-pipeline").as_deref(),
            Some("data-pipeline")
        );
    }
}
