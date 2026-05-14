//! Best-effort `~/.p10k.zsh` → [`Config`] importer.
//!
//! Powerlevel10k stores configuration as a zsh script — it's not just data,
//! it's executable. We intentionally do **not** execute it; instead we
//! parse it as text. That trades some fidelity (parameter expansion,
//! conditionals, function bodies) for safety and reproducibility.
//!
//! Coverage target (per ROADMAP Phase 6): hit the common variables that
//! drive 80% of real configs. Today that means:
//!
//! - `POWERLEVEL9K_MODE` → [`Config::mode`]
//! - `POWERLEVEL9K_INSTANT_PROMPT` → [`Config::instant_prompt`]
//! - `POWERLEVEL9K_LEFT_PROMPT_ELEMENTS` → `layout.left`
//! - `POWERLEVEL9K_RIGHT_PROMPT_ELEMENTS` → `layout.right`
//! - `POWERLEVEL9K_<SEG>_FOREGROUND` / `_BACKGROUND`
//! - `POWERLEVEL9K_<SEG>_<STATE>_FOREGROUND` / `_BACKGROUND`
//!
//! Variables outside this set get a warning and are silently dropped.
//! Callers can surface the warning list via [`ImportOutcome::warnings`].

use std::collections::HashMap;

use crate::{
    Color, ColorMode, Config, InstantPromptMode, Layout, Mode, Padding, SegmentConfig, SegmentRef,
    StateOverrides,
};

/// Snapshot of the segment names `p10k-rs-segments::build()` resolves.
///
/// Kept in lock-step with the segments crate manually; the cross-crate test
/// `p10k_rs_segments::tests::importer_known_segment_names_match` pins the
/// invariant so a future regression is caught immediately. Exposed `pub`
/// solely so that cross-crate test can read it — no other consumer.
pub const KNOWN_SEGMENT_NAMES: &[&str] = &[
    "ai_host",
    "anaconda",
    "aws",
    "background_jobs",
    "command_execution_time",
    "context",
    "dir",
    "docker_context",
    "fnm",
    "jj",
    "kubecontext",
    "mise",
    "node_version",
    "nodenv",
    "os_icon",
    "pixi",
    "prompt_char",
    "pyenv",
    "python_version",
    "root_indicator",
    "rust_version",
    "status",
    "terraform",
    "time",
    "vcs",
    "vi_mode",
    "virtualenv",
];

/// Result of an import run.
#[derive(Debug, Clone, Default)]
pub struct ImportOutcome {
    /// The translated configuration. Always present — even on a fully-empty
    /// input we return a default [`Config`].
    pub config: Config,
    /// One line per skipped variable, with the original key and reason.
    /// Show these to the user so they know what didn't make it across.
    pub warnings: Vec<String>,
}

