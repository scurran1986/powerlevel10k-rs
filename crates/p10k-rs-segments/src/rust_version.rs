//! `rust_version` — Rust toolchain version segment.
//!
//! Renders `rust:<version>` in red when the current working directory sits
//! inside a Cargo project. Project detection walks upward from `cwd` looking
//! for `Cargo.toml`, capped at 64 levels to bound work on pathological
//! symlink loops.
//!
//! ## The perf caveat
//!
//! Rendering this segment spawns `rustc --version` as a subprocess on every
//! prompt — the same shape as `node_version`. We bound the damage two ways:
//!
//! 1. The segment is **gated by `Cargo.toml` presence**, so directories
//!    outside Rust projects never pay the spawn at all.
//! 2. A future slice can cache by `cwd` or hand the lookup off to the
//!    long-lived daemon so the fork/exec drops out of the hot path. For
//!    today we spawn unconditionally when enabled — boring code wins and
//!    the spawn is cheap relative to a cold prompt.
//!
//! Stdout from `rustc` is treated as untrusted input — a hostile `$PATH`
//! could shadow `rustc` with a binary that emits ANSI or `%`-expansion
//! payloads. Output flows through `sanitize_for_terminal` before it lands
//! in the rendered segment.

use std::time::Duration;

use p10k_rs_core::output_with_deadline;
use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Wall-clock budget for `rustc --version`. Same 500 ms ceiling as the
/// other version segments — see `node_version::VERSION_DEADLINE`.
/// Slice 51.
const VERSION_DEADLINE: Duration = Duration::from_millis(500);

/// Default Nerd Font v3 glyph (dev-rust). Override via
/// `[segment.rust_version].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{e7a8}";

/// Rust toolchain version segment.
///
/// Enabled when a `Cargo.toml` exists in `cwd` or any ancestor (up to 64
/// levels). When enabled, spawns `rustc --version`, parses the version
/// token, and renders `rust:<version>` in red (matching Rust's mascot
/// palette).
#[derive(Debug, Default)]
pub struct RustVersion;

impl Segment for RustVersion {
    fn name(&self) -> &'static str {
        "rust_version"
    }

    fn enabled(&self, ctx: &RenderCtx<'_>) -> bool {
        has_cargo_toml(ctx.cwd)
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same discipline as `node_version.rs`.
        let Some(version) = fetch_rust_version() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
                background: None,
            };
        };

        let plain = format!("rust:{version}");
        let icon = style::resolve_icon(ctx.config, self.name(), None, DEFAULT_ICON);
        let bg = style::render_bg(ctx.config, self.name(), None, Color::Named("red".into()));
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
            background: Some(style::resolve_bg(
                ctx.config,
                self.name(),
                None,
                Color::Named("red".into()),
            )),
        }
    }
}

/// Walk upward from `start`, returning `true` as soon as a `Cargo.toml`
/// file is found in any ancestor directory. Bounded at 64 iterations to
/// keep symlink loops and absurd path depths from stalling the prompt.
fn has_cargo_toml(start: &std::path::Path) -> bool {
    let mut cur = Some(start);
    for _ in 0..64 {
        let Some(dir) = cur else {
            return false;
        };
        if dir.join("Cargo.toml").is_file() {
            return true;
        }
        cur = dir.parent();
    }
    false
}

/// Parse the output of `rustc --version`. The canonical shape is:
///
/// ```text
/// rustc 1.88.0 (5e0f72fd1 2025-09-19)
/// ```
///
/// We pull the second whitespace-separated token as the version and run it
/// through [`sanitize_for_terminal`]. Returns `None` when the first token
/// isn't `rustc`, the second token is missing, or sanitisation strips it to
/// empty.
fn parse_rustc_version(raw: &str) -> Option<String> {
    let mut tokens = raw.split_whitespace();
    if tokens.next()? != "rustc" {
        return None;
    }
    let v = tokens.next()?;
    if v.is_empty() {
        return None;
    }
    Some(sanitize_for_terminal(v).into_owned())
}

