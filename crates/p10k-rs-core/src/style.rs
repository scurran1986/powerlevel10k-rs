//! Color and styling primitives.
//!
//! Single source of truth for prompt color emission. Segments must call
//! [`render_fg`] / [`render_bg`] / [`reset_fg`] rather than building ANSI
//! escape strings by hand — that's how per-segment config overrides
//! (`[segment.dir].foreground = "red"`) actually reach the prompt.
//!
//! [`Color`] and [`ColorMode`] are re-exported from `p10k-rs-config` so
//! callers can name them without depending on the config crate directly.

pub use p10k_rs_config::{Color, ColorMode};

use p10k_rs_config::Config;

/// SGR reset to default foreground. `\x1b[39m` rather than `\x1b[0m` so any
/// background a future segment may have set survives.
#[must_use]
pub fn reset_fg() -> &'static str {
    "\x1b[39m"
}

/// SGR reset to default background.
#[must_use]
pub fn reset_bg() -> &'static str {
    "\x1b[49m"
}

/// Resolve a segment's foreground color and emit the corresponding SGR
/// escape, honouring config overrides.
///
/// Precedence (highest first):
/// 1. `config.segments[segment].states[state].foreground` (when `state` is `Some`)
/// 2. `config.segments[segment].foreground`
/// 3. `default`
///
/// The emitted escape is mode-aware: a [`Color::Rgb`] under
/// [`ColorMode::Ansi8`] degrades to the nearest 8-colour cube cell, etc.
#[must_use]
pub fn render_fg(config: &Config, segment: &str, state: Option<&str>, default: Color) -> String {
    let color = resolve(config, segment, state, Side::Fg, default);
    sgr_fg(&color, config.colors)
}

/// Same as [`render_fg`] but for the background.
#[must_use]
pub fn render_bg(config: &Config, segment: &str, state: Option<&str>, default: Color) -> String {
    let color = resolve(config, segment, state, Side::Bg, default);
    sgr_bg(&color, config.colors)
}

#[derive(Clone, Copy)]
enum Side {
    Fg,
    Bg,
}

fn resolve(
    config: &Config,
    segment: &str,
    state: Option<&str>,
    side: Side,
    default: Color,
) -> Color {
    let Some(sc) = config.segments.get(segment) else {
        return default;
    };
    if let Some(s) = state {
        if let Some(so) = sc.states.get(s) {
            let state_color = match side {
                Side::Fg => so.foreground.as_ref(),
                Side::Bg => so.background.as_ref(),
            };
            if let Some(c) = state_color {
                return c.clone();
            }
        }
    }
    let base = match side {
        Side::Fg => sc.foreground.as_ref(),
        Side::Bg => sc.background.as_ref(),
    };
    base.cloned().unwrap_or(default)
}

/// Emit a foreground SGR escape for `color` under `mode`.
///
/// Public so segments that need to compose styles themselves (e.g.
/// the prompt arrow that flips by `last_status`) can call it directly.
#[must_use]
pub fn sgr_fg(color: &Color, mode: ColorMode) -> String {
    match (resolve_to_palette(color, mode), mode) {
        (Palette::Ansi8(n), _) => format!("\x1b[{}m", 30 + n),
        (Palette::Ansi256(n), _) => format!("\x1b[38;5;{n}m"),
        (Palette::Rgb(r, g, b), _) => format!("\x1b[38;2;{r};{g};{b}m"),
    }
}

/// Background counterpart to [`sgr_fg`].
#[must_use]
pub fn sgr_bg(color: &Color, mode: ColorMode) -> String {
    match (resolve_to_palette(color, mode), mode) {
        (Palette::Ansi8(n), _) => format!("\x1b[{}m", 40 + n),
        (Palette::Ansi256(n), _) => format!("\x1b[48;5;{n}m"),
        (Palette::Rgb(r, g, b), _) => format!("\x1b[48;2;{r};{g};{b}m"),
    }
}

enum Palette {
    Ansi8(u8),
    Ansi256(u8),
    Rgb(u8, u8, u8),
}

