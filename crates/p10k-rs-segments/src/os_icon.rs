//! `os_icon` — host operating system glyph.
//!
//! Always-on segment that emits a single glyph identifying the OS the shell
//! is running on, painted white-on-grey (P10K-classic palette, `brightblack`
//! as a neutral leading block).
//!
//! # Detection
//!
//! Compile-time `cfg!(target_os = ...)` only narrows us down to the family.
//! On Linux we additionally probe `/etc/os-release` for the `ID=` field to
//! pick a distro-specific Nerd Font glyph (Ubuntu, Debian, Arch, Fedora,
//! `NixOS`, Alpine, Gentoo, Manjaro, openSUSE), and `/proc/version` to detect
//! WSL. The probe runs once per process and is cached in a `OnceLock`, so
//! repeated prompts in the same process pay one syscall pair total.
//!
//! All returned glyphs are `&'static str`, which is what makes the cache
//! cheap — no allocations, no lifetimes to thread through.
//!
//! # Override
//!
//! Users can pin the glyph with `[segment.os_icon].icon = "..."` in TOML;
//! the detected value is only the default passed to
//! [`style::resolve_icon`].
//!
//! # Nerd Font caveat
//!
//! Glyphs are Nerd Font v3 private-use-area codepoints. Terminals without
//! a Nerd Font installed render boxes (tofu). The future `configure`
//! wizard will detect Nerd Font availability and auto-switch to ASCII
//! fallbacks; for now the escape hatch is dropping this segment from
//! `[layout].left`.

use std::sync::OnceLock;

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Generic Linux Tux — fallback when `/etc/os-release` is missing or the
/// `ID=` value is not one we recognise.
const LINUX_GENERIC: &str = "\u{f17c}";
/// Windows glyph, used when we detect we're running under WSL.
const WSL_WINDOWS: &str = "\u{f17a}";
/// Apple glyph for `macOS`.
const MACOS_APPLE: &str = "\u{f179}";
/// `FreeBSD` daemon — close-enough fallback for the BSDs and other Unixes.
const BSD_DAEMON: &str = "\u{f30e}";
/// Last-resort unknown glyph (question-mark in a circle).
const UNKNOWN: &str = "\u{f128}";

/// Process-lifetime cache for the detected OS glyph. `/etc/os-release` and
/// `/proc/version` don't change between prompts on a live host, so reading
/// them once is correct and cheap.
static DETECTED_ICON: OnceLock<&'static str> = OnceLock::new();

/// Operating-system icon segment.
///
/// Emits a single Nerd Font glyph styled white-on-brightblack. Always
/// enabled. The glyph is picked once per process by probing the host and
/// then cached.
#[derive(Debug, Default)]
pub struct OsIcon;

impl Segment for OsIcon {
    fn name(&self) -> &'static str {
        "os_icon"
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let default = detect_os_icon();
        let icon = style::resolve_icon(ctx.config, self.name(), None, default);
        let bg = style::render_bg(
            ctx.config,
            self.name(),
            None,
            Color::Named("brightblack".into()),
        );
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("white".into()));
        let text = format!("{bg}{fg}{icon}{}{}", style::reset_fg(), style::reset_bg());
        SegmentOutput {
            text,
            plain_len: 1,
            state: None,
            icon: None,
            background: Some(Color::Named("brightblack".into())),
        }
    }
}

/// Resolve (and cache) the OS glyph for the current host.
///
/// Precedence:
///
/// 1. WSL (Linux + `/proc/version` mentioning `microsoft`/`WSL`) → Windows.
/// 2. Linux `/etc/os-release` `ID=` lookup against the known-distro map.
/// 3. macOS → Apple.
/// 4. Other Unix (BSDs) → `FreeBSD` daemon.
/// 5. Otherwise → generic unknown.
///
/// The result is a `&'static str` for cheap caching.
fn detect_os_icon() -> &'static str {
    DETECTED_ICON.get_or_init(probe_os_icon)
}

/// One-shot probe that walks the precedence chain. Factored out so
/// `detect_os_icon` is a thin cache wrapper.
fn probe_os_icon() -> &'static str {
    if cfg!(target_os = "linux") {
        if is_wsl() {
            return WSL_WINDOWS;
        }
        if let Ok(contents) = std::fs::read_to_string("/etc/os-release") {
            if let Some(id) = parse_os_release_id(&contents) {
                return map_linux_id(id);
            }
        }
        return LINUX_GENERIC;
    }
    if cfg!(target_os = "macos") {
        return MACOS_APPLE;
    }
    if cfg!(target_os = "freebsd")
        || cfg!(target_os = "openbsd")
        || cfg!(target_os = "netbsd")
        || cfg!(target_os = "dragonfly")
    {
        return BSD_DAEMON;
    }
    UNKNOWN
}

/// Return `true` when `/proc/version` looks like a WSL kernel build —
/// containing `microsoft` (case-insensitive) or `WSL`. Filesystem-error
/// or non-Linux hosts return `false`.
fn is_wsl() -> bool {
    let Ok(contents) = std::fs::read_to_string("/proc/version") else {
        return false;
    };
    let lower = contents.to_ascii_lowercase();
    lower.contains("microsoft") || lower.contains("wsl")
}

