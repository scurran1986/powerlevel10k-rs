//! Color and styling primitives.
//!
//! Single source of truth for prompt color emission. Segments must call
//! [`render_fg`] / [`render_bg`] / [`reset_fg`] rather than building ANSI
//! escape strings by hand — that's how per-segment config overrides
//! (`[segment.dir].foreground = "red"`) actually reach the prompt.
//!
//! [`Color`] and [`ColorMode`] are re-exported from `p10k-rs-config` so
//! callers can name them without depending on the config crate directly.

use std::borrow::Cow;

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
pub fn render_fg(
    config: &Config,
    segment: &str,
    state: Option<&str>,
    default: Color,
) -> Cow<'static, str> {
    let color = resolve(config, segment, state, Side::Fg, default);
    sgr_fg(&color, config.colors)
}

/// Same as [`render_fg`] but for the background.
#[must_use]
pub fn render_bg(
    config: &Config,
    segment: &str,
    state: Option<&str>,
    default: Color,
) -> Cow<'static, str> {
    let color = resolve(config, segment, state, Side::Bg, default);
    sgr_bg(&color, config.colors)
}

/// Resolve a segment's background to a [`Color`], honouring config
/// overrides — the colour itself, not the SGR escape [`render_bg`] emits.
///
/// Precedence matches [`render_bg`]. Segments must stash *this* value in
/// [`SegmentOutput::background`](crate::SegmentOutput) rather than the raw
/// default: the powerline-separator renderer reads that field to colour
/// the chevron between adjacent chips, so a hardcoded default there leaves
/// the separator mismatched whenever a theme overrides the chip colour.
#[must_use]
pub fn resolve_bg(config: &Config, segment: &str, state: Option<&str>, default: Color) -> Color {
    resolve(config, segment, state, Side::Bg, default)
}

/// Resolve a segment's icon glyph, honouring per-state and per-segment
/// overrides.
///
/// Precedence (highest first):
/// 1. `config.segments[segment].states[state].icon` (when `state` is `Some`)
/// 2. `config.segments[segment].icon`
/// 3. `default`
///
/// Returns a `&str` borrowing from either the config string or the
/// caller-supplied default — no allocation in the common case. Caller
/// uses it directly in their `format!` chain.
#[must_use]
pub fn resolve_icon<'a>(
    config: &'a Config,
    segment: &str,
    state: Option<&str>,
    default: &'a str,
) -> &'a str {
    if let Some(sc) = config.segments.get(segment) {
        if let Some(s) = state {
            if let Some(so) = sc.states.get(s) {
                if let Some(icon) = so.icon.as_deref() {
                    return icon;
                }
            }
        }
        if let Some(icon) = sc.icon.as_deref() {
            return icon;
        }
    }
    default
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

/// Precomputed SGR foreground escapes for the 10 Ansi8 codepoints we
/// ever emit (0..=7 = the 8 named colours, 9 = "default fg"). Indexed
/// directly by [`Palette::Ansi8`]'s carried value; index 8 is unused
/// and kept as the empty string so the array stays contiguous.
const ANSI8_FG_TABLE: [&str; 10] = [
    "\x1b[30m", "\x1b[31m", "\x1b[32m", "\x1b[33m", "\x1b[34m", "\x1b[35m", "\x1b[36m", "\x1b[37m",
    "", "\x1b[39m",
];

/// Background counterpart to [`ANSI8_FG_TABLE`].
const ANSI8_BG_TABLE: [&str; 10] = [
    "\x1b[40m", "\x1b[41m", "\x1b[42m", "\x1b[43m", "\x1b[44m", "\x1b[45m", "\x1b[46m", "\x1b[47m",
    "", "\x1b[49m",
];

/// Emit a foreground SGR escape for `color` under `mode`.
///
/// Public so segments that need to compose styles themselves (e.g.
/// the prompt arrow that flips by `last_status`) can call it directly.
///
/// Returns `Cow<'static, str>`: the common Ansi8 path borrows from a
/// precomputed static table (zero alloc), while Ansi256 / Rgb fall
/// through to `format!` and yield an owned String. Callers that
/// `push_str`/`format!` the result see no difference — `Cow<str>`
/// derefs and `Display`s like a `&str`.
#[must_use]
pub fn sgr_fg(color: &Color, mode: ColorMode) -> Cow<'static, str> {
    // `Color::None` foreground means "terminal default" — same as a reset.
    if matches!(color, Color::None) {
        return Cow::Borrowed(reset_fg());
    }
    match (resolve_to_palette(color, mode), mode) {
        (Palette::Ansi8(n), _) => {
            Cow::Borrowed(ANSI8_FG_TABLE.get(n as usize).copied().unwrap_or(""))
        }
        (Palette::Ansi256(n), _) => Cow::Owned(format!("\x1b[38;5;{n}m")),
        (Palette::Rgb(r, g, b), _) => Cow::Owned(format!("\x1b[38;2;{r};{g};{b}m")),
    }
}