fn resolve_to_palette(color: &Color, mode: ColorMode) -> Palette {
    match (color, mode) {
        (Color::Named(name), m) => named_to_palette(name, m),
        (Color::Indexed(n), ColorMode::Ansi8) => Palette::Ansi8(n & 0x07),
        (Color::Indexed(n), _) => Palette::Ansi256(*n),
        (Color::Rgb([r, g, b]), ColorMode::TrueColor) => Palette::Rgb(*r, *g, *b),
        (Color::Rgb([r, g, b]), ColorMode::Ansi256) => Palette::Ansi256(rgb_to_256(*r, *g, *b)),
        (Color::Rgb([r, g, b]), ColorMode::Ansi8) => Palette::Ansi8(rgb_to_8(*r, *g, *b)),
    }
}

fn named_to_palette(name: &str, mode: ColorMode) -> Palette {
    // Powerlevel9k compat: the 16 standard names always resolve cleanly.
    // Arbitrary p9k names (`wheat4`, `darkslateblue4`, ...) land later when
    // the full upstream table imports. Unknown → default foreground (39),
    // expressed as ANSI 8 code 9 which our format string renders as `\x1b[39m`.
    let entry: Option<(u8, u8, [u8; 3])> = match name.to_ascii_lowercase().as_str() {
        "black" => Some((0, 0, [0, 0, 0])),
        "red" => Some((1, 1, [170, 0, 0])),
        "green" => Some((2, 2, [0, 170, 0])),
        "yellow" => Some((3, 3, [170, 85, 0])),
        "blue" => Some((4, 4, [0, 0, 170])),
        "magenta" => Some((5, 5, [170, 0, 170])),
        "cyan" => Some((6, 6, [0, 170, 170])),
        "white" => Some((7, 7, [170, 170, 170])),
        "brightblack" | "grey" | "gray" => Some((0, 8, [85, 85, 85])),
        "brightred" => Some((1, 9, [255, 85, 85])),
        "brightgreen" => Some((2, 10, [85, 255, 85])),
        "brightyellow" => Some((3, 11, [255, 255, 85])),
        "brightblue" => Some((4, 12, [85, 85, 255])),
        "brightmagenta" => Some((5, 13, [255, 85, 255])),
        "brightcyan" => Some((6, 14, [85, 255, 255])),
        "brightwhite" => Some((7, 15, [255, 255, 255])),
        _ => None,
    };
    match (entry, mode) {
        (Some((c8, _, _)), ColorMode::Ansi8) => Palette::Ansi8(c8),
        (Some((_, c256, _)), ColorMode::Ansi256) => Palette::Ansi256(c256),
        (Some((_, _, [r, g, b])), ColorMode::TrueColor) => Palette::Rgb(r, g, b),
        (None, _) => Palette::Ansi8(9),
    }
}

fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    // 6×6×6 colour cube starting at index 16. `c * 5 / 255` is mathematically
    // bounded to [0, 5], so try_from never fails — the unwrap_or is a no-op.
    let q = |c: u8| -> u8 { u8::try_from(u16::from(c) * 5 / 255).unwrap_or(0) };
    16 + 36 * q(r) + 6 * q(g) + q(b)
}