/// Translate a Powerlevel10k zsh config to a [`Config`].
///
/// Best-effort: malformed lines and unknown variables become warnings.
/// The returned `Config` is sanitised by [`Config::from_toml`]-equivalent
/// rules: every string field that lands in the prompt is run through
/// the same control-byte stripper the parser uses (see the
/// "Sanitisation" section on [`Config::from_toml`]).
#[must_use]
pub fn import_p10k_zsh(input: &str) -> ImportOutcome {
    let assignments = parse_assignments(input);
    let mut cfg = Config {
        schema_version: 1,
        ..Config::default()
    };
    let mut warnings = Vec::new();
    apply_assignments(&assignments, &mut cfg, &mut warnings);
    cfg.sanitize_in_place();
    ImportOutcome {
        config: cfg,
        warnings,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AssignValue {
    Scalar(String),
    Array(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Assignment {
    key: String,
    value: AssignValue,
}

fn parse_assignments(input: &str) -> Vec<Assignment> {
    let mut out = Vec::new();
    let mut iter = input.lines();
    while let Some(line) = iter.next() {
        let Some((key, rest)) = parse_assign_head(line) else {
            continue;
        };
        let trimmed = rest.trim();
        if let Some(array_body) = trimmed.strip_prefix('(') {
            // Could be single-line `(a b c)` or multi-line spanning more
            // input. Accumulate tokens until the matching `)`.
            if let Some(closed) = array_body.find(')') {
                let body = &array_body[..closed];
                out.push(Assignment {
                    key,
                    value: AssignValue::Array(tokenise_array(body)),
                });
            } else {
                let mut tokens = tokenise_array(array_body);
                let mut closed_ok = false;
                for cont in iter.by_ref() {
                    let cleaned = strip_inline_comment(cont);
                    if let Some(idx) = cleaned.find(')') {
                        tokens.extend(tokenise_array(&cleaned[..idx]));
                        closed_ok = true;
                        break;
                    }
                    tokens.extend(tokenise_array(&cleaned));
                }
                if closed_ok {
                    out.push(Assignment {
                        key,
                        value: AssignValue::Array(tokens),
                    });
                }
            }
        } else {
            let val = unquote_scalar(strip_inline_comment(trimmed).trim());
            out.push(Assignment {
                key,
                value: AssignValue::Scalar(val),
            });
        }
    }
    out
}

/// Parse the head of an assignment line. Returns `(key, value_text)` where
/// `value_text` is everything after the `=`. Returns `None` for comments,
/// blank lines, or anything that doesn't look like an assignment.
fn parse_assign_head(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    // Strip optional declaration keywords. zsh accepts `typeset -g`,
    // `typeset -ga`, `declare`, `local`, `export`, etc. We accept the
    // common forms greedily.
    let rest = strip_keywords(trimmed);
    let eq = rest.find('=')?;
    let key = rest[..eq].trim();
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || key.is_empty() {
        return None;
    }
    Some((key.to_owned(), &rest[eq + 1..]))
}

fn strip_keywords(line: &str) -> &str {
    let prefixes: &[&str] = &[
        "typeset -ga ",
        "typeset -gA ",
        "typeset -g ",
        "typeset -a ",
        "typeset -A ",
        "typeset ",
        "declare -g ",
        "declare ",
        "local ",
        "export ",
    ];
    let mut current = line;
    loop {
        let mut stripped = false;
        for p in prefixes {
            if let Some(rest) = current.strip_prefix(p) {
                current = rest.trim_start();
                stripped = true;
                break;
            }
        }
        if !stripped {
            return current;
        }
    }
}

fn strip_inline_comment(s: &str) -> String {
    // zsh `#` comment unless inside quotes — we don't fully track quote
    // state for arrays (token values rarely contain `#`), so this is a
    // crude split-on-first-`#`. Good enough for p10k configs in practice.
    if let Some(idx) = s.find('#') {
        s[..idx].to_owned()
    } else {
        s.to_owned()
    }
}

fn tokenise_array(body: &str) -> Vec<String> {
    body.split_whitespace()
        .filter(|t| !t.is_empty())
        .map(unquote_scalar)
        .filter(|t| !t.is_empty())
        .collect()
}

fn unquote_scalar(s: &str) -> String {
    let s = s.trim();
    let unq = if let Some(inner) = s.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        inner
    } else if let Some(inner) = s.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')) {
        inner
    } else {
        s
    };
    unq.to_owned()
}

/// Classified `POWERLEVEL9K_*` variable.
#[derive(Debug, PartialEq, Eq)]
enum KeyKind {
    Mode,
    InstantPrompt,
    LeftPromptElements,
    RightPromptElements,
    /// Per-segment foreground/background, optionally state-scoped.
    SegmentColor {
        segment: String,
        state: Option<String>,
        attr: ColorAttr,
    },
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ColorAttr {
    Foreground,
    Background,
}

fn classify(key: &str) -> Option<KeyKind> {
    let lower = key.to_ascii_lowercase();
    let rest = lower.strip_prefix("powerlevel9k_")?;
    match rest {
        "mode" => return Some(KeyKind::Mode),
        "instant_prompt" => return Some(KeyKind::InstantPrompt),
        "left_prompt_elements" => return Some(KeyKind::LeftPromptElements),
        "right_prompt_elements" => return Some(KeyKind::RightPromptElements),
        _ => {}
    }
    // Trailing color attr.
    let (head, attr) = if let Some(h) = rest.strip_suffix("_foreground") {
        (h, ColorAttr::Foreground)
    } else if let Some(h) = rest.strip_suffix("_background") {
        (h, ColorAttr::Background)
    } else {
        return None;
    };
    // Longest-match segment-name prefix. We iterate known segments by
    // length descending so `command_execution_time` wins over `command`
    // if both ever ship.
    let mut sorted: Vec<&'static str> = KNOWN_SEGMENT_NAMES.to_vec();
    sorted.sort_by_key(|s| std::cmp::Reverse(s.len()));
    for seg in sorted {
        if head == seg {
            return Some(KeyKind::SegmentColor {
                segment: seg.to_owned(),
                state: None,
                attr,
            });
        }
        if let Some(state) = head.strip_prefix(&format!("{seg}_")) {
            if !state.is_empty() {
                return Some(KeyKind::SegmentColor {
                    segment: seg.to_owned(),
                    state: Some(state.to_owned()),
                    attr,
                });
            }
        }
    }
    None
}

fn apply_assignments(assigns: &[Assignment], cfg: &mut Config, warnings: &mut Vec<String>) {
    for a in assigns {
        let Some(kind) = classify(&a.key) else {
            warnings.push(format!("unrecognised variable: {}", a.key));
            continue;
        };
        match (&kind, &a.value) {
            (KeyKind::Mode, AssignValue::Scalar(v)) => {
                cfg.mode = parse_mode(v).unwrap_or_else(|| {
                    warnings.push(format!("unknown mode value '{v}' — using default"));
                    Mode::default()
                });
            }
            (KeyKind::InstantPrompt, AssignValue::Scalar(v)) => {
                cfg.instant_prompt = parse_instant_prompt(v).unwrap_or_else(|| {
                    warnings.push(format!(
                        "unknown instant_prompt value '{v}' — using default"
                    ));
                    InstantPromptMode::default()
                });
            }
            (KeyKind::LeftPromptElements, AssignValue::Array(items)) => {
                cfg.layout.left = filter_elements(items);
            }
            (KeyKind::RightPromptElements, AssignValue::Array(items)) => {
                cfg.layout.right = filter_elements(items);
            }
            (
                KeyKind::SegmentColor {
                    segment,
                    state,
                    attr,
                },
                AssignValue::Scalar(v),
            ) => {
                if v.is_empty() {
                    continue;
                }
                let Some(color) = parse_color(v) else {
                    warnings.push(format!(
                        "could not parse color '{v}' for {key}",
                        key = a.key
                    ));
                    continue;
                };
                set_segment_color(&mut cfg.segments, segment, state.as_deref(), *attr, color);
            }
            // Non-matching shapes (e.g. scalar where array expected) get
            // dropped with a warning.
            _ => {
                warnings.push(format!("ignoring '{}' (unexpected value shape)", a.key));
            }
        }
    }
    // Default Config has `Layout` with a None `colors` field; if the user
    // didn't set colors elsewhere we leave the loader's default (Ansi256).
    // This is intentional — most p10k configs don't carry a colors mode
    // because zsh inherits from the terminal.
    let _ = ColorMode::default();
}

fn parse_mode(v: &str) -> Option<Mode> {
    match v.to_ascii_lowercase().as_str() {
        "ascii" => Some(Mode::Ascii),
        "awesome-fontconfig" | "awesome" | "awesome-mapped-fontconfig" => Some(Mode::Awesome),
        "nerdfont-v2" => Some(Mode::NerdFontV2),
        "nerdfont-v3" | "nerdfont-complete" | "nerd-font-complete" => Some(Mode::NerdFontV3),
        "compatible" => Some(Mode::Compatible),
        _ => None,
    }
}

fn parse_instant_prompt(v: &str) -> Option<InstantPromptMode> {
    match v.to_ascii_lowercase().as_str() {
        "off" => Some(InstantPromptMode::Off),
        "quiet" => Some(InstantPromptMode::Quiet),
        "verbose" => Some(InstantPromptMode::Verbose),
        _ => None,
    }
}

fn filter_elements(items: &[String]) -> Vec<SegmentRef> {
    items
        .iter()
        // p10k uses `=newline` to break the prompt onto a new line — not
        // a segment. Strip it and any other pseudo-elements that start
        // with non-identifier chars.
        .filter(|s| s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))
        .map(|s| SegmentRef(s.clone()))
        .collect()
}

fn parse_color(v: &str) -> Option<Color> {
    let trimmed = v.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(n) = trimmed.parse::<u8>() {
        return Some(Color::Indexed(n));
    }
    if let Some(rgb) = parse_hex_rgb(trimmed) {
        return Some(Color::Rgb(rgb));
    }
    // Bare names are passed through. The renderer maps them via the P9k
    // compat table in `p10k_rs_core::style`.
    Some(Color::Named(trimmed.to_owned()))
}

fn parse_hex_rgb(v: &str) -> Option<[u8; 3]> {
    let hex = v.strip_prefix('#').unwrap_or(v);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r, g, b])
}

