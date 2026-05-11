//! `python_version` — active Python interpreter version segment.
//!
//! Shows `py:<version>` in yellow when the current directory (or any
//! ancestor, up to 64 levels) contains a Python project marker:
//! `pyproject.toml`, `setup.py`, `requirements.txt`, or `Pipfile`. The
//! walk short-circuits on the first match so deep trees don't pay for
//! every level.
//!
//! ## `python` is ambiguous
//!
//! `python` on `$PATH` may resolve to Python 2 *or* Python 3 depending on
//! the host: modern distros usually ship only `python3` and a `python`
//! shim, but plenty of legacy systems still have a real Python 2.7
//! sitting at `python`. We don't care which — we ask the interpreter and
//! parse whatever it prints. The wrinkle is that Python 2.x historically
//! wrote `--version` output to **stderr**, while Python 3 writes it to
//! **stdout**. We capture both and pick whichever is non-empty.
//!
//! ## Perf cost
//!
//! Like `node_version`, this segment spawns a subprocess (`python
//! --version`) on every prompt while a marker file is in scope. Real-world
//! cost is a few ms on warm caches, but it's still strictly more
//! expensive than a stat-only segment. The marker check keeps us out of
//! the spawn entirely when the cwd isn't a Python project, which is the
//! common case.

use p10k_rs_core::safety::sanitize_for_terminal;
use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Filenames that mark a directory as the root (or descendant) of a
/// Python project. First match wins; order is not significant.
const MARKERS: &[&str] = &["pyproject.toml", "setup.py", "requirements.txt", "Pipfile"];

/// Python-version segment.
///
/// Walks up from `ctx.cwd` looking for a Python project marker; when one
/// is found, spawns `python --version` and renders `py:<version>` in
/// yellow.
#[derive(Debug, Default)]
pub struct PythonVersion;

impl Segment for PythonVersion {
    fn name(&self) -> &'static str {
        "python_version"
    }

    fn enabled(&self, ctx: &RenderCtx<'_>) -> bool {
        has_python_marker(ctx.cwd)
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        // Defensive: `enabled()` already gated this, but `render` must be
        // safe to call regardless — mirrors the pattern in `virtualenv.rs`
        // and `vcs.rs`.
        let Some(version) = fetch_python_version() else {
            return SegmentOutput {
                text: String::new(),
                plain_len: 0,
                state: None,
                icon: None,
            };
        };

        let plain = format!("py:{version}");
        let plain_len = u16::try_from(plain.chars().count()).unwrap_or(u16::MAX);
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("yellow".into()));
        let text = format!("{fg}{plain}{}", style::reset_fg());
        SegmentOutput {
            text,
            plain_len,
            state: None,
            icon: None,
        }
    }
}

/// Walk up from `start` for up to 64 levels, returning `true` as soon as
/// a directory contains any of [`MARKERS`]. Capped at 64 to bound work
/// on pathological trees / FUSE mounts that pretend to be infinite.
fn has_python_marker(start: &std::path::Path) -> bool {
    let mut cur = Some(start);
    for _ in 0..64 {
        let Some(dir) = cur else {
            return false;
        };
        for marker in MARKERS {
            if dir.join(marker).is_file() {
                return true;
            }
        }
        cur = dir.parent();
    }
    false
}

/// Parse the raw output of `python --version`.
///
/// Strips the `Python ` prefix (with a best-effort retry after trimming
/// leading whitespace, in case some odd interpreter prefixes a newline),
/// then takes the first whitespace-separated token of the remainder. The
/// resulting version string is run through [`sanitize_for_terminal`] so
/// stray control characters never reach the rendered prompt.
fn parse_python_version(raw: &str) -> Option<String> {
    let rest = raw
        .strip_prefix("Python ")
        .or_else(|| raw.trim_start().strip_prefix("Python "))?;
    let version = rest.split_whitespace().next()?;
    if version.is_empty() {
        return None;
    }
    Some(sanitize_for_terminal(version))
}

/// Spawn `python --version` and parse its output.
///
/// Captures both stdout and stderr because Python 2 wrote `--version` to
/// stderr while Python 3 writes to stdout. Whichever stream is non-empty
/// wins; `LC_ALL=C` is set so we don't get a localised version string.
fn fetch_python_version() -> Option<String> {
    let out = std::process::Command::new("python")
        .arg("--version")
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = if stdout.trim().is_empty() {
        stderr.into_owned()
    } else {
        stdout.into_owned()
    };
    parse_python_version(&combined).filter(|s| !s.is_empty())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{has_python_marker, parse_python_version};

    /// Build a unique scratch directory under `std::env::temp_dir()` and
    /// return it. Mirrors the pattern in
    /// `crates/p10k-rs/tests/render_from_config.rs`.
    fn scratch_dir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!(
            "p10krs-python-version-{}-{}-{}",
            label,
            std::process::id(),
            nanos,
        ));
        std::fs::create_dir_all(&p).expect("mkdir scratch");
        p
    }

    #[test]
    fn parse_python3_output() {
        assert_eq!(
            parse_python_version("Python 3.12.1\n"),
            Some("3.12.1".to_owned())
        );
    }

    #[test]
    fn parse_python2_output() {
        assert_eq!(
            parse_python_version("Python 2.7.18\n"),
            Some("2.7.18".to_owned())
        );
    }

    #[test]
    fn parse_unknown_format_is_none() {
        assert_eq!(parse_python_version("python3 3.12"), None);
    }

    #[test]
    fn parse_strips_control_chars() {
        // A bogus version string with an embedded CR would otherwise
        // let an attacker overwrite the start of the prompt line.
        // sanitize_for_terminal drops control chars before the token
        // lands in `text`.
        let parsed = parse_python_version("Python 3.12\r1.foo")
            .expect("a valid token comes back from sanitisation");
        assert!(
            !parsed.contains('\r'),
            "CR survived sanitisation: {parsed:?}"
        );
    }

    #[test]
    fn walk_finds_pyproject() {
        let dir = scratch_dir("walk-pyproject");
        std::fs::write(dir.join("pyproject.toml"), b"[tool.poetry]\n").expect("write marker");
        assert!(has_python_marker(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn walk_finds_setup_py() {
        let dir = scratch_dir("walk-setup-py");
        std::fs::write(dir.join("setup.py"), b"from setuptools import setup\n")
            .expect("write marker");
        assert!(has_python_marker(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn walk_missing_returns_false() {
        let dir = scratch_dir("walk-missing");
        assert!(!has_python_marker(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }
}
