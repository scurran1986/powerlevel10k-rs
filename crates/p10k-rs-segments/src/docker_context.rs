//! `docker_context` — Docker current-context segment.
//!
//! Shows `docker:<context>` in cyan when a Docker context other than the
//! implicit `default` (local engine) is active. Mirrors upstream
//! Powerlevel10k's `kubecontext`/`docker_context` semantics — see
//! [P10K #1485](https://github.com/romkatv/powerlevel10k/issues/1485).
//!
//! ## Detection precedence
//!
//! 1. `$DOCKER_CONTEXT` — explicit override, wins unconditionally.
//! 2. `$DOCKER_HOST` — when set without `$DOCKER_CONTEXT`, render the raw
//!    host string (truncated to a reasonable length). This is the
//!    socket-only path: no on-disk Docker context exists, but the user
//!    has aimed their CLI at a non-default endpoint.
//! 3. `~/.docker/config.json` — parse `currentContext` (camelCase, per
//!    Docker CLI). On any parse error or missing field, fall through.
//! 4. Otherwise: hidden.
//!
//! ## `default` filter
//!
//! Matches upstream's `DOCKER_CONTEXT_DEFAULT_FILTER`: when the resolved
//! context is `default` (the implicit local engine) we hide the segment
//! to avoid prompt noise in the common case.
//!
//! ## Sanitisation
//!
//! Context names and `$DOCKER_HOST` values are user-controlled and reach
//! the prompt line, so they pass through [`sanitize_for_terminal`] before
//! render — same pattern as `kubecontext`, `cwd`, and `virtualenv`.

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};
use serde::Deserialize;

/// Default Nerd Font v3 glyph for the `docker_context` segment — the
/// Docker whale (`nf-md-docker`). Overridable via
/// `[segment.docker_context].icon = "..."` in TOML.
const DEFAULT_ICON: &str = "\u{f308}";

/// Hard cap on rendered context/host length. `$DOCKER_HOST` URLs can be
/// arbitrarily long; trim aggressively so the segment stays prompt-sized.
const MAX_RENDER_LEN: usize = 40;

/// Docker current-context segment.
///
/// Resolves the active Docker context (env override, then
/// `~/.docker/config.json`) and emits `docker:<context>`. Hidden when
/// the context is the implicit `default` (local engine) or unresolvable
/// — matches upstream P10K's `DOCKER_CONTEXT_DEFAULT_FILTER`.
#[derive(Debug, Default)]
pub struct DockerContext;

impl Segment for DockerContext {
    fn name(&self) -> &'static str {
        "docker_context"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        current_docker_context().is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same pattern as `kubecontext.rs`.
        let Some(context) = current_docker_context() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
                background: None,
            };
        };

        let plain = format!("docker:{context}");
        let icon = style::resolve_icon(ctx.config, self.name(), None, DEFAULT_ICON);
        let bg = style::render_bg(ctx.config, self.name(), None, Color::Named("cyan".into()));
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("black".into()));
        let text = format!(
            "{bg}{fg}{icon} {plain}{}{}",
            style::reset_fg(),
            style::reset_bg()
        );
        let plain_len = u16::try_from(plain.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: Some(DEFAULT_ICON),
            background: Some(Color::Named("cyan".into())),
        }
    }
}

/// Shape of the relevant slice of `~/.docker/config.json`. Only the
/// `currentContext` field matters here; everything else (auths, plugins,
/// credsStore, …) is intentionally ignored.
#[derive(Debug, Deserialize)]
struct DockerConfig {
    #[serde(rename = "currentContext")]
    current_context: Option<String>,
}

/// Resolve the docker config path: `$HOME/.docker/config.json`. Returns
/// `None` when `$HOME` is unset.
fn config_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        std::path::PathBuf::from(home)
            .join(".docker")
            .join("config.json"),
    )
}