/// Background counterpart to [`sgr_fg`].
#[must_use]
pub fn sgr_bg(color: &Color, mode: ColorMode) -> Cow<'static, str> {
    // `Color::None` background paints no chip — emit nothing so the
    // segment text rides the terminal's own background (lean rendering).
    if matches!(color, Color::None) {
        return Cow::Borrowed("");
    }
    match (resolve_to_palette(color, mode), mode) {
        (Palette::Ansi8(n), _) => {
            Cow::Borrowed(ANSI8_BG_TABLE.get(n as usize).copied().unwrap_or(""))
        }
        (Palette::Ansi256(n), _) => Cow::Owned(format!("\x1b[48;5;{n}m")),
        (Palette::Rgb(r, g, b), _) => Cow::Owned(format!("\x1b[48;2;{r};{g};{b}m")),
    }
}

enum Palette {
    Ansi8(u8),
    Ansi256(u8),
    Rgb(u8, u8, u8),
}

fn resolve_to_palette(color: &Color, mode: ColorMode) -> Palette {
    // `FollowTerminal` only meaningfully changes the lookup for *named*
    // colours that hit one of the 16 standard palette slots — that's
    // what OSC 4 actually answers about. Indexed (8..=255) and arbitrary
    // `Rgb` literals can't benefit from the queried palette, so they
    // degrade to the `Ansi256` policy.
    match (color, mode) {
        (Color::Named(name), m) => named_to_palette(name, m),
        (Color::Indexed(n), ColorMode::Ansi8) => Palette::Ansi8(n & 0x07),
        (Color::Indexed(n), _) => Palette::Ansi256(*n),
        (Color::Rgb([r, g, b]), ColorMode::TrueColor) => Palette::Rgb(*r, *g, *b),
        (Color::Rgb([r, g, b]), ColorMode::Ansi8) => Palette::Ansi8(rgb_to_8(*r, *g, *b)),
        (Color::Rgb([r, g, b]), ColorMode::Ansi256 | ColorMode::FollowTerminal) => {
            Palette::Ansi256(rgb_to_256(*r, *g, *b))
        }
        // `ColorMode` is `#[non_exhaustive]`; future variants degrade to
        // Ansi256 as the safe default.
        (Color::Rgb([r, g, b]), _) => Palette::Ansi256(rgb_to_256(*r, *g, *b)),
        // `sgr_fg`/`sgr_bg` short-circuit `Color::None` before calling
        // this, so this arm is unreachable in practice; map to the
        // "default" slot (code 9 → `\x1b[39m`/`\x1b[49m`) for safety.
        (Color::None, _) => Palette::Ansi8(9),
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
        (Some((_, _, [r, g, b])), ColorMode::TrueColor) => Palette::Rgb(r, g, b),
        // FollowTerminal: if the OSC 4 probe answered, swap in the
        // terminal's actual RGB for this palette slot; otherwise fall
        // back to the 256-colour index so something still paints. The
        // spec-default `[r, g, b]` triple is intentionally unused here —
        // we'd rather surface the terminal's idea of "red" than our
        // hardcoded `[170, 0, 0]` when the probe failed.
        (Some((_, c256, _)), ColorMode::FollowTerminal) => match terminal_palette_lookup(c256) {
            Some([r, g, b]) => Palette::Rgb(r, g, b),
            None => Palette::Ansi256(c256),
        },
        // `ColorMode` is `#[non_exhaustive]`; any future variant degrades
        // to Ansi256 with the named-colour's canonical 256 index.
        (Some((_, c256, _)), _) => Palette::Ansi256(c256),
        (None, _) => Palette::Ansi8(9),
    }
}