/// Spawn `rustc --version`, capture stdout, and parse out the version
/// token. Returns `None` on any failure (missing binary, non-zero exit,
/// unparseable output, empty after sanitisation).
///
/// `LC_ALL=C` is set so a hostile locale can't reshape the output. Stderr
/// is discarded — we don't surface tool errors in the prompt.
fn fetch_rust_version() -> Option<String> {
    // Slice 51: bounded subprocess. A wedged `rustc` (broken rustup
    // shim, hung toolchain download) would otherwise freeze the prompt;
    // `output_with_deadline` caps the wait at 500 ms.
    let mut cmd = std::process::Command::new("rustc");
    cmd.arg("--version").env("LC_ALL", "C");
    let out = output_with_deadline(&mut cmd, VERSION_DEADLINE).ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    parse_rustc_version(&raw).filter(|s| !s.is_empty())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use p10k_rs_core::safety::SafeText;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::{has_cargo_toml, parse_rustc_version, RustVersion};

    /// Build a unique scratch directory under `std::env::temp_dir()`. Caller
    /// is responsible for cleanup; we deliberately leak on panic so failing
    /// tests leave evidence on disk.
    fn scratch_dir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!(
            "p10krs-rust-version-{}-{}-{}",
            label,
            std::process::id(),
            nanos,
        ));
        std::fs::create_dir_all(&p).expect("mkdir scratch");
        p
    }

    #[test]
    fn parse_standard_output() {
        assert_eq!(
            parse_rustc_version("rustc 1.88.0 (5e0f72fd1 2025-09-19)\n"),
            Some("1.88.0".to_owned())
        );
    }

    #[test]
    fn parse_nightly_output() {
        assert_eq!(
            parse_rustc_version("rustc 1.91.0-nightly (abc 2026-01-01)\n"),
            Some("1.91.0-nightly".to_owned())
        );
    }

    #[test]
    fn parse_non_rustc_returns_none() {
        assert_eq!(parse_rustc_version("garbage 1.0\n"), None);
    }

    #[test]
    fn parse_empty_returns_none() {
        assert_eq!(parse_rustc_version(""), None);
    }

    #[test]
    fn walk_finds_cargo_toml() {
        let dir = scratch_dir("cwd");
        std::fs::write(dir.join("Cargo.toml"), "").expect("write Cargo.toml");
        assert!(has_cargo_toml(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn walk_finds_cargo_toml_in_parent() {
        let root = scratch_dir("parent");
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).expect("mkdir nested");
        std::fs::write(root.join("Cargo.toml"), "").expect("write Cargo.toml");
        assert!(has_cargo_toml(&nested));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn walk_missing_returns_false() {
        let dir = scratch_dir("missing");
        assert!(!has_cargo_toml(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn renders_with_default_icon() {
        // Gate: render only produces the icon when `rustc --version` runs;
        // no-op on machines without a Rust toolchain on PATH.
        if std::process::Command::new("rustc")
            .arg("--version")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            return;
        }
        let dir = scratch_dir("renders-default-icon");
        std::fs::write(dir.join("Cargo.toml"), "").expect("write Cargo.toml");
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = RenderCtx {
            config: &cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: &dir,
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
        let out = RustVersion.render(&ctx);
        assert!(
            out.text.contains('\u{e7a8}'),
            "default icon missing: {:?}",
            out.text
        );
        assert_eq!(out.icon, Some("\u{e7a8}"));
        // Powerline ribbon: red bg SGR (`48;5;1`) must appear in the styled
        // text and the matching declaration must surface on `background` so
        // the renderer can paint the arrow into / out of the segment.
        assert!(
            out.text.contains("\x1b[48;5;1m"),
            "red bg SGR missing: {:?}",
            out.text
        );
        assert_eq!(
            out.background,
            Some(p10k_rs_core::style::Color::Named("red".into()))
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
