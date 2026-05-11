//! Configuration wizard for `p10k-rs`.
//!
//! `p10k-rs configure` runs this wizard interactively: a short Q&A flow that
//! picks a preset, glyph mode, and colour palette, then emits the resulting
//! `Config` as TOML on stdout. Users redirect to their config path:
//!
//! ```text
//! p10k-rs configure > ~/.config/p10k-rs/config.toml
//! ```
//!
//! The full 30-screen `06-wizard-and-presets.md` flow is a separate slice —
//! this MVP wizard ships the three highest-leverage choices.
//!
//! Designed for unit-testability: the core driver [`run_with`] takes an
//! arbitrary `BufRead` + `Write` pair so tests can script the entire flow
//! without touching the terminal.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::io::{BufRead, Write};

use thiserror::Error;

use p10k_rs_config::Config;

/// Errors raised by the wizard.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum WizardError {
    /// User aborted before completing the flow (Ctrl-D / EOF on stdin).
    #[error("wizard cancelled by user")]
    Cancelled,
    /// Failure constructing the resulting [`Config`] from the chosen
    /// inputs. The bundled preset TOML strings are static, so this only
    /// fires if a preset gets corrupted in source.
    #[error("internal preset error: {0}")]
    Preset(#[from] p10k_rs_config::ConfigError),
    /// I/O failure reading the user's input or writing prompts.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Run the wizard against stdin/stderr.
///
/// Returns a fully-validated [`Config`] composed from the user's answers.
/// Prompts and progress messages go to stderr so the binary can pipe stdout
/// straight into a config file. The caller is responsible for serialising
/// the returned config (via [`Config::to_toml`]) and writing it to disk.
///
/// # Errors
///
/// See [`WizardError`]. The most common: [`WizardError::Cancelled`] if the
/// user closes stdin (Ctrl-D).
pub fn run() -> Result<Config, WizardError> {
    let stdin = std::io::stdin();
    let stderr = std::io::stderr();
    run_with(stdin.lock(), stderr.lock())
}

/// Driver core — same as [`run`] but with arbitrary reader/writer.
///
/// Exposed so tests can script the entire flow without a real terminal.
/// The writer is where prompts and progress go (stderr in production); the
/// reader is where answers come from (stdin in production).
pub fn run_with<R: BufRead, W: Write>(mut reader: R, mut writer: W) -> Result<Config, WizardError> {
    writeln!(writer, "Welcome to the p10k-rs configure wizard.\n")?;

    let preset_idx = prompt_choice(
        &mut reader,
        &mut writer,
        "Pick a style preset:",
        &[
            "lean      — minimal single-line prompt",
            "classic   — info-rich (os, dir, vcs, status, time, …)",
            "minimal   — just dir + prompt char",
        ],
        0,
    )?;

    let mode_idx = prompt_choice(
        &mut reader,
        &mut writer,
        "Glyph mode:",
        &[
            "nerd-font-v3 — rich icons; requires a Nerd Font installed",
            "ascii        — works on every terminal",
        ],
        0,
    )?;

    let colors_idx = prompt_choice(
        &mut reader,
        &mut writer,
        "Colour palette:",
        &[
            "256-color  — widely supported (the default)",
            "true-color — 24-bit RGB; modern terminals",
            "8-color    — basic; for very limited terminals",
        ],
        0,
    )?;

    let toml = build_toml(preset_idx, mode_idx, colors_idx);
    let cfg = Config::from_toml(&toml)?;
    writeln!(writer, "\nDone. Writing TOML to stdout.")?;
    Ok(cfg)
}

/// Read a single-line answer; return its trimmed value or `None` on EOF.
fn read_line<R: BufRead>(reader: &mut R) -> Result<Option<String>, WizardError> {
    let mut buf = String::new();
    let n = reader.read_line(&mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    Ok(Some(buf.trim().to_owned()))
}

/// Present `options` to the user as a numbered list, read a 1-based choice,
/// and return the 0-based index. Blank input picks `default_idx`. Invalid
/// input re-prompts. EOF returns [`WizardError::Cancelled`].
fn prompt_choice<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    question: &str,
    options: &[&str],
    default_idx: usize,
) -> Result<usize, WizardError> {
    debug_assert!(default_idx < options.len());
    loop {
        writeln!(writer, "{question}")?;
        for (i, opt) in options.iter().enumerate() {
            let marker = if i == default_idx { ">" } else { " " };
            writeln!(writer, "  {marker} {}) {opt}", i + 1)?;
        }
        write!(writer, "[{}]: ", default_idx + 1)?;
        writer.flush()?;
        let Some(line) = read_line(reader)? else {
            return Err(WizardError::Cancelled);
        };
        if line.is_empty() {
            return Ok(default_idx);
        }
        if let Ok(n) = line.parse::<usize>() {
            if n >= 1 && n <= options.len() {
                return Ok(n - 1);
            }
        }
        writeln!(
            writer,
            "  → please enter a number between 1 and {}, or press enter for the default.\n",
            options.len()
        )?;
    }
}

/// Compose the TOML representation of the user's choices.
///
/// Kept as a `String` rather than a typed `Config` constructor because
/// `Config` is `#[non_exhaustive]` and uses `#[serde(transparent)]` newtypes
/// that can't be field-initialised from outside the schema crate.
/// Round-tripping through [`Config::from_toml`] is the only ergonomic path.
fn build_toml(preset_idx: usize, mode_idx: usize, colors_idx: usize) -> String {
    let mode = match mode_idx {
        1 => "ascii",
        _ => "nerd-font-v3",
    };
    let colors = match colors_idx {
        1 => "true-color",
        2 => "ansi8",
        _ => "ansi256",
    };
    let layout = match preset_idx {
        1 => CLASSIC_LAYOUT,
        2 => MINIMAL_LAYOUT,
        _ => LEAN_LAYOUT,
    };
    format!("schema_version = 1\nmode = \"{mode}\"\ncolors = \"{colors}\"\n\n[layout]\n{layout}\n")
}

const LEAN_LAYOUT: &str = "left = [\"dir\", \"vcs\", \"prompt_char\"]";

const CLASSIC_LAYOUT: &str = "left = [\"os_icon\", \"context\", \"dir\", \"vcs\", \"status\", \"command_execution_time\", \"time\", \"prompt_char\"]";

const MINIMAL_LAYOUT: &str = "left = [\"dir\", \"prompt_char\"]";

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn drive(input: &str) -> (Result<Config, WizardError>, String) {
        let mut output = Vec::new();
        let result = run_with(Cursor::new(input.as_bytes()), &mut output);
        (result, String::from_utf8(output).expect("utf8 prompts"))
    }

    #[test]
    fn picks_defaults_on_blank_answers() {
        let (cfg, _) = drive("\n\n\n");
        let cfg = cfg.expect("default flow must succeed");
        assert_eq!(cfg.mode, p10k_rs_config::Mode::NerdFontV3);
        assert_eq!(cfg.colors, p10k_rs_config::ColorMode::Ansi256);
        let names: Vec<&str> = cfg.layout.left.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(names, &["dir", "vcs", "prompt_char"]);
    }

    #[test]
    fn picks_classic_preset_with_ascii_truecolor() {
        // preset 2 = classic, mode 2 = ascii, colors 2 = truecolor
        let (cfg, _) = drive("2\n2\n2\n");
        let cfg = cfg.expect("classic flow must succeed");
        assert_eq!(cfg.mode, p10k_rs_config::Mode::Ascii);
        assert_eq!(cfg.colors, p10k_rs_config::ColorMode::TrueColor);
        let names: Vec<&str> = cfg.layout.left.iter().map(|r| r.0.as_str()).collect();
        assert!(names.contains(&"os_icon"));
        assert!(names.contains(&"context"));
        assert!(names.contains(&"command_execution_time"));
    }

    #[test]
    fn picks_minimal_preset() {
        // preset 3 = minimal, mode 1 = nerdfont (default), colors 1 = 256 (default)
        let (cfg, _) = drive("3\n\n\n");
        let cfg = cfg.expect("minimal flow must succeed");
        let names: Vec<&str> = cfg.layout.left.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(names, &["dir", "prompt_char"]);
    }

    #[test]
    fn invalid_input_reprompts() {
        // First answer is garbage, then "out of range", then the actual choice.
        let (cfg, prompts) = drive("xyz\n99\n1\n\n\n");
        let cfg = cfg.expect("re-prompt flow must eventually succeed");
        let names: Vec<&str> = cfg.layout.left.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(names, &["dir", "vcs", "prompt_char"]);
        let hint_count = prompts.matches("please enter a number").count();
        assert!(
            hint_count >= 2,
            "expected ≥2 re-prompt hints; got {hint_count}.\nOutput:\n{prompts}"
        );
    }

    #[test]
    fn eof_returns_cancelled() {
        let (cfg, _) = drive("");
        assert!(matches!(cfg, Err(WizardError::Cancelled)));
    }

    #[test]
    fn run_with_writes_intro_and_done_lines() {
        let (_cfg, prompts) = drive("\n\n\n");
        assert!(prompts.contains("Welcome"));
        assert!(prompts.contains("Done"));
        assert!(prompts.contains("style preset"));
        assert!(prompts.contains("Glyph mode"));
        assert!(prompts.contains("Colour palette"));
    }

    #[test]
    fn output_roundtrips_through_to_toml_and_from_toml() {
        let (cfg, _) = drive("\n\n\n");
        let cfg = cfg.unwrap();
        let toml = cfg.to_toml().expect("serialise");
        let parsed = Config::from_toml(&toml).expect("reparse");
        assert_eq!(cfg.layout.left, parsed.layout.left);
        assert_eq!(cfg.mode, parsed.mode);
        assert_eq!(cfg.colors, parsed.colors);
    }
}
