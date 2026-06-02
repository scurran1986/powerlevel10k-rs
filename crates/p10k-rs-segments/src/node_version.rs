//! `node_version` — Node.js runtime version segment.
//!
//! Renders `node:<version>` (stripping the leading `v` Node prints) when the
//! current working directory sits inside a `Node.js` project. Project
//! detection walks upward from `cwd` looking for `package.json`, capped at
//! 64 levels to bound work on pathological symlink loops.
//!
//! ## The perf caveat
//!
//! Rendering this segment spawns `node --version` as a subprocess on every
//! prompt. That cost is the price of admission for runtime-version segments
//! and is the same shape that `pyenv`, `rbenv`, etc. would pay. We bound the
//! damage two ways:
//!
//! 1. The segment is **gated by `package.json` presence**, so directories
//!    outside JS projects never pay the spawn at all.
//! 2. A future slice can cache by `cwd` or hand the lookup off to the
//!    long-lived daemon so the fork/exec drops out of the hot path. For
//!    today we spawn unconditionally when enabled — boring code wins and
//!    the spawn is cheap relative to a cold prompt.
//!
//! Stdout from `node` is treated as untrusted input — a hostile `$PATH`
//! could shadow `node` with a binary that emits ANSI or `%`-expansion
//! payloads. Output flows through `sanitize_for_terminal` before it lands
//! in the rendered segment.

use std::time::Duration;

use p10k_rs_core::output_with_deadline;
use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Wall-clock budget for `node --version`. A healthy install runs in
/// 50–150 ms warm; 500 ms gives roughly 3× headroom while staying short
/// enough that a wedged binary (NordVPN-freeze pattern, hung NFS,
/// corrupted toolchain) doesn't keep the prompt hostage for more than
/// half a second. Slice 51.
const VERSION_DEADLINE: Duration = Duration::from_millis(500);

/// Default Nerd Font v3 glyph (mdi-nodejs). Override via
/// `[segment.node_version].icon = "..."` in the TOML config.
const DEFAULT_ICON: &str = "\u{e718}";

/// Node.js version segment.
///
/// Enabled when a `package.json` exists in `cwd` or any ancestor (up to 64
/// levels). When enabled, spawns `node --version`, strips the leading `v`,
/// and renders `node:<version>` in green.
#[derive(Debug, Default)]
pub struct NodeVersion;

impl Segment for NodeVersion {
    fn name(&self) -> &'static str {
        "node_version"
    }

    fn enabled(&self, ctx: &RenderCtx<'_>) -> bool {
        has_package_json(ctx.cwd)
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — same discipline as `virtualenv.rs`.
        let Some(version) = fetch_node_version() else {
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

/// Walk upward from `start`, returning `true` as soon as a `package.json`
/// file is found in any ancestor directory. Bounded at 64 iterations to
/// keep symlink loops and absurd path depths from stalling the prompt.
fn has_package_json(start: &std::path::Path) -> bool {
    has_package_json_bounded(start, None, 64)
}

/// Inner walker with an optional `stop_at` boundary. Production passes
/// `None` (walk to filesystem root or depth cap, whichever comes first);
/// tests pass a boundary directory so the walk doesn't reach
/// `/tmp/package.json` on developer machines polluted with one. The
/// boundary check happens *before* the file check on each iteration, so
/// `stop_at` is the first directory NOT inspected.
fn has_package_json_bounded(
    start: &std::path::Path,
    stop_at: Option<&std::path::Path>,
    max_depth: usize,
) -> bool {
    let mut cur = Some(start);
    for _ in 0..max_depth {
        let Some(dir) = cur else {
            return false;
        };
        if Some(dir) == stop_at {
            return false;
        }
        if dir.join("package.json").is_file() {
            return true;
        }
        cur = dir.parent();
    }
    false
}

/// Spawn `node --version`, capture stdout, strip the leading `v`, and
/// sanitise. Returns `None` on any failure (missing binary, non-zero exit,
/// empty output after cleaning).
///
/// `LC_ALL=C` is set so a hostile locale can't reshape the output. Stderr
/// is discarded — we don't surface tool errors in the prompt.
fn fetch_node_version() -> Option<String> {
    // Slice 51: route through `output_with_deadline` so a wedged `node`
    // (e.g. behind a hung filesystem or a VPN reconnect) can't lock the
    // prompt. On timeout we get `ErrorKind::TimedOut`; the `.ok()?` below
    // collapses that to `None`, which renders as a hidden segment.
    let mut cmd = std::process::Command::new("node");
    cmd.arg("--version").env("LC_ALL", "C");
    let out = output_with_deadline(&mut cmd, VERSION_DEADLINE).ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    let trimmed = raw.trim();
    let version = trimmed.strip_prefix('v').unwrap_or(trimmed);
    let clean = sanitize_for_terminal(version).into_owned();
    if clean.is_empty() {
        None
    } else {
        Some(clean)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use p10k_rs_core::safety::SafeText;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::style::Color;
    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::{has_package_json, has_package_json_bounded, NodeVersion};

    /// Build a unique scratch directory under `std::env::temp_dir()`. Caller
    /// is responsible for cleanup; we deliberately leak on panic so failing
    /// tests leave evidence on disk.
    fn scratch_dir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!(
            "p10krs-node-version-{}-{}-{}",
            label,
            std::process::id(),
            nanos,
        ));
        std::fs::create_dir_all(&p).expect("mkdir scratch");
        p
    }

    #[test]
    fn walk_finds_package_json_in_cwd() {
        let dir = scratch_dir("cwd");
        std::fs::write(dir.join("package.json"), "{}").expect("write package.json");
        assert!(has_package_json(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn walk_finds_package_json_in_parent() {
        let root = scratch_dir("parent");
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).expect("mkdir nested");
        std::fs::write(root.join("package.json"), "{}").expect("write package.json");
        assert!(has_package_json(&nested));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn walk_returns_false_when_missing() {
        // Use the bounded walker stopped at scratch's parent so a stray
        // `/tmp/package.json` on a developer machine can't trip us.
        // Production callers pass `None` for `stop_at` and walk to the
        // filesystem root; this test asserts only the empty-scratch case.
        let dir = scratch_dir("missing");
        let stop = dir.parent();
        assert!(!has_package_json_bounded(&dir, stop, 64));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn renders_with_default_icon() {
        // Gate: render only produces the icon when `node --version` runs;
        // no-op on machines without a node runtime on PATH.
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            return;
        }
        let dir = scratch_dir("renders-default-icon");
        std::fs::write(dir.join("package.json"), "{}").expect("write package.json");
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
        let out = NodeVersion.render(&ctx);
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
        std::fs::remove_dir_all(&dir).ok();
    }
}
