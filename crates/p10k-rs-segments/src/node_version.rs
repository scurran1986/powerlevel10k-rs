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

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

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
            };
        };

        let plain = format!("node:{version}");
        let plain_len = u16::try_from(plain.chars().count()).unwrap_or(u16::MAX);
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("green".into()));
        let text = format!("{fg}{plain}{}", style::reset_fg());
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: None,
        }
    }
}

/// Walk upward from `start`, returning `true` as soon as a `package.json`
/// file is found in any ancestor directory. Bounded at 64 iterations to
/// keep symlink loops and absurd path depths from stalling the prompt.
fn has_package_json(start: &std::path::Path) -> bool {
    let mut cur = Some(start);
    for _ in 0..64 {
        let Some(dir) = cur else {
            return false;
        };
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
    let out = std::process::Command::new("node")
        .arg("--version")
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    let trimmed = raw.trim();
    let version = trimmed.strip_prefix('v').unwrap_or(trimmed);
    let clean = sanitize_for_terminal(version);
    if clean.is_empty() {
        None
    } else {
        Some(clean)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::has_package_json;

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
        let dir = scratch_dir("missing");
        assert!(!has_package_json(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }
}
