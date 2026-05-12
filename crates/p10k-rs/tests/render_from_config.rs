#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Integration test: rendering from a TOML config matches the factory default.
//!
//! Slice 13's contract is "no-config-file behaviour is byte-identical to
//! today". The cleanest way to test that is:
//!
//! 1. Run the binary with no `P10K_RS_CONFIG` (and an XDG/HOME pointed at
//!    an empty scratch dir, so we don't accidentally pick up the dev's
//!    own config) — capture the baseline output.
//! 2. Run again with `P10K_RS_CONFIG` pointing at a TOML fixture whose
//!    layout matches the factory default — assert byte-identical output.
//!
//! Both invocations run in the same scratch dir under `/tmp` so the
//! `dir` segment renders the same path either way.

use std::path::Path;
use std::process::Command;

/// Locate the freshly-compiled binary from cargo's `CARGO_BIN_EXE_<name>` env.
fn p10k_rs_bin() -> String {
    env!("CARGO_BIN_EXE_p10k-rs").to_owned()
}

/// Run `p10k-rs prompt` in `cwd` with the given env overrides. Returns
/// stdout bytes.
fn run_prompt(cwd: &Path, extra_env: &[(&str, &str)]) -> Vec<u8> {
    let mut child = Command::new(p10k_rs_bin());
    child
        .current_dir(cwd)
        .arg("prompt")
        .arg("--shell")
        .arg("zsh")
        .arg("--last-status")
        .arg("0")
        .arg("--last-duration-ms")
        .arg("0")
        // Make the test deterministic: clear inherited p10k env so the
        // gitstatusd FIFOs from the parent shell don't leak into the
        // sandbox. Set HOME to an empty scratch dir so the loader's
        // ~/.config/p10k-rs/config.toml fallback can't accidentally
        // resolve to a real file on the dev's machine.
        .env_remove("_P10K_RS_GITSTATUSD_REQ")
        .env_remove("_P10K_RS_GITSTATUSD_RESP")
        .env_remove("XDG_CONFIG_HOME");
    for (k, v) in extra_env {
        child.env(k, v);
    }
    let out = child.output().expect("spawn p10k-rs");
    assert!(
        out.status.success(),
        "p10k-rs prompt failed: status={:?} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
    );
    out.stdout
}

/// Build a unique scratch directory under `std::env::temp_dir()` and return
/// it. Caller is responsible for `remove_dir_all` when done.
fn scratch_dir(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!(
        "p10krs-render-from-config-{}-{}-{}",
        label,
        std::process::id(),
        nanos,
    ));
    std::fs::create_dir_all(&p).expect("mkdir scratch");
    p
}

