#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Integration test: `p10k-rs config check [--config <path>]` (T1.4).
//!
//! The subcommand parses the user's TOML config and exits 0 with a
//! one-line `OK:` message on success, non-zero with the parse / I/O
//! error on stderr otherwise. We exercise both ends of that contract
//! plus the default search-path fallback.

use std::path::Path;
use std::process::Command;

fn p10k_rs_bin() -> String {
    env!("CARGO_BIN_EXE_p10k-rs").to_owned()
}

fn scratch_dir(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!(
        "p10krs-config-check-{}-{}-{}",
        label,
        std::process::id(),
        nanos,
    ));
    std::fs::create_dir_all(&p).expect("mkdir scratch");
    p
}

/// Run `p10k-rs config check [--config <path>]` with `P10K_RS_CONFIG`
/// and `XDG_CONFIG_HOME` cleared so the test is hermetic.
fn run_check(explicit: Option<&Path>, env: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(p10k_rs_bin());
    cmd.arg("config").arg("check");
    if let Some(p) = explicit {
        cmd.arg("--config").arg(p);
    }
    cmd.env_remove("P10K_RS_CONFIG")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("HOME");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("spawn p10k-rs config check")
}

#[test]
fn config_check_valid_toml_exits_zero_with_ok_line() {
    let dir = scratch_dir("valid");
    let fixture = dir.join("good.toml");
    std::fs::write(
        &fixture,
        b"schema_version = 1\n[layout]\nleft = [\"dir\", \"prompt_char\"]\n",
    )
    .expect("write fixture");

    let out = run_check(Some(&fixture), &[]);
    assert!(
        out.status.success(),
        "expected exit 0, got {:?}; stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("OK: "),
        "expected OK: prefix on stdout, got {stdout:?}",
    );
    assert!(
        stdout.contains("parses cleanly"),
        "expected 'parses cleanly' on stdout, got {stdout:?}",
    );
    assert!(
        stdout.contains(fixture.to_str().expect("utf8")),
        "expected fixture path in OK message, got {stdout:?}",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_check_broken_toml_exits_nonzero_with_stderr() {
    let dir = scratch_dir("broken");
    let fixture = dir.join("broken.toml");
    // Unclosed table header — TOML parser rejects with a clean diagnostic.
    std::fs::write(&fixture, b"schema_version = 1\n[layout\nleft = [\"dir\"]\n")
        .expect("write fixture");

    let out = run_check(Some(&fixture), &[]);
    assert!(
        !out.status.success(),
        "expected non-zero exit on broken TOML, got success; stdout={}",
        String::from_utf8_lossy(&out.stdout),
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.is_empty(),
        "expected non-empty stderr on broken TOML, got empty; stdout={}",
        String::from_utf8_lossy(&out.stdout),
    );
    // The error trail should name the file we pointed at so users can
    // jump to it from their terminal.
    assert!(
        stderr.contains(fixture.to_str().expect("utf8")),
        "expected fixture path in stderr, got {stderr:?}",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_check_missing_file_exits_nonzero() {
    // Explicit `--config <path>` to a file that doesn't exist must be a
    // hard error — silently falling through to the default search path
    // would mask a typo on the command line.
    let dir = scratch_dir("missing");
    let nonexistent = dir.join("nope.toml");

    let out = run_check(Some(&nonexistent), &[]);
    assert!(
        !out.status.success(),
        "expected non-zero exit on missing --config path",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.is_empty(),
        "expected non-empty stderr on missing file",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_check_default_path_finds_xdg_config() {
    // No `--config` flag: the binary should walk
    // `$XDG_CONFIG_HOME/p10k-rs/config.toml` and validate that file.
    let xdg = scratch_dir("xdg-config-home");
    let p10k_dir = xdg.join("p10k-rs");
    std::fs::create_dir_all(&p10k_dir).expect("mkdir p10k-rs");
    let cfg = p10k_dir.join("config.toml");
    std::fs::write(&cfg, b"schema_version = 1\n[layout]\nleft = [\"dir\"]\n").expect("write cfg");

    let out = run_check(None, &[("XDG_CONFIG_HOME", xdg.to_str().expect("utf8"))]);
    assert!(
        out.status.success(),
        "expected exit 0 on default search path, got {:?}; stderr={}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("OK: "),
        "expected OK: prefix on stdout, got {stdout:?}",
    );
    assert!(
        stdout.contains(cfg.to_str().expect("utf8")),
        "expected resolved config path in OK message, got {stdout:?}",
    );

    let _ = std::fs::remove_dir_all(&xdg);
}