fn rgb_to_8(r: u8, g: u8, b: u8) -> u8 {
    let bit = |c: u8| -> u8 { u8::from(c >= 128) };
    bit(r) | (bit(g) << 1) | (bit(b) << 2)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn cfg_with(toml_str: &str) -> Config {
        Config::from_toml(toml_str).unwrap()
    }

    #[test]
    fn reset_constants_match_ansi() {
        assert_eq!(reset_fg(), "\x1b[39m");
        assert_eq!(reset_bg(), "\x1b[49m");
    }

    #[test]
    fn named_blue_in_ansi256() {
        let out = sgr_fg(&Color::Named("blue".into()), ColorMode::Ansi256);
        assert_eq!(out, "\x1b[38;5;4m");
    }

    #[test]
    fn named_red_in_ansi8() {
        let out = sgr_fg(&Color::Named("red".into()), ColorMode::Ansi8);
        assert_eq!(out, "\x1b[31m");
    }

    #[test]
    fn named_green_in_truecolor() {
        let out = sgr_fg(&Color::Named("green".into()), ColorMode::TrueColor);
        assert_eq!(out, "\x1b[38;2;0;170;0m");
    }

    #[test]
    fn indexed_falls_back_in_ansi8() {
        // Indexed colours above 7 mask to the low three bits.
        let out = sgr_fg(&Color::Indexed(202), ColorMode::Ansi8);
        assert_eq!(out, "\x1b[32m"); // 202 & 7 == 2 == green
    }

    #[test]
    fn rgb_downgrades_to_256_then_to_8() {
        let mid_red = Color::Rgb([200, 0, 0]);
        assert_eq!(sgr_fg(&mid_red, ColorMode::TrueColor), "\x1b[38;2;200;0;0m");
        // 200 → q(200) = 200*5/255 = 3, so 16 + 36*3 + 0 + 0 = 124.
        assert_eq!(sgr_fg(&mid_red, ColorMode::Ansi256), "\x1b[38;5;124m");
        // r=200 >= 128 → bit 0; g=0, b=0 → 1 → red.
        assert_eq!(sgr_fg(&mid_red, ColorMode::Ansi8), "\x1b[31m");
    }

    #[test]
    fn unknown_named_color_defaults_to_fg() {
        // `wheat4` and similar p9k names aren't in our table yet. Don't
        // crash; emit default foreground.
        let out = sgr_fg(&Color::Named("wheat4".into()), ColorMode::Ansi256);
        assert_eq!(out, "\x1b[39m");
    }

    #[test]
    fn bg_uses_4x_offset() {
        assert_eq!(
            sgr_bg(&Color::Named("red".into()), ColorMode::Ansi8),
            "\x1b[41m"
        );
        assert_eq!(
            sgr_bg(&Color::Named("red".into()), ColorMode::Ansi256),
            "\x1b[48;5;1m"
        );
    }

    #[test]
    fn render_fg_falls_back_to_default_when_no_config() {
        let cfg = cfg_with("schema_version = 1\n");
        let out = render_fg(&cfg, "dir", None, Color::Named("blue".into()));
        assert_eq!(out, "\x1b[38;5;4m");
    }

    #[test]
    fn render_fg_honours_segment_override() {
        let cfg = cfg_with(
            "schema_version = 1\n\
             [segment.dir]\n\
             foreground = \"red\"\n",
        );
        let out = render_fg(&cfg, "dir", None, Color::Named("blue".into()));
        assert_eq!(out, "\x1b[38;5;1m");
    }

    #[test]
    fn render_fg_honours_state_override() {
        let cfg = cfg_with(
            "schema_version = 1\n\
             [segment.vcs]\n\
             foreground = \"yellow\"\n\
             [segment.vcs.states.dirty]\n\
             foreground = \"magenta\"\n",
        );
        let dirty = render_fg(&cfg, "vcs", Some("dirty"), Color::Named("yellow".into()));
        let clean = render_fg(&cfg, "vcs", Some("clean"), Color::Named("yellow".into()));
        assert_eq!(dirty, "\x1b[38;5;5m");
        // No "clean" state override → segment-level "yellow" wins.
        assert_eq!(clean, "\x1b[38;5;3m");
    }

    #[test]
    fn render_fg_indexed_color_passes_through() {
        let cfg = cfg_with(
            "schema_version = 1\n\
             [segment.dir]\n\
             foreground = 202\n",
        );
        let out = render_fg(&cfg, "dir", None, Color::Named("blue".into()));
        assert_eq!(out, "\x1b[38;5;202m");
    }

    #[test]
    fn render_fg_rgb_color_in_truecolor_mode() {
        let cfg = cfg_with(
            "schema_version = 1\n\
             colors = \"true-color\"\n\
             [segment.dir]\n\
             foreground = [255, 128, 0]\n",
        );
        let out = render_fg(&cfg, "dir", None, Color::Named("blue".into()));
        assert_eq!(out, "\x1b[38;2;255;128;0m");
    }

    #[test]
    fn render_bg_works_independently_of_fg_override() {
        let cfg = cfg_with(
            "schema_version = 1\n\
             [segment.dir]\n\
             foreground = \"red\"\n\
             background = \"blue\"\n",
        );
        let fg = render_fg(&cfg, "dir", None, Color::Named("black".into()));
        let bg = render_bg(&cfg, "dir", None, Color::Named("black".into()));
        assert_eq!(fg, "\x1b[38;5;1m");
        assert_eq!(bg, "\x1b[48;5;4m");
    }
}
