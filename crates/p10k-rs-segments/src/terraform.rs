//! `terraform` — Terraform workspace segment.
//!
//! Shows `tf:<workspace>` in magenta when a Terraform workspace can be
//! resolved for the current directory. Detection mirrors what Terraform
//! itself does:
//!
//! 1. `$TF_WORKSPACE` wins if set and non-empty (Terraform reads this env
//!    var before consulting the on-disk state).
//! 2. Otherwise we walk up from `ctx.cwd` looking for
//!    `.terraform/environment`, whose first line is the active workspace
//!    name.
//!
//! Workspace names come from the filesystem / environment and are
//! attacker-controlled in the same way `cwd` is — they pass through
//! [`SafeText::from_untrusted`] before landing in the prompt buffer.

use p10k_rs_core::safety::SafeText;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Default Nerd Font v3 glyph (mdi-terraform). Override via
/// `[segment.terraform].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{f1771}";

/// Terraform workspace segment.
///
/// Renders `tf:<workspace>` when either `$TF_WORKSPACE` is set or a
/// `.terraform/environment` file is reachable by walking up from the
/// current directory. Hidden otherwise.
#[derive(Debug, Default)]
pub struct Terraform;

impl Segment for Terraform {
    fn name(&self) -> &'static str {
        "terraform"
    }

    fn enabled(&self, ctx: &RenderCtx<'_>) -> bool {
        current_terraform_workspace(ctx.cwd).is_some()
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same pattern `virtualenv` and `vcs`
        // follow for missing inputs.
        let Some(workspace) = current_terraform_workspace(ctx.cwd) else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
                background: None,
            };
        };

        let plain = format!("tf:{workspace}");
        let icon = style::resolve_icon(ctx.config, self.name(), None, DEFAULT_ICON);
        let bg = style::render_bg(
            ctx.config,
            self.name(),
            None,
            Color::Named("magenta".into()),
        );
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
            background: Some(Color::Named("magenta".into())),
        }
    }
}

/// Resolve the active Terraform workspace, if any.
///
/// `$TF_WORKSPACE` takes precedence (matches Terraform's own resolution
/// order); otherwise falls back to walking up from `cwd`. The returned
/// string is already sanitised for terminal output.
fn current_terraform_workspace(cwd: &std::path::Path) -> Option<SafeText> {
    if let Ok(ws) = std::env::var("TF_WORKSPACE") {
        let clean = SafeText::from_untrusted(&ws);
        if !clean.is_empty() {
            return Some(clean);
        }
    }
    find_terraform_workspace(cwd)
        .map(|s| SafeText::from_untrusted(&s))
        .filter(|s| !s.is_empty())
}

/// Walks up from `start` looking for `.terraform/environment`. Returns
/// the trimmed first line of the file, or None if not found or unreadable.
/// Caps the walk depth at 64 to avoid pathological symlink loops.
fn find_terraform_workspace(start: &std::path::Path) -> Option<String> {
    let mut cur = Some(start);
    for _ in 0..64 {
        let dir = cur?;
        let candidate = dir.join(".terraform").join("environment");
        if candidate.is_file() {
            let raw = std::fs::read_to_string(&candidate).ok()?;
            let workspace = raw.lines().next()?.trim().to_owned();
            if !workspace.is_empty() {
                return Some(workspace);
            }
            return None;
        }
        cur = dir.parent();
    }
    None
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use p10k_rs_core::safety::SafeText;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::{find_terraform_workspace, Terraform};

    /// Build a unique scratch directory under `std::env::temp_dir()` and
    /// return it. Caller is responsible for `remove_dir_all` when done.
    /// Mirrors the pattern used by `crates/p10k-rs/tests/render_from_config.rs`.
    fn scratch_dir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!(
            "p10krs-terraform-{}-{}-{}",
            label,
            std::process::id(),
            nanos,
        ));
        std::fs::create_dir_all(&p).expect("mkdir scratch");
        p
    }

    #[test]
    fn walk_finds_workspace_in_cwd() {
        let cwd = scratch_dir("ws-in-cwd");
        let tf_dir = cwd.join(".terraform");
        std::fs::create_dir_all(&tf_dir).expect("mkdir .terraform");
        std::fs::write(tf_dir.join("environment"), "default").expect("write env");

        assert_eq!(find_terraform_workspace(&cwd), Some("default".to_owned()));

        std::fs::remove_dir_all(&cwd).expect("cleanup");
    }

    #[test]
    fn walk_finds_workspace_in_parent() {
        let root = scratch_dir("ws-in-parent");
        let tf_dir = root.join(".terraform");
        std::fs::create_dir_all(&tf_dir).expect("mkdir .terraform");
        std::fs::write(tf_dir.join("environment"), "staging").expect("write env");

        let inner = root.join("modules").join("network");
        std::fs::create_dir_all(&inner).expect("mkdir inner");

        assert_eq!(find_terraform_workspace(&inner), Some("staging".to_owned()));

        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn walk_returns_none_when_no_terraform_dir() {
        let cwd = scratch_dir("no-tf");

        assert_eq!(find_terraform_workspace(&cwd), None);

        std::fs::remove_dir_all(&cwd).expect("cleanup");
    }

    #[test]
    fn walk_strips_trailing_newline() {
        let cwd = scratch_dir("trailing-newline");
        let tf_dir = cwd.join(".terraform");
        std::fs::create_dir_all(&tf_dir).expect("mkdir .terraform");
        std::fs::write(tf_dir.join("environment"), "prod\n").expect("write env");

        assert_eq!(find_terraform_workspace(&cwd), Some("prod".to_owned()));

        std::fs::remove_dir_all(&cwd).expect("cleanup");
    }

    #[test]
    fn walk_empty_file_is_none() {
        let cwd = scratch_dir("empty-file");
        let tf_dir = cwd.join(".terraform");
        std::fs::create_dir_all(&tf_dir).expect("mkdir .terraform");
        std::fs::write(tf_dir.join("environment"), "").expect("write env");

        assert_eq!(find_terraform_workspace(&cwd), None);

        std::fs::remove_dir_all(&cwd).expect("cleanup");
    }

    #[test]
    fn renders_with_default_icon() {
        let cwd = scratch_dir("renders-default-icon");
        let tf_dir = cwd.join(".terraform");
        std::fs::create_dir_all(&tf_dir).expect("mkdir .terraform");
        std::fs::write(tf_dir.join("environment"), "default").expect("write env");
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = RenderCtx {
            config: &cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: &cwd,
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
        let out = Terraform.render(&ctx);
        assert!(
            out.text.contains('\u{f1771}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{f1771}"));
        // Powerline ribbon: magenta bg SGR (`48;5;5`) must appear in the
        // styled text and the matching declaration must surface on
        // `background` so adjacent segments can chain arrows correctly.
        assert!(
            out.text.contains("\x1b[48;5;5m"),
            "magenta bg SGR missing: {:?}",
            out.text
        );
        assert_eq!(
            out.background,
            Some(p10k_rs_core::style::Color::Named("magenta".into()))
        );
        std::fs::remove_dir_all(&cwd).expect("cleanup");
    }
}
