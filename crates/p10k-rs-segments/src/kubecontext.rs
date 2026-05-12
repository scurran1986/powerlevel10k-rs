//! `kubecontext` — Kubernetes current-context segment.
//!
//! Shows `k8s:<context>` in cyan when a kubeconfig file is readable and
//! contains a `current-context:` entry. Honours `$KUBECONFIG` when set;
//! otherwise falls back to `~/.kube/config`.
//!
//! ## Parsing caveat
//!
//! We hand-parse a single line out of the YAML rather than pull in a YAML
//! crate dep — the rest of `kubeconfig` doesn't concern us. This is the
//! 95%-correct approach: it won't handle anchors, multi-line scalars, or
//! the (unlikely) case of `current-context:` appearing inside a string
//! literal elsewhere in the file. A future slice can swap in `serde_yml`
//! if a real-world config breaks us.
//!
//! ## `$KUBECONFIG` handling
//!
//! Real kubectl merges every colon-separated path in `$KUBECONFIG`. For
//! slice 17 we only read the *first* path — multi-file merging is out of
//! scope. Most setups in practice are a single file anyway.
//!
//! ## Sanitisation
//!
//! Context names are user-controlled (anyone with a kubeconfig writes
//! them) and end up on the prompt line, so they pass through
//! [`sanitize_for_terminal`] before render — same pattern as `cwd` and
//! `virtualenv`.

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph for the kubecontext segment — the
/// Kubernetes wheel (`nf-md-kubernetes`). Overridable via
/// `[segment.kubecontext].icon = "..."` in TOML.
const DEFAULT_ICON: &str = "\u{f10fe}";

/// Kubernetes current-context segment.
///
/// Reads the active kubeconfig (`$KUBECONFIG` first path, else
/// `~/.kube/config`), extracts `current-context:`, and emits
/// `k8s:<context>`. Hidden when no kubeconfig is readable or no
/// `current-context` line is present.
#[derive(Debug, Default)]
pub struct Kubecontext;

impl Segment for Kubecontext {
    fn name(&self) -> &'static str {
        "kubecontext"
    }

    fn enabled(&self, _ctx: &RenderCtx<'_>) -> bool {
        current_kubecontext().is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same pattern as `virtualenv.rs`.
        let Some(context) = current_kubecontext() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
            };
        };

        let plain = format!("k8s:{context}");
        let icon = ctx
            .config
            .segments
            .get(self.name())
            .and_then(|sc| sc.icon.as_deref())
            .unwrap_or(DEFAULT_ICON);
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("cyan".into()));
        let text = format!("{fg}{icon} {plain}{}", style::reset_fg());
        let plain_len = u16::try_from(plain.chars().count())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: Some(DEFAULT_ICON),
        }
    }
}

/// Resolve the kubeconfig path: first entry of `$KUBECONFIG` if set and
/// non-empty, else `$HOME/.kube/config`. Returns `None` when neither is
/// available.
fn config_path() -> Option<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("KUBECONFIG") {
        let s = path.to_string_lossy();
        // Take only the first path if colon-separated.
        let first = s.split(':').next().unwrap_or("");
        if !first.is_empty() {
            return Some(std::path::PathBuf::from(first));
        }
    }
    let home = std::env::var_os("HOME")?;
    Some(std::path::PathBuf::from(home).join(".kube").join("config"))
}

/// Read the resolved kubeconfig and return the sanitised current-context,
/// or `None` if the file is missing/unreadable or has no usable
/// `current-context` line.
fn current_kubecontext() -> Option<String> {
    let path = config_path()?;
    let yaml = std::fs::read_to_string(path).ok()?;
    parse_current_context(&yaml)
        .map(|s| sanitize_for_terminal(&s))
        .filter(|s| !s.is_empty())
}

/// Find `current-context: <value>` in YAML. Stops at first match. Returns
/// the trimmed value with surrounding quotes (single or double) stripped.
/// Returns None if no such line exists, or the value is empty after trimming.
fn parse_current_context(yaml: &str) -> Option<String> {
    for line in yaml.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("current-context:") {
            let val = rest.trim();
            let unquoted = val
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .or_else(|| val.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                .unwrap_or(val);
            if unquoted.is_empty() {
                return None;
            }
            return Some(unquoted.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_current_context;

    #[test]
    fn parse_simple_kubeconfig() {
        assert_eq!(
            parse_current_context("apiVersion: v1\ncurrent-context: prod\n"),
            Some("prod".to_owned())
        );
    }

    #[test]
    fn parse_double_quoted() {
        assert_eq!(
            parse_current_context("current-context: \"prod\"\n"),
            Some("prod".to_owned())
        );
    }

    #[test]
    fn parse_single_quoted() {
        assert_eq!(
            parse_current_context("current-context: 'prod'\n"),
            Some("prod".to_owned())
        );
    }

    #[test]
    fn parse_no_context_line() {
        assert_eq!(
            parse_current_context("apiVersion: v1\nkind: Config\nclusters: []\n"),
            None
        );
    }

    #[test]
    fn parse_empty_value() {
        assert_eq!(parse_current_context("current-context: \"\"\n"), None);
    }

    #[test]
    fn parse_leading_whitespace() {
        assert_eq!(
            parse_current_context("  current-context: dev\n"),
            Some("dev".to_owned())
        );
    }
}