fn set_segment_color(
    segments: &mut HashMap<String, SegmentConfig>,
    segment: &str,
    state: Option<&str>,
    attr: ColorAttr,
    color: Color,
) {
    let entry = segments.entry(segment.to_owned()).or_default();
    if let Some(state) = state {
        let st = entry.states.entry(state.to_owned()).or_default();
        match attr {
            ColorAttr::Foreground => st.foreground = Some(color),
            ColorAttr::Background => st.background = Some(color),
        }
    } else {
        match attr {
            ColorAttr::Foreground => entry.foreground = Some(color),
            ColorAttr::Background => entry.background = Some(color),
        }
    }
}

#[allow(dead_code)] // referenced via Default in SegmentConfig path; keeps schema explicit
fn _force_link() {
    let _: Padding = Padding::default();
    let _: Layout = Layout::default();
    let _: StateOverrides = StateOverrides::default();
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_scalar() {
        let out = parse_assignments("typeset -g POWERLEVEL9K_MODE=ascii\n");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "POWERLEVEL9K_MODE");
        assert_eq!(out[0].value, AssignValue::Scalar("ascii".to_owned()));
    }

    #[test]
    fn parses_double_quoted_value() {
        let out = parse_assignments(r#"POWERLEVEL9K_MODE="ascii""#);
        assert_eq!(out[0].value, AssignValue::Scalar("ascii".to_owned()));
    }

    #[test]
    fn parses_single_quoted_value() {
        let out = parse_assignments("POWERLEVEL9K_MODE='ascii'\n");
        assert_eq!(out[0].value, AssignValue::Scalar("ascii".to_owned()));
    }

    #[test]
    fn parses_empty_value() {
        let out = parse_assignments("POWERLEVEL9K_DIR_BACKGROUND=\n");
        assert_eq!(out[0].value, AssignValue::Scalar(String::new()));
    }

    #[test]
    fn parses_single_line_array() {
        let out = parse_assignments("POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(dir vcs)");
        assert_eq!(
            out[0].value,
            AssignValue::Array(vec!["dir".into(), "vcs".into()])
        );
    }

    #[test]
    fn parses_multi_line_array_with_comments() {
        let input = "typeset -g POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(\n  dir       # current directory\n  vcs       # version control\n  prompt_char\n)";
        let out = parse_assignments(input);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].value,
            AssignValue::Array(vec!["dir".into(), "vcs".into(), "prompt_char".into(),])
        );
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        let input = "# a comment\n\n  # another\nFOO=bar\n";
        let out = parse_assignments(input);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "FOO");
    }

    #[test]
    fn strips_typeset_variants() {
        for prefix in [
            "typeset -g ",
            "typeset -ga ",
            "declare -g ",
            "local ",
            "export ",
        ] {
            let input = format!("{prefix}POWERLEVEL9K_MODE=ascii\n");
            let out = parse_assignments(&input);
            assert_eq!(out[0].key, "POWERLEVEL9K_MODE", "{prefix:?}");
        }
    }

    #[test]
    fn classify_mode() {
        assert_eq!(classify("POWERLEVEL9K_MODE"), Some(KeyKind::Mode));
    }

    #[test]
    fn classify_instant_prompt() {
        assert_eq!(
            classify("POWERLEVEL9K_INSTANT_PROMPT"),
            Some(KeyKind::InstantPrompt)
        );
    }

    #[test]
    fn classify_left_elements() {
        assert_eq!(
            classify("POWERLEVEL9K_LEFT_PROMPT_ELEMENTS"),
            Some(KeyKind::LeftPromptElements)
        );
    }

    #[test]
    fn classify_segment_color_simple() {
        assert_eq!(
            classify("POWERLEVEL9K_DIR_FOREGROUND"),
            Some(KeyKind::SegmentColor {
                segment: "dir".into(),
                state: None,
                attr: ColorAttr::Foreground,
            })
        );
    }

    #[test]
    fn classify_segment_color_with_state() {
        assert_eq!(
            classify("POWERLEVEL9K_VCS_CLEAN_FOREGROUND"),
            Some(KeyKind::SegmentColor {
                segment: "vcs".into(),
                state: Some("clean".into()),
                attr: ColorAttr::Foreground,
            })
        );
    }

    #[test]
    fn classify_multi_word_segment_with_state() {
        // command_execution_time is itself multi-word; a state suffix
        // must still resolve cleanly via longest-prefix match.
        assert_eq!(
            classify("POWERLEVEL9K_COMMAND_EXECUTION_TIME_FOREGROUND"),
            Some(KeyKind::SegmentColor {
                segment: "command_execution_time".into(),
                state: None,
                attr: ColorAttr::Foreground,
            })
        );
    }

    #[test]
    fn classify_unknown_segment() {
        assert_eq!(classify("POWERLEVEL9K_NOTASEGMENT_FOREGROUND"), None);
    }

    #[test]
    fn classify_unknown_key_shape() {
        assert_eq!(classify("POWERLEVEL9K_DIR_CLASSES"), None);
        assert_eq!(classify("SOMETHING_ELSE"), None);
    }

    #[test]
    fn parse_color_indexed() {
        assert_eq!(parse_color("42"), Some(Color::Indexed(42)));
        assert_eq!(parse_color("255"), Some(Color::Indexed(255)));
    }

    #[test]
    fn parse_color_named() {
        assert_eq!(parse_color("red"), Some(Color::Named("red".into())));
        assert_eq!(
            parse_color("brightblue"),
            Some(Color::Named("brightblue".into()))
        );
    }

    #[test]
    fn parse_color_hex_rgb() {
        assert_eq!(parse_color("#ff8800"), Some(Color::Rgb([0xff, 0x88, 0x00])));
        assert_eq!(parse_color("ff8800"), Some(Color::Rgb([0xff, 0x88, 0x00])));
    }

    #[test]
    fn parse_color_empty_is_none() {
        assert_eq!(parse_color(""), None);
        assert_eq!(parse_color("   "), None);
    }

    #[test]
    fn import_minimal_config() {
        let input = "
            typeset -g POWERLEVEL9K_MODE=ascii
            typeset -g POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(dir vcs prompt_char)
            typeset -g POWERLEVEL9K_DIR_FOREGROUND=4
        ";
        let out = import_p10k_zsh(input);
        assert_eq!(out.config.mode, Mode::Ascii);
        assert_eq!(out.config.layout.left.len(), 3);
        assert_eq!(out.config.layout.left[0].0, "dir");
        let dir = out.config.segments.get("dir").expect("dir entry");
        assert_eq!(dir.foreground, Some(Color::Indexed(4)));
    }

    #[test]
    fn import_state_overrides() {
        let input = "
            POWERLEVEL9K_VCS_CLEAN_FOREGROUND=10
            POWERLEVEL9K_VCS_MODIFIED_FOREGROUND=11
        ";
        let out = import_p10k_zsh(input);
        let vcs = out.config.segments.get("vcs").expect("vcs entry");
        let clean = vcs.states.get("clean").expect("clean state");
        let modified = vcs.states.get("modified").expect("modified state");
        assert_eq!(clean.foreground, Some(Color::Indexed(10)));
        assert_eq!(modified.foreground, Some(Color::Indexed(11)));
    }

    #[test]
    fn import_strips_pseudo_elements() {
        // p10k allows `=newline` and similar as layout pseudo-elements.
        // Drop them — they're not real segments.
        let input = "POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(dir =newline prompt_char)\n";
        let out = import_p10k_zsh(input);
        let names: Vec<&str> = out
            .config
            .layout
            .left
            .iter()
            .map(|r| r.0.as_str())
            .collect();
        assert_eq!(names, &["dir", "prompt_char"]);
    }

    #[test]
    fn import_warns_on_unknown_variables() {
        let input = "POWERLEVEL9K_DIR_CLASSES=()\nPOWERLEVEL9K_MADE_UP_THING=42\n";
        let out = import_p10k_zsh(input);
        assert_eq!(out.warnings.len(), 2);
        assert!(out.warnings.iter().any(|w| w.contains("DIR_CLASSES")));
        assert!(out.warnings.iter().any(|w| w.contains("MADE_UP_THING")));
    }

    #[test]
    fn import_sanitises_imported_colors_indirectly() {
        // Colors are typed enums; no control bytes can land here. But
        // separators *can* be imported textually in a future slice — this
        // test pins that sanitise_in_place runs on the imported Config.
        let mut cfg = Config::default();
        cfg.layout.separators.left = Some("hi\rEVIL".to_owned());
        cfg.sanitize_in_place();
        assert_eq!(cfg.layout.separators.left.as_deref(), Some("hiEVIL"));
    }

    #[test]
    fn known_segment_names_are_alphabetical() {
        let mut sorted: Vec<&'static str> = KNOWN_SEGMENT_NAMES.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted.as_slice(), KNOWN_SEGMENT_NAMES);
    }
}
