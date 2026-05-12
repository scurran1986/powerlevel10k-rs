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
    run_prompt_side(cwd, "left", extra_env)
}

/// Run `p10k-rs prompt --render-side <side>` in `cwd` with env overrides.
/// Slice 33 splits PROMPT and RPROMPT across two invocations; tests that
/// care about the right side go through this helper.
fn run_prompt_side(cwd: &Path, side: &str, extra_env: &[(&str, &str)]) -> Vec<u8> {
    let mut child = Command::new(p10k_rs_bin());
    child
        .current_dir(cwd)
        .arg("prompt")
        .arg("--shell")
        .arg("zsh")
        .arg("--render-side")
        .arg(side)
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
    // matches the factory default exactly (slice 38: classic two-sided
    // P10K lean layout).
    let fixture = home.join("config.toml");
    std::fs::write(
        &fixture,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"context\", \"dir\", \"vcs\", \"prompt_char\"]\n\
          right = [\"status\", \"command_execution_time\", \"background_jobs\", \"time\"]\n\
          [layout.frame]\n\
          glyph = \"\xe2\x95\xad\xe2\x94\x80\"\n\
          foreground = \"blue\"\n",
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

#[test]
fn layout_frame_glyph_prefixes_prompt() {
    // Slice 25: `[layout.frame].glyph = "▶"` prefixes the prompt with
    // the glyph + a space, in the configured (or default white) colour.
    let cwd = scratch_dir("frame-cwd");
    let home = scratch_dir("frame-home");

    let cfg = home.join("config.toml");
    std::fs::write(
        &cfg,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"dir\"]\n\
          [layout.frame]\n\
          glyph = \"FRAME\"\n",
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
        s.contains("FRAME"),
        "frame glyph should appear in the prompt: {s:?}"
    );
    // The frame glyph wears the default white SGR; check the wrapped
    // form (zsh-mode `%{…%}` bracketing) appears around it.
    assert!(
        s.contains("\x1b[38;5;7m"),
        "frame must wear the default white SGR: {s:?}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn layout_ruler_glyph_emits_horizontal_line() {
    // Slice 25: `[layout.ruler].glyph = "-"` emits a horizontal divider
    // ABOVE the prompt; the binary fills the line to `$COLUMNS` (or 80
    // fallback) and appends a newline.
    let cwd = scratch_dir("ruler-cwd");
    let home = scratch_dir("ruler-home");

    let cfg = home.join("config.toml");
    std::fs::write(
        &cfg,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"dir\"]\n\
          [layout.ruler]\n\
          glyph = \"-\"\n",
    )
    .expect("write fixture");

    // Force COLUMNS to a known width so the test is deterministic.
    let out = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
            ("COLUMNS", "20"),
        ],
    );
    let s = String::from_utf8_lossy(&out);
    let dashes = "-".repeat(20);
    assert!(
        s.contains(&dashes),
        "ruler must emit COLUMNS=20 dashes: {s:?}"
    );
    // SGR wraps in zsh-mode are `%{…%}` — the ruler colour is default
    // white (ANSI256 7), so the wrap should be present.
    assert!(
        s.contains("\x1b[38;5;7m"),
        "ruler must wear the default white SGR: {s:?}"
    );
    // The ruler ends with a newline; the prompt's first segment must
    // appear AFTER that newline.
    let ruler_newline_idx = s
        .find('\n')
        .expect("ruler must terminate with a newline before the prompt");
    let after_ruler = &s[ruler_newline_idx + 1..];
    assert!(
        !after_ruler.is_empty(),
        "prompt content must follow the ruler line: {s:?}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn segment_show_in_dir_includes_only_matching() {
    // Slice 32: `[segment.<name>].show_in_dir = ["/tmp/**"]` keeps the
    // segment only when the cwd matches at least one glob. Pick the dir
    // segment as a proxy — its folder glyph (U+F07B, slice 23 default)
    // is a clean presence marker.
    let inside_cwd = scratch_dir("show-in-dir-inside");
    let home = scratch_dir("show-in-dir-home");

    // The "outside" cwd must NOT match `/tmp/**`. `scratch_dir` builds
    // under `std::env::temp_dir()` which is typically `/tmp` on Linux,
    // so we'd need somewhere off that tree. Use `home` (a temp dir
    // we control) for the "inside" matched case via an explicit
    // `[segment.dir].show_in_dir = ["<home>/**"]` glob, and use an
    // arbitrary OS root (`/`) as the "outside" run-from path.
    let cfg = home.join("config.toml");
    let allow = format!("{}/**", home.to_string_lossy());
    std::fs::write(
        &cfg,
        format!(
            "schema_version = 1\n\
             [layout]\n\
             left = [\"dir\", \"prompt_char\"]\n\
             [segment.dir]\n\
             show_in_dir = [\"{allow}\"]\n",
        )
        .as_bytes(),
    )
    .expect("write fixture");

    // Cwd matches the glob — dir must render.
    let _ = inside_cwd; // unused: we run from `home` which the glob matches
    let inside_out = run_prompt(
        &home,
        &[
            ("P10K_RS_CONFIG", cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    let inside_str = String::from_utf8_lossy(&inside_out);
    assert!(
        inside_str.contains('\u{f07b}'),
        "dir must render when cwd matches show_in_dir glob: {inside_str:?}"
    );

    // Cwd doesn't match the glob — dir must be skipped. Use `/` as a
    // path that's guaranteed to exist on Linux and is not under `home`.
    let outside_cwd = std::path::Path::new("/");
    let outside_out = run_prompt(
        outside_cwd,
        &[
            ("P10K_RS_CONFIG", cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    let outside_str = String::from_utf8_lossy(&outside_out);
    assert!(
        !outside_str.contains('\u{f07b}'),
        "dir must NOT render when cwd is outside show_in_dir glob: {outside_str:?}"
    );
    // Other segments still survive — the prompt_char `❯` should be there.
    assert!(
        outside_str.contains('\u{276f}'),
        "prompt_char must still render when dir is gated out: {outside_str:?}"
    );

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn segment_disabled_dir_pattern_excludes_matching() {
    // Slice 32: `[segment.<name>].disabled_dir_pattern = "..."` drops the
    // segment when the cwd matches the glob. Build a `secret/` and a
    // `public/` subdir under one scratch root; gate dir off in `secret/`.
    let root = scratch_dir("disabled-dir-pat-root");
    let secret = root.join("secret").join("x");
    let public = root.join("public").join("x");
    std::fs::create_dir_all(&secret).expect("mkdir secret");
    std::fs::create_dir_all(&public).expect("mkdir public");

    let home = scratch_dir("disabled-dir-pat-home");
    let cfg = home.join("config.toml");
    let block = format!("{}/secret/**", root.to_string_lossy());
    std::fs::write(
        &cfg,
        format!(
            "schema_version = 1\n\
             [layout]\n\
             left = [\"dir\", \"prompt_char\"]\n\
             [segment.dir]\n\
             disabled_dir_pattern = \"{block}\"\n",
        )
        .as_bytes(),
    )
    .expect("write fixture");

    // Secret cwd matches the disable glob — dir must be dropped.
    let secret_out = run_prompt(
        &secret,
        &[
            ("P10K_RS_CONFIG", cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    let secret_str = String::from_utf8_lossy(&secret_out);
    assert!(
        !secret_str.contains('\u{f07b}'),
        "dir must NOT render when cwd matches disabled_dir_pattern: {secret_str:?}"
    );
    assert!(
        secret_str.contains('\u{276f}'),
        "prompt_char must still render alongside a gated-out dir: {secret_str:?}"
    );

    // Public cwd doesn't match — dir must render.
    let public_out = run_prompt(
        &public,
        &[
            ("P10K_RS_CONFIG", cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    let public_str = String::from_utf8_lossy(&public_out);
    assert!(
        public_str.contains('\u{f07b}'),
        "dir must render when cwd is outside disabled_dir_pattern: {public_str:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn segment_disabled_field_skips_segment() {
    // End-to-end proof of slice 27: a `[segment.dir].disabled = true`
    // entry removes the segment from the rendered prompt without
    // perturbing the rest of the layout. Distinct from the
    // unknown-segment path — `disabled` is a deliberate opt-in by the
    // user, so it skips silently (no warning) and the layout slot
    // simply collapses.
    let cwd = scratch_dir("disabled-cwd");
    let home = scratch_dir("disabled-home");

    let enabled_cfg = home.join("enabled.toml");
    std::fs::write(
        &enabled_cfg,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"dir\", \"prompt_char\"]\n",
    )
    .expect("write enabled");

    let disabled_cfg = home.join("disabled.toml");
    std::fs::write(
        &disabled_cfg,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"dir\", \"prompt_char\"]\n\
          [segment.dir]\n\
          disabled = true\n",
    )
    .expect("write disabled");

    let enabled_out = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", enabled_cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    let disabled_out = run_prompt(
        &cwd,
        &[
            ("P10K_RS_CONFIG", disabled_cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );

    let enabled_str = String::from_utf8_lossy(&enabled_out);
    let disabled_str = String::from_utf8_lossy(&disabled_out);

    // Folder glyph (U+F07B, slice 23 default for `dir`) must be present
    // when enabled and absent when disabled — that's the proof the gate
    // fired.
    assert!(
        enabled_str.contains('\u{f07b}'),
        "enabled prompt should carry the dir folder glyph: {enabled_str:?}"
    );
    assert!(
        !disabled_str.contains('\u{f07b}'),
        "disabled dir must not render the folder glyph: {disabled_str:?}"
    );

    // The prompt_char segment (slice 1's `❯`) is still in layout.left
    // and must survive — the gate must skip the named segment only.
    assert!(
        disabled_str.contains('\u{276f}'),
        "prompt_char must still render with dir disabled: {disabled_str:?}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn right_prompt_renders_layout_right() {
    // Slice 33: `[layout].right = ["time"]` plus `--render-side right`
    // produces a styled ribbon on stdout. The time segment paints itself
    // on a `brightblack` (ANSI256 8) background and emits an SGR for it,
    // so we assert both that the bg SGR is present (proof the segment
    // ran) and that the mirrored powerline arrow `\u{e0b2}` appears
    // (proof the right-side renderer wove its leading transition).
    let cwd = scratch_dir("right-prompt-cwd");
    let home = scratch_dir("right-prompt-home");

    let cfg = home.join("config.toml");
    std::fs::write(
        &cfg,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"dir\"]\n\
          right = [\"time\"]\n",
    )
    .expect("write fixture");

    let out = run_prompt_side(
        &cwd,
        "right",
        &[
            ("P10K_RS_CONFIG", cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    let s = String::from_utf8_lossy(&out);

    // ANSI256 brightblack = 8. The time segment paints bg=brightblack and
    // surfaces it via SegmentOutput.background, so the right-side renderer
    // also emits the matching SGR on its leading arrow (fg=brightblack).
    assert!(
        s.contains("\x1b[38;5;8m") || s.contains("\x1b[48;5;8m"),
        "right prompt should carry the time segment's brightblack styling: {s:?}"
    );
    // Mirror of the left arrow — the right-prompt-specific glyph.
    assert!(
        s.contains('\u{e0b2}'),
        "right prompt should carry the left-pointing powerline arrow: {s:?}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn transient_prompt_renders_prompt_char_only() {
    // Slice 35: `--render-side transient` returns the collapsed prompt
    // — just the `prompt_char` segment (the `❯`). It must NOT carry the
    // dir glyph (U+F07B), the frame corner (`╭─`), or any ruler.
    let cwd = scratch_dir("transient-cwd");
    let home = scratch_dir("transient-home");

    // Use a fixture that opts in to transient mode so the binary
    // emits the chevron instead of suppressing output. The factory
    // default keeps `transient_prompt = off`, which is correct for
    // existing users but unhelpful for this assertion.
    let cfg = home.join("config.toml");
    std::fs::write(
        &cfg,
        b"schema_version = 1\n\
          transient_prompt = \"always\"\n\
          [layout]\n\
          left = [\"dir\", \"vcs\", \"prompt_char\"]\n\
          [layout.frame]\n\
          glyph = \"\xe2\x95\xad\xe2\x94\x80\"\n",
    )
    .expect("write fixture");

    let out = run_prompt_side(
        &cwd,
        "transient",
        &[
            ("P10K_RS_CONFIG", cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    let s = String::from_utf8_lossy(&out);

    assert!(
        s.contains('\u{276f}'),
        "transient must carry the prompt_char chevron: {s:?}"
    );
    assert!(
        !s.contains('\u{f07b}'),
        "transient must NOT carry the dir folder glyph: {s:?}"
    );
    assert!(
        !s.contains("\u{256d}\u{2500}"),
        "transient must NOT carry the frame top corner '╭─': {s:?}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn layout_frame_bottom_glyph_override() {
    // Slice 40: `[layout.frame].bottom_glyph` replaces the hardcoded
    // `╰─` corner on the multi-line prompt's line 2. Build a layout
    // with `prompt_char` trailing + frame active so the multi-line
    // branch fires, set a distinctive bottom glyph, and assert it
    // shows up AFTER the line-break that introduces line 2.
    let cwd = scratch_dir("frame-bottom-glyph-cwd");
    let home = scratch_dir("frame-bottom-glyph-home");

    let cfg = home.join("config.toml");
    std::fs::write(
        &cfg,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"dir\", \"prompt_char\"]\n\
          [layout.frame]\n\
          glyph = \"TOP>\"\n\
          bottom_glyph = \"BTM>\"\n",
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

    // The configured bottom glyph must appear in the output.
    assert!(
        s.contains("BTM>"),
        "bottom glyph override must appear in the prompt: {s:?}"
    );
    // And it must land AFTER a newline — the multi-line branch fires
    // only when prompt_char trails, and the corner sits on line 2.
    let newline_idx = s
        .find('\n')
        .expect("multi-line prompt must contain a newline before line 2");
    let after_newline = &s[newline_idx + 1..];
    assert!(
        after_newline.contains("BTM>"),
        "bottom glyph must appear AFTER the line-2 newline: {s:?}"
    );
    // The hardcoded default `╰─` must NOT leak through when overridden.
    assert!(
        !s.contains("\u{2570}\u{2500}"),
        "default ╰─ must be replaced by the override: {s:?}"
    );

    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn right_prompt_empty_when_no_layout_right() {
    // Slice 33: when `[layout].right` is absent (or empty), invoking the
    // binary with `--render-side right` must produce no stdout — the zsh
    // init script then assigns `RPROMPT=""` cleanly without any escapes
    // bleeding in. The factory default ships no `layout.right`, so this
    // is the path most users will hit.
    let cwd = scratch_dir("right-empty-cwd");
    let home = scratch_dir("right-empty-home");

    let cfg = home.join("config.toml");
    std::fs::write(
        &cfg,
        b"schema_version = 1\n\
          [layout]\n\
          left = [\"dir\"]\n",
    )
    .expect("write fixture");

    let out = run_prompt_side(
        &cwd,
        "right",
        &[
            ("P10K_RS_CONFIG", cfg.to_str().expect("utf8")),
            ("HOME", home.to_str().expect("utf8")),
        ],
    );
    assert!(
        out.is_empty(),
        "right prompt with no layout.right must be empty: {:?}",
        String::from_utf8_lossy(&out),
    );

    let _ = std::fs::remove_dir_all(&cwd);
    let _ = std::fs::remove_dir_all(&home);
}