/// Map a known `/etc/os-release` `ID=` value to its Nerd Font glyph,
/// falling back to the generic Linux Tux for anything unrecognised.
fn map_linux_id(id: &str) -> &'static str {
    match id {
        "ubuntu" => "\u{f31b}",
        "debian" => "\u{f306}",
        "arch" => "\u{f303}",
        "fedora" => "\u{f30a}",
        "nixos" => "\u{f313}",
        "alpine" => "\u{f300}",
        "gentoo" => "\u{f30d}",
        "manjaro" => "\u{f312}",
        "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" => "\u{f314}",
        _ => LINUX_GENERIC,
    }
}

/// Extract the `ID=` value from `/etc/os-release` contents.
///
/// Returns the first occurrence (later assignments are ignored — the file
/// is shell syntax but in practice never repeats keys). The value is
/// returned with surrounding single or double quotes stripped and with
/// surrounding whitespace trimmed. Returns `None` if no `ID=` line exists
/// or the value is empty after unquoting.
///
/// Lines beginning with `#` (after leading whitespace) are skipped per the
/// `os-release(5)` format spec.
fn parse_os_release_id(contents: &str) -> Option<&str> {
    for line in contents.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("ID=") else {
            continue;
        };
        let val = rest.trim();
        let unquoted = val
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| val.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(val);
        if unquoted.is_empty() {
            return None;
        }
        return Some(unquoted);
    }
    None
}

#[cfg(test)]
mod tests {
    use p10k_rs_core::safety::SafeText;
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::{detect_os_icon, map_linux_id, parse_os_release_id, OsIcon, LINUX_GENERIC};

    fn make_ctx<'a>(cfg: &'a Config, env: &'a EnvSnapshot) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: Path::new("/"),
            cwd_display: SafeText::default(),
            git: None,
            jj: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
            upcoming_command: "",
            shell_integration_active: false,
            sync_output: false,
        }
    }

    #[test]
    fn name_is_os_icon() {
        assert_eq!(OsIcon.name(), "os_icon");
    }

    #[test]
    fn renders_some_glyph() {
        let (cfg, env) = (Config::default(), EnvSnapshot::default());
        let ctx = make_ctx(&cfg, &env);
        let out = OsIcon.render(&ctx);
        assert!(!out.text.is_empty());
        // ANSI256 white = colour index 7.
        assert!(
            out.text.contains("\x1b[38;5;7m"),
            "missing white FG escape: {:?}",
            out.text
        );
        // brightblack (ANSI256 index 8) bg for the P10K-classic grey
        // leading block.
        assert!(
            out.text.contains("\x1b[48;5;8m"),
            "missing brightblack BG escape: {:?}",
            out.text
        );
        assert_eq!(
            out.background,
            Some(p10k_rs_core::style::Color::Named("brightblack".into()))
        );
        assert_eq!(out.plain_len, 1);
        assert_eq!(out.state, None);
    }

    #[test]
    fn detected_icon_is_single_char() {
        // The cached / probed glyph must remain a single visual codepoint
        // so `plain_len = 1` keeps holding.
        let g = detect_os_icon();
        assert_eq!(g.chars().count(), 1, "glyph not single-char: {g:?}");
    }

    #[test]
    fn parse_os_release_id_extracts_known_ids() {
        // Plain ubuntu.
        assert_eq!(
            parse_os_release_id("NAME=Ubuntu\nID=ubuntu\nVERSION=\"22.04\"\n"),
            Some("ubuntu")
        );
        // Double-quoted.
        assert_eq!(parse_os_release_id("ID=\"debian\"\n"), Some("debian"));
        // Single-quoted.
        assert_eq!(parse_os_release_id("ID='arch'\n"), Some("arch"));
        // Leading whitespace + comments before the line.
        assert_eq!(
            parse_os_release_id("# distro file\n#ID=ignored\n  ID=fedora\n"),
            Some("fedora")
        );
        // openSUSE variants.
        assert_eq!(
            parse_os_release_id("ID=opensuse-tumbleweed\n"),
            Some("opensuse-tumbleweed")
        );
        // Nothing.
        assert_eq!(parse_os_release_id("NAME=Foo\nVERSION=1\n"), None);
        // Empty value.
        assert_eq!(parse_os_release_id("ID=\"\"\n"), None);
        // Commented-out ID is ignored.
        assert_eq!(parse_os_release_id("# ID=ubuntu\n"), None);
    }

    #[test]
    fn map_linux_id_covers_known_distros() {
        assert_eq!(map_linux_id("ubuntu"), "\u{f31b}");
        assert_eq!(map_linux_id("debian"), "\u{f306}");
        assert_eq!(map_linux_id("arch"), "\u{f303}");
        assert_eq!(map_linux_id("fedora"), "\u{f30a}");
        assert_eq!(map_linux_id("nixos"), "\u{f313}");
        assert_eq!(map_linux_id("alpine"), "\u{f300}");
        assert_eq!(map_linux_id("gentoo"), "\u{f30d}");
        assert_eq!(map_linux_id("manjaro"), "\u{f312}");
        assert_eq!(map_linux_id("opensuse"), "\u{f314}");
        assert_eq!(map_linux_id("opensuse-leap"), "\u{f314}");
        assert_eq!(map_linux_id("opensuse-tumbleweed"), "\u{f314}");
        // Unknown ids fall back to the generic Tux.
        assert_eq!(map_linux_id("voidlinux"), LINUX_GENERIC);
        assert_eq!(map_linux_id(""), LINUX_GENERIC);
    }
}
