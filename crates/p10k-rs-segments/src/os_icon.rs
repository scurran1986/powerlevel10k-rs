//! `os_icon` — host operating system glyph.
//!
//! Always-on segment that emits a single styled glyph identifying the OS the
//! shell is running on. The glyph is picked at compile time from
//! `cfg!(target_os = ...)` — there is no runtime probe, so this is a constant
//! per build.
//!
//! # Nerd Font caveat
//!
//! The glyphs are Nerd Font v3 codepoints from the Unicode private-use area
//! (e.g. U+F17C for Linux). Terminals without a Nerd Font installed will
//! render boxes (tofu). The future `configure` wizard will detect Nerd Font
//! availability and auto-switch to ASCII fallbacks; for now, the only escape
//! hatch is the `?` glyph emitted on unknown OSes. If the boxes bother you,
//! drop this segment from `[layout].left` in your config.

use p10k_rs_core::style::{self, Color};
use p10k_rs_core::{RenderCtx, Segment, SegmentOutput};

/// Operating-system icon segment.
///
/// Emits a single Nerd Font glyph styled in white. Always enabled.
#[derive(Debug, Default)]
pub struct OsIcon;

impl Segment for OsIcon {
    fn name(&self) -> &'static str {
        "os_icon"
    }

    fn render(&self, ctx: &RenderCtx<'_>) -> SegmentOutput {
        let glyph = os_glyph();
        let fg = style::render_fg(ctx.config, self.name(), None, Color::Named("white".into()));
        let text = format!("{fg}{glyph}{}", style::reset_fg());
        SegmentOutput {
            text,
            plain_len: 1,
            state: None,
            icon: None,
        }
    }
}

/// Pick the OS glyph for the current build target.
///
/// Resolved at compile time via `cfg!(target_os = ...)`. Codepoints are
/// Nerd Font v3 private-use-area entries; the ASCII `?` is the fallback for
/// any target we don't have a glyph for.
const fn os_glyph() -> char {
    if cfg!(target_os = "linux") {
        '\u{f17c}'
    } else if cfg!(target_os = "macos") {
        '\u{f179}'
    } else if cfg!(target_os = "freebsd")
        || cfg!(target_os = "openbsd")
        || cfg!(target_os = "netbsd")
    {
        '\u{f30c}'
    } else {
        '?'
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell};

    use super::{os_glyph, OsIcon};

    fn make_ctx<'a>(cfg: &'a Config, env: &'a EnvSnapshot) -> RenderCtx<'a> {
        RenderCtx {
            config: cfg,
            shell: Shell::Zsh,
            host: HostKind::None,
            cwd: Path::new("/"),
            git: None,
            last_status: 0,
            last_duration: Duration::ZERO,
            jobs: 0,
            now: SystemTime::UNIX_EPOCH,
            env,
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
        assert_eq!(out.plain_len, 1);
        assert_eq!(out.state, None);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn glyph_picker_linux() {
        assert_eq!(os_glyph(), '\u{f17c}');
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn glyph_picker_macos() {
        assert_eq!(os_glyph(), '\u{f179}');
    }

    #[test]
    fn glyph_picker_is_single_char() {
        // Sanity: whatever the build target, plain_len = 1 must hold.
        let g = os_glyph();
        assert_eq!(g.to_string().chars().count(), 1);
    }
}
