#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Integration test: `p10k-rs import <path>` translates a `.p10k.zsh`
//! fixture to TOML and the result round-trips through `Config::from_toml`.

use std::path::Path;
use std::process::Command;

fn p10k_rs_bin() -> String {
    env!("CARGO_BIN_EXE_p10k-rs").to_owned()
}

fn run_import(fixture: &Path) -> (Vec<u8>, Vec<u8>) {
    let out = Command::new(p10k_rs_bin())
        .arg("import")
        .arg(fixture)
        .output()
        .expect("spawn p10k-rs import");
    assert!(
        out.status.success(),
        "import failed: status={:?} stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
    );
    (out.stdout, out.stderr)
}

fn scratch_dir(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!(
        "p10krs-import-{}-{}-{}",
        label,
        std::process::id(),
        nanos,
    ));
    std::fs::create_dir_all(&p).expect("mkdir scratch");
    p
}

#[test]
fn import_translates_common_p10k_config() {
    let dir = scratch_dir("common");
    let fixture = dir.join("sample.p10k.zsh");
    std::fs::write(
        &fixture,
        b"\
            typeset -g POWERLEVEL9K_MODE=nerdfont-complete\n\
            typeset -g POWERLEVEL9K_INSTANT_PROMPT=verbose\n\
            typeset -g POWERLEVEL9K_LEFT_PROMPT_ELEMENTS=(\n\
              os_icon\n\
              dir\n\
              vcs\n\
              =newline\n\
              prompt_char\n\
            )\n\
            typeset -g POWERLEVEL9K_DIR_FOREGROUND=33\n\
            typeset -g POWERLEVEL9K_VCS_CLEAN_FOREGROUND=10\n\
            typeset -g POWERLEVEL9K_VCS_MODIFIED_FOREGROUND=11\n\
        ",
    )
    .expect("write fixture");

    let (stdout, _stderr) = run_import(&fixture);
    let toml = String::from_utf8(stdout).expect("utf8 stdout");

    // Round-trip: imported TOML must reparse cleanly via the loader.
    let cfg = p10k_rs_config::Config::from_toml(&toml).expect("imported TOML must reparse");

    assert_eq!(cfg.mode, p10k_rs_config::Mode::NerdFontV3);
    assert_eq!(
        cfg.instant_prompt,
        p10k_rs_config::InstantPromptMode::Verbose
    );
    let names: Vec<&str> = cfg.layout.left.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(names, &["os_icon", "dir", "vcs", "prompt_char"]);
    let dir_seg = cfg.segments.get("dir").expect("dir segment carried over");
    assert_eq!(dir_seg.foreground, Some(p10k_rs_config::Color::Indexed(33)));
    let vcs = cfg.segments.get("vcs").expect("vcs segment carried over");
    let clean = vcs.states.get("clean").expect("clean state");
    let modified = vcs.states.get("modified").expect("modified state");
    assert_eq!(clean.foreground, Some(p10k_rs_config::Color::Indexed(10)));
    assert_eq!(
        modified.foreground,
        Some(p10k_rs_config::Color::Indexed(11))
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_emits_warnings_for_unknown_variables_to_stderr() {
    let dir = scratch_dir("warns");
    let fixture = dir.join("config.p10k.zsh");
    std::fs::write(
        &fixture,
        b"POWERLEVEL9K_DIR_CLASSES=()\n\
          POWERLEVEL9K_UNRECOGNISED_THING=42\n",
    )
    .expect("write fixture");

    let (_stdout, stderr) = run_import(&fixture);
    let stderr = String::from_utf8(stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("DIR_CLASSES"),
        "missing DIR_CLASSES warning: {stderr:?}"
    );
    assert!(
        stderr.contains("UNRECOGNISED_THING"),
        "missing UNRECOGNISED_THING warning: {stderr:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_handles_missing_file_with_clean_error() {
    let dir = scratch_dir("missing");
    let nonexistent = dir.join("does-not-exist.p10k.zsh");

    let out = Command::new(p10k_rs_bin())
        .arg("import")
        .arg(&nonexistent)
        .output()
        .expect("spawn p10k-rs");
    assert!(!out.status.success(), "should fail on missing file");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("read p10k config"),
        "stderr lacks read-error context: {stderr:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