/// Look up `index` in the OSC 4 cached palette, returning `None` when
/// either the probe hasn't run, ran and failed, or `index` is outside
/// the 16-slot range we query.
///
/// Thin shim over [`crate::term_query::palette`] so the style helpers
/// don't import the module directly in every callsite — and so a future
/// slice that wants to disable the cache for tests has one place to
/// stub.
fn terminal_palette_lookup(index: u8) -> Option<[u8; 3]> {
    let palette = crate::term_query::palette()?;
    let slot = palette.get(usize::from(index))?;
    match slot {
        Color::Rgb([r, g, b]) => Some([*r, *g, *b]),
        // The cache is constructed exclusively from `Color::Rgb`. Any
        // other variant means somebody mutated `PALETTE_CACHE`
        // through unsafe means; play it safe and bail.
        _ => None,
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
    fn follow_terminal_falls_back_to_ansi256_without_probe() {
        // Slice 53: under `cargo test` `/dev/tty` is normally a pipe (or
        // missing entirely in CI sandboxes), so the OSC 4 probe returns
        // `None` and the cache stores that miss. The renderer must then
        // emit the same byte sequence as plain `Ansi256` rather than
        // panicking or emitting an empty escape.
        let out = sgr_fg(&Color::Named("red".into()), ColorMode::FollowTerminal);
        // Either the probe succeeded (truecolor SGR — possible on a
        // dev's interactive `cargo test`) or it failed and we landed on
        // the Ansi256 fallback. Both are acceptable; the contract is
        // "something legible paints".
        let truecolor_ok = out.starts_with("\x1b[38;2;");
        let ansi256_ok = out == "\x1b[38;5;1m";
        assert!(
            truecolor_ok || ansi256_ok,
            "expected truecolor or Ansi256 red, got: {out:?}"
        );
    }

    #[test]
    fn follow_terminal_rgb_literal_degrades_to_ansi256() {
        // Arbitrary `[r,g,b]` literals can't be looked up in the queried
        // 16-slot palette — they aren't named colours. `FollowTerminal`
        // must degrade to `Ansi256`'s 6×6×6 cube for them, not panic or
        // emit raw truecolor (the latter would violate the user's
        // explicit "follow my terminal" intent on a non-truecolor host).
        let out = sgr_fg(&Color::Rgb([200, 0, 0]), ColorMode::FollowTerminal);
        assert_eq!(out, "\x1b[38;5;124m");
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

    #[test]
    fn sgr_bg_none_emits_nothing() {
        // `Color::None` background paints no chip — the SGR is empty so
        // the segment text rides the terminal's own background.
        assert_eq!(sgr_bg(&Color::None, ColorMode::TrueColor), "");
        assert_eq!(sgr_bg(&Color::None, ColorMode::Ansi8), "");
    }

    #[test]
    fn sgr_fg_none_falls_back_to_default_fg() {
        // `Color::None` foreground means "terminal default" — same byte
        // sequence as `reset_fg`.
        assert_eq!(sgr_fg(&Color::None, ColorMode::TrueColor), reset_fg());
    }

    #[test]
    fn resolve_bg_returns_override_color_not_default() {
        // The powerline separator reads `SegmentOutput.background` (a
        // `Color`), not the SGR string `render_bg` emits. `resolve_bg`
        // must return the config-resolved colour so separators match the
        // chip — the bug was segments stashing the hardcoded default in
        // `out.background`, leaving rainbow chevrons between overridden
        // chips.
        let cfg = cfg_with(
            "schema_version = 1\n\
             [segment.dir]\n\
             background = \"red\"\n",
        );
        let bg = resolve_bg(&cfg, "dir", None, Color::Named("blue".into()));
        assert_eq!(bg, Color::Named("red".into()));
    }

    #[test]
    fn resolve_bg_falls_back_to_default_without_override() {
        let cfg = cfg_with("schema_version = 1\n");
        let bg = resolve_bg(&cfg, "dir", None, Color::Named("blue".into()));
        assert_eq!(bg, Color::Named("blue".into()));
    }

    #[test]
    fn resolve_bg_honours_state_override() {
        let cfg = cfg_with(
            "schema_version = 1\n\
             [segment.vcs]\n\
             background = \"yellow\"\n\
             [segment.vcs.states.dirty]\n\
             background = \"magenta\"\n",
        );
        let dirty = resolve_bg(&cfg, "vcs", Some("dirty"), Color::Named("green".into()));
        let clean = resolve_bg(&cfg, "vcs", Some("clean"), Color::Named("green".into()));
        assert_eq!(dirty, Color::Named("magenta".into()));
        // No "clean" state override → segment-level "yellow" wins.
        assert_eq!(clean, Color::Named("yellow".into()));
    }

    #[test]
    fn resolve_icon_falls_back_to_default() {
        let cfg = cfg_with("schema_version = 1\n");
        assert_eq!(resolve_icon(&cfg, "dir", None, "DEFAULT"), "DEFAULT");
        assert_eq!(
            resolve_icon(&cfg, "dir", Some("dirty"), "DEFAULT"),
            "DEFAULT"
        );
    }

    #[test]
    fn resolve_icon_honours_segment_override() {
        let cfg = cfg_with(
            "schema_version = 1\n\
             [segment.dir]\n\
             icon = \"OVERRIDE\"\n",
        );
        assert_eq!(resolve_icon(&cfg, "dir", None, "DEFAULT"), "OVERRIDE");
        // State given but no state-keyed icon → segment-level wins.
        assert_eq!(
            resolve_icon(&cfg, "dir", Some("any"), "DEFAULT"),
            "OVERRIDE"
        );
    }

    #[test]
    fn resolve_icon_honours_state_override() {
        let cfg = cfg_with(
            "schema_version = 1\n\
             [segment.vcs]\n\
             icon = \"BASE\"\n\
             [segment.vcs.states.dirty]\n\
             icon = \"DIRTY\"\n",
        );
        assert_eq!(resolve_icon(&cfg, "vcs", Some("dirty"), "DEFAULT"), "DIRTY");
        // Unknown state → segment-level fallback.
        assert_eq!(resolve_icon(&cfg, "vcs", Some("clean"), "DEFAULT"), "BASE");
        // No state → segment-level.
        assert_eq!(resolve_icon(&cfg, "vcs", None, "DEFAULT"), "BASE");
    }
}