#[test]
fn config_with_default_layout_matches_baseline() {
    let cwd = scratch_dir("baseline-cwd");
    let home = scratch_dir("baseline-home");

    // Baseline: no config file. P10K_RS_CONFIG points at a path that
    // doesn't exist (so we don't pick up the dev's real config), HOME at
    // an empty dir.
    let missing_cfg = home.join("does-not-exist.toml");
    let baseline = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", missing_cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );

    // Same args, but P10K_RS_CONFIG points at a TOML fixture whose layout
    // matches the factory default exactly.
    let fixture = home.join("config.toml");
    std::fs::write(
        &fixture,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"dir\", \"vcs\", \"command_execution_time\", \"status\", \"prompt_char\"]\n",
    )
    .expect("write fixture");

    let from_config = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", fixture.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );

    assert_eq!(
        baseline,
        from_config,
        "byte-identical contract violated: baseline={:?} from_config={:?}",
        String::from_utf8_lossy(&baseline),
        String::from_utf8_lossy(&from_config),
    );

    // Cleanup. Failure here would only ever leak a temp dir; non-fatal.
    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn segment_foreground_override_reaches_render() {
    // End-to-end proof of slice 14: a `[segment.dir].foreground = "red"`
    // entry actually changes the SGR escape the renderer emits. Without
    // this wiring, every styling field in `SegmentConfig` would be
    // silently inert.
    let cwd = scratch_dir("override-cwd");
    let home = scratch_dir("override-home");

    let default_cfg = home.join("default.toml");
    std::fs::write(
        &default_cfg,
        b"schema_version = 1\n[layout]\nleft = [\"dir\"]\n",
    )
    .expect("write default");

    let red_cfg = home.join("red.toml");
    std::fs::write(
        &red_cfg,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"dir\"]\n\
          [segment.dir]\n\
          foreground = \"red\"\n",
    )
    .expect("write red");

    let default_out = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", default_cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    let red_out = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", red_cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );

    // Default ColorMode is Ansi256: blue = \x1b[38;5;4m, red = \x1b[38;5;1m.
    let default_str = String::from_utf8_lossy(&default_out);
    let red_str = String::from_utf8_lossy(&red_out);
    assert!(
        default_str.contains("\x1b[38;5;4m"),
        "default dir should be blue: {default_str:?}"
    );
    assert!(
        red_str.contains("\x1b[38;5;1m"),
        "overridden dir should be red: {red_str:?}"
    );
    assert!(
        !red_str.contains("\x1b[38;5;4m"),
        "overridden dir must not still be blue: {red_str:?}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn unknown_segment_is_skipped_with_warning() {
    let cwd = scratch_dir("unknown-cwd");
    let home = scratch_dir("unknown-home");

    // Two configs: one with only `dir`, one with `dir` plus an unknown
    // name. The unknown should be silently dropped, leaving identical
    // output.
    let cfg_a = home.join("a.toml");
    std::fs::write(&cfg_a, b"schema_version = 1\n[layout]\nleft = [\"dir\"]\n").expect("write a");

    let cfg_b = home.join("b.toml");
    std::fs::write(
        &cfg_b,
        b"schema_version = 1\n[layout]\nleft = [\"dir\", \"not_a_real_segment\"]\n",
    )
    .expect("write b");

    let only_dir = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", cfg_a.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    let with_unknown = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", cfg_b.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );

    assert_eq!(
        only_dir, with_unknown,
        "unknown segment must be skipped, not crash or render"
    );

    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn custom_layout_separator_reaches_render() {
    // Slice 22: `[layout.separators].left` replaces the default single
    // space between segments.
    let cwd = scratch_dir("sep-cwd");
    let home = scratch_dir("sep-home");

    let cfg = home.join("config.toml");
    std::fs::write(
        &cfg,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"dir\", \"prompt_char\"]\n\
          [layout.separators]\n\
          left = \" | \"\n",
    )
    .expect("write fixture");

    let out = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains(" | "),
        "custom separator must appear between segments: {s:?}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn segment_icon_override_replaces_default() {
    // Slice 23: `[segment.<name>].icon` overrides the segment's default
    // Nerd Font glyph. End-to-end proof: the override character flows
    // through Config → sanitize_in_place → segment::render → output.
    let cwd = scratch_dir("icon-cwd");
    let home = scratch_dir("icon-home");

    let cfg = home.join("config.toml");
    std::fs::write(
        &cfg,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"dir\"]\n\
          [segment.dir]\n\
          icon = \"DIR>\"\n",
    )
    .expect("write fixture");

    let out = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    let s = String::from_utf8_lossy(&out);
    assert!(
        s.contains("DIR>"),
        "override icon must appear in output: {s:?}"
    );
    assert!(
        !s.contains('\u{f07b}'),
        "default folder glyph must be replaced: {s:?}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn segment_padding_adds_spaces_around_segment() {
    // Slice 22: `[segment.<name>].padding.left/right` wraps the segment
    // in the requested number of spaces.
    let cwd = scratch_dir("pad-cwd");
    let home = scratch_dir("pad-home");

    let baseline_cfg = home.join("baseline.toml");
    std::fs::write(
        &baseline_cfg,
        b"schema_version = 1\n[layout]\nleft = [\"dir\", \"prompt_char\"]\n",
    )
    .expect("write baseline");

    let padded_cfg = home.join("padded.toml");
    std::fs::write(
        &padded_cfg,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"dir\", \"prompt_char\"]\n\
          [segment.dir]\n\
          padding = { left = 2, right = 3 }\n",
    )
    .expect("write padded");

    let baseline = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", baseline_cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    let padded = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", padded_cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );

    // Padded output should be exactly 5 bytes longer (2 left + 3 right).
    // Bytes, not codepoints — padding emits ASCII spaces; the rest of
    // the prompt is unchanged between the two configs.
    assert_eq!(
        padded.len(),
        baseline.len() + 5,
        "padded len {} != baseline len {} + 5\n  baseline: {:?}\n  padded:   {:?}",
        padded.len(),
        baseline.len(),
        String::from_utf8_lossy(&baseline),
        String::from_utf8_lossy(&padded),
    );

    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}