/// Resolve the active Docker context via the documented precedence
/// chain. Returns `None` when the segment should stay hidden — that
/// covers the `default` context, unset/empty values, and unreadable
/// config files.
fn current_docker_context() -> Option<String> {
    if let Some(ctx) = std::env::var("DOCKER_CONTEXT")
        .ok()
        .filter(|s| !s.is_empty())
    {
        return finalise(&ctx);
    }
    if let Some(host) = std::env::var("DOCKER_HOST").ok().filter(|s| !s.is_empty()) {
        return finalise(&host);
    }
    let path = config_path()?;
    let json = std::fs::read_to_string(path).ok()?;
    let parsed = parse_docker_config(&json)?;
    finalise(&parsed)
}

/// Sanitise, truncate, and apply the `default`-filter. Returns `None`
/// when the value is empty, the literal `default`, or sanitises away to
/// nothing.
fn finalise(raw: &str) -> Option<String> {
    let sanitised = sanitize_for_terminal(raw);
    let trimmed = sanitised.trim();
    if trimmed.is_empty() || trimmed == "default" {
        return None;
    }
    Some(truncate_chars(trimmed, MAX_RENDER_LEN))
}

/// Truncate `s` to at most `max` chars (not bytes), appending `…` when
/// truncation actually trims content. Avoids slicing inside a
/// multi-byte codepoint.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let head: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{head}\u{2026}")
}

/// Best-effort parse of `~/.docker/config.json`. Returns the
/// `currentContext` string if present and non-empty, else `None`. Any
/// JSON error collapses to `None` — the segment hides rather than
/// surfacing parse failures on the prompt.
fn parse_docker_config(json: &str) -> Option<String> {
    let cfg: DockerConfig = serde_json::from_str(json).ok()?;
    cfg.current_context.filter(|s| !s.is_empty())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{finalise, parse_docker_config};

    #[test]
    fn parse_docker_config_extracts_current_context() {
        let json = r#"{"currentContext": "myctx", "auths": {}}"#;
        assert_eq!(parse_docker_config(json), Some("myctx".to_owned()));
    }

    #[test]
    fn parse_docker_config_missing_field_is_none() {
        let json = r#"{"auths": {}, "credsStore": "desktop"}"#;
        assert_eq!(parse_docker_config(json), None);
    }

    #[test]
    fn parse_docker_config_default_treated_as_none() {
        // Parse keeps "default"; the `default`-filter lives in `finalise`.
        let json = r#"{"currentContext": "default"}"#;
        assert_eq!(parse_docker_config(json), Some("default".to_owned()));
        // Confirm the full pipeline hides it.
        assert_eq!(finalise("default"), None);
    }

    #[test]
    fn parse_docker_config_empty_string_is_none() {
        let json = r#"{"currentContext": ""}"#;
        assert_eq!(parse_docker_config(json), None);
    }

    #[test]
    fn parse_docker_config_invalid_json_is_none() {
        assert_eq!(parse_docker_config("not json"), None);
    }

    #[test]
    fn sanitise_context_strips_control_chars() {
        // Embedded ESC + bell: sanitiser must scrub these before render.
        let evil = "prod\x1b[31mRED\x07";
        let out = finalise(evil).expect("non-empty after sanitise");
        assert!(!out.contains('\x1b'), "ANSI escape leaked: {out:?}");
        assert!(!out.contains('\x07'), "BEL leaked: {out:?}");
    }

    #[test]
    fn finalise_truncates_long_docker_host() {
        let long = "tcp://very-long-docker-host-name.internal.example.com:2376";
        let out = finalise(long).expect("non-empty");
        assert!(
            out.chars().count() <= 40,
            "expected <=40 chars, got {}: {out:?}",
            out.chars().count()
        );
        assert!(out.ends_with('\u{2026}'), "expected ellipsis: {out:?}");
    }

    #[test]
    fn finalise_empty_is_none() {
        assert_eq!(finalise(""), None);
        assert_eq!(finalise("   "), None);
    }
}
