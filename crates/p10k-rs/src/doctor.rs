//! `p10k-rs doctor` subcommand (v0.2).
//!
//! Runtime diagnostic that checks the common environmental snags users
//! hit: missing Nerd Font glyphs (`◆` placeholders), missing gitstatusd
//! binary, broken / stale instant-prompt cache permissions, broken
//! TOML config, OSC 7 unsupported terminals, WSL + Windows font story
//! gaps, and shell init not sourced.
//!
//! Output is a list of checks, each with one of `OK` / `WARN` /
//! `ERROR` / `SKIP` and a one-line message. The `--json` form emits a
//! schema-versioned envelope (`p10krs.doctor/v1`) so monitoring /
//! tooling can branch on structured fields. Exit codes:
//!
//! | All `OK` / `SKIP` only         | 0 |
//! | At least one `WARN`, no `ERROR`| 1 |
//! | At least one `ERROR`           | 2 |

use std::fmt::Write as _;
use std::path::PathBuf;

/// Schema identifier emitted by `render_json` so downstream consumers
/// can pin the envelope shape they parse against.
const SCHEMA_ID: &str = "p10krs.doctor/v1";

/// One check's severity. `Skip` means the check could not be evaluated
/// in the current environment (e.g. an OSC 7 probe with no
/// `TERM_PROGRAM` set); it never contributes to the exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    /// Check evaluated and passed.
    Ok,
    /// Check evaluated and the user should look at it, but the prompt
    /// will still render.
    Warn,
    /// Check evaluated and the user almost certainly needs to fix
    /// something (e.g. missing gitstatusd, unparseable config).
    Error,
    /// Check could not be evaluated in this environment.
    Skip,
}

impl Status {
    /// Stable wire string for the text and JSON renderers.
    fn as_wire(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Skip => "SKIP",
        }
    }
}

/// Result of a single check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DoctorCheck {
    /// Stable short name, e.g. `gitstatusd_binary`. Snake_case so
    /// downstream `jq` filters don't need quoting.
    pub(crate) name: &'static str,
    /// Severity. See [`Status`].
    pub(crate) status: Status,
    /// One-line human-readable detail. Wrapping responsibility lives in
    /// the terminal, not here.
    pub(crate) message: String,
}

impl DoctorCheck {
    fn ok(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: Status::Ok,
            message: message.into(),
        }
    }
    fn warn(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: Status::Warn,
            message: message.into(),
        }
    }
    fn error(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: Status::Error,
            message: message.into(),
        }
    }
    fn skip(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: Status::Skip,
            message: message.into(),
        }
    }
}

/// The whole report. Owns the per-check vector + a derived top-level
/// summary so consumers can branch on a single field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DoctorReport {
    /// Checks in the order they ran (stable; the renderer prints them
    /// top-down, scripts can `jq '.checks[]'` in the same order).
    pub(crate) checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    /// Aggregate severity → exit code. Errors beat warnings beat ok;
    /// skips don't move the needle.
    pub(crate) fn exit_code(&self) -> i32 {
        let mut worst = Status::Ok;
        for c in &self.checks {
            worst = match (worst, c.status) {
                (_, Status::Error) => Status::Error,
                (Status::Error, _) => Status::Error,
                (_, Status::Warn) => Status::Warn,
                (Status::Warn, _) => Status::Warn,
                _ => worst,
            };
        }
        match worst {
            Status::Error => 2,
            Status::Warn => 1,
            _ => 0,
        }
    }

    /// Pretty-print to a String. Two columns: 6-char status tag, then
    /// `<name>: <message>`. Stable text format; users may grep it but
    /// scripts should prefer `--json`.
    pub(crate) fn render(&self) -> String {
        let mut out = String::new();
        for c in &self.checks {
            let _ = writeln!(
                out,
                "[{:<5}] {}: {}",
                c.status.as_wire(),
                c.name,
                c.message
            );
        }
        let exit = self.exit_code();
        let summary = match exit {
            0 => "all checks passed",
            1 => "warnings present",
            _ => "errors present",
        };
        let _ = writeln!(out, "doctor: {summary} (exit {exit})");
        out
    }

    /// JSON representation. Hand-rolled per the convention in
    /// `daemon_health.rs` / `verify.rs` — string escaping is the only
    /// non-trivial bit so a serde dep would be overkill.
    pub(crate) fn render_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"schema\":\"");
        out.push_str(SCHEMA_ID);
        out.push_str("\",\"exit_code\":");
        let _ = write!(out, "{}", self.exit_code());
        out.push_str(",\"checks\":[");
        for (i, c) in self.checks.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"name\":\"");
            out.push_str(c.name);
            out.push_str("\",\"status\":\"");
            out.push_str(c.status.as_wire());
            out.push_str("\",\"message\":\"");
            out.push_str(&json_escape(&c.message));
            out.push_str("\"}");
        }
        out.push_str("]}");
        out
    }
}

/// JSON-escape a string for double-quoted output. Mirrors the encoder
/// in `daemon_health.rs` / `verify.rs` so the project keeps one
/// convention.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Probe-side environment accessor. Tests pass a stub so we can drive
/// each check deterministically without touching `std::env` (which
/// would force the ENV_LOCK pattern for every test).
pub(crate) trait Env {
    /// Lookup an env var. Same semantics as `std::env::var().ok()`.
    fn var(&self, key: &str) -> Option<String>;
    /// Lookup an env var as a path. Mirrors `std::env::var_os` →
    /// `PathBuf`. Default impl routes through [`Env::var`] for
    /// simplicity; the live impl reads `OsString` directly.
    fn var_path(&self, key: &str) -> Option<PathBuf> {
        self.var(key).map(PathBuf::from)
    }
    /// Contents of `/proc/sys/kernel/osrelease`, when readable. Routed
    /// through the trait so test stubs aren't fooled by the host's real
    /// `/proc` when probing for WSL.
    fn osrelease(&self) -> Option<String> {
        None
    }
}

/// Live env accessor — defers to `std::env`. Tests use [`MapEnv`].
pub(crate) struct StdEnv;
impl Env for StdEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
    fn var_path(&self, key: &str) -> Option<PathBuf> {
        std::env::var_os(key).map(PathBuf::from)
    }
    fn osrelease(&self) -> Option<String> {
        std::fs::read_to_string("/proc/sys/kernel/osrelease").ok()
    }
}

/// HashMap-backed env stub for tests.
#[cfg(test)]
pub(crate) struct MapEnv(pub std::collections::HashMap<String, String>);
#[cfg(test)]
impl Env for MapEnv {
    fn var(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

// ----- check_* (each is testable with a stub env / scratch paths) -----

/// Heuristic: best-effort Nerd Font glyph coverage probe.
///
/// We cannot read the user's installed font set from a TTY-only Rust
/// process portably. So this probe is structural:
///
/// 1. If `LANG` / `LC_ALL` / `LC_CTYPE` indicate a non-UTF-8 locale,
///    icons cannot render — `Error`.
/// 2. If WSL is detected, we cannot see the Windows-side font list
///    — `Warn`, point at `install-fonts --windows`.
/// 3. Otherwise emit `Skip` with the caveat about the limitation.
///
/// Documented as a heuristic; a `Skip` is not a guarantee fonts are
/// fine. The check exists to flag the obvious failure modes, not to
/// promise universal coverage.
pub(crate) fn check_nerd_font_glyphs<E: Env>(env: &E) -> DoctorCheck {
    let lang = env
        .var("LC_ALL")
        .or_else(|| env.var("LC_CTYPE"))
        .or_else(|| env.var("LANG"))
        .unwrap_or_default();
    let upper = lang.to_ascii_uppercase();
    if !lang.is_empty() && !upper.contains("UTF-8") && !upper.contains("UTF8") {
        return DoctorCheck::error(
            "nerd_font_glyphs",
            format!("non-UTF-8 locale {lang:?}; icons will not render"),
        );
    }
    if is_wsl(env) {
        return DoctorCheck::warn(
            "nerd_font_glyphs",
            "WSL detected — Windows-side font install required; \
             run `p10k-rs install-fonts --windows` or see docs/src/wsl-windows.md",
        );
    }
    DoctorCheck::skip(
        "nerd_font_glyphs",
        "heuristic only — cannot read the OS font registry from the prompt; \
         verify visually that folder / branch glyphs render (not as `◆`)",
    )
}

/// Locate the gitstatusd helper binary and report whether it's
/// present. Re-uses [`p10k_rs_git::locate_gitstatusd`] so the answer
/// matches what the prompt path would discover.
pub(crate) fn check_gitstatusd_binary() -> DoctorCheck {
    match p10k_rs_git::locate_gitstatusd() {
        Some(p) => DoctorCheck::ok("gitstatusd_binary", format!("found at {}", p.display())),
        None => DoctorCheck::error(
            "gitstatusd_binary",
            "no gitstatusd on PATH or via $P10K_RS_GITSTATUSD_BIN — \
             re-run install.sh or set $P10K_RS_GITSTATUSD_BIN",
        ),
    }
}

/// Verify the gitstatusd binary's sha256 against the bundled pin.
/// Skips when the binary isn't located (`check_gitstatusd_binary`
/// already errored on that) or when the host triple isn't pinned.
pub(crate) fn check_gitstatusd_version_pin() -> DoctorCheck {
    let pins = match p10k_rs_git::Pins::load_embedded() {
        Ok(p) => p,
        Err(e) => {
            return DoctorCheck::error(
                "gitstatusd_version_pin",
                format!("embedded pins failed to parse: {e}"),
            );
        }
    };
    let Some(triple) = p10k_rs_git::detect_host_triple() else {
        return DoctorCheck::skip(
            "gitstatusd_version_pin",
            "host triple not in pin file — version cross-check skipped",
        );
    };
    let entry = match pins.for_triple(triple) {
        Ok(e) => e,
        Err(_) => {
            return DoctorCheck::skip(
                "gitstatusd_version_pin",
                format!("no pin entry for triple {triple}"),
            );
        }
    };
    let Some(path) = p10k_rs_git::locate_gitstatusd() else {
        return DoctorCheck::skip(
            "gitstatusd_version_pin",
            "binary not located; cannot hash",
        );
    };
    // Re-use the verify path's stream-hasher for the actual sha256
    // computation. Doing the hash twice (once here, once if the user
    // also runs `verify`) is cheap relative to the user's manual
    // intent and keeps the doctor self-contained.
    match crate::verify::sha256_of_path_pub(&path) {
        Ok(got) if got == entry.binary_sha256 => DoctorCheck::ok(
            "gitstatusd_version_pin",
            format!("sha256 matches pin {} ({triple})", pins.version),
        ),
        Ok(got) => DoctorCheck::warn(
            "gitstatusd_version_pin",
            format!(
                "sha256 differs from pin {} for {triple} (expected={}, got={})",
                pins.version, entry.binary_sha256, got
            ),
        ),
        Err(e) => DoctorCheck::warn(
            "gitstatusd_version_pin",
            format!("could not read {}: {e}", path.display()),
        ),
    }
}

/// Resolve the config file path the prompt path would load. Mirrors
/// `resolve_default_config_path` in `main.rs` but lifted here so the
/// doctor's check operates on the same answer.
fn discover_config_path<E: Env>(env: &E) -> Option<PathBuf> {
    if let Some(p) = env.var_path("P10K_RS_CONFIG") {
        return Some(p);
    }
    if let Some(xdg) = env.var_path("XDG_CONFIG_HOME") {
        let candidate = xdg.join("p10k-rs").join("config.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    if let Some(home) = env.var_path("HOME") {
        let candidate = home.join(".config").join("p10k-rs").join("config.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Is a config file present at any of the standard locations?
pub(crate) fn check_config_file_present<E: Env>(env: &E) -> DoctorCheck {
    match discover_config_path(env) {
        Some(p) => DoctorCheck::ok("config_file_present", format!("found {}", p.display())),
        None => DoctorCheck::warn(
            "config_file_present",
            "no ~/.config/p10k-rs/config.toml — running on factory defaults",
        ),
    }
}

/// Does the user's config (if present) parse?
pub(crate) fn check_config_file_parses<E: Env>(env: &E) -> DoctorCheck {
    let Some(path) = discover_config_path(env) else {
        return DoctorCheck::skip(
            "config_file_parses",
            "no config file present; nothing to validate",
        );
    };
    match p10k_rs_core::Config::load_from_path(&path) {
        Ok(_) => DoctorCheck::ok("config_file_parses", format!("{} parses cleanly", path.display())),
        Err(e) => DoctorCheck::error(
            "config_file_parses",
            format!("{}: {e}", path.display()),
        ),
    }
}

/// Was the per-shell init script sourced? Detects by looking for the
/// markers the shell init exports.
pub(crate) fn check_shell_init_sourced<E: Env>(env: &E) -> DoctorCheck {
    // The zsh init exports `_P10K_RS_GITSTATUSD_PID_FILE` on daemon
    // spawn. If we see it, init was sourced and the daemon channel
    // is up. Outside an interactive zsh, the user may legitimately
    // not need it (e.g. running ad-hoc from a script), so this is a
    // Warn, not Error.
    if env.var("_P10K_RS_GITSTATUSD_PID_FILE").is_some() {
        DoctorCheck::ok(
            "shell_init_sourced",
            "daemon-respawn env vars present; init script is sourced",
        )
    } else {
        DoctorCheck::warn(
            "shell_init_sourced",
            "no `_P10K_RS_GITSTATUSD_*` env vars — run \
             `eval \"$(p10k-rs init zsh)\"` from your shell rc",
        )
    }
}

/// Verify the instant-prompt cache directory is writable and has
/// `0o700` perms (parent) + readable cache file perms. Uses
/// `XDG_CACHE_HOME` or `$HOME/.cache`.
pub(crate) fn check_instant_prompt_cache_writable<E: Env>(env: &E) -> DoctorCheck {
    let base = match env.var_path("XDG_CACHE_HOME") {
        Some(p) => p,
        None => match env.var_path("HOME") {
            Some(h) => h.join(".cache"),
            None => {
                return DoctorCheck::warn(
                    "instant_prompt_cache_writable",
                    "neither XDG_CACHE_HOME nor HOME set; cannot evaluate",
                );
            }
        },
    };
    let dir = base.join("p10k-rs");
    if !dir.exists() {
        return DoctorCheck::ok(
            "instant_prompt_cache_writable",
            format!("{} does not exist yet (will be created on first prompt)", dir.display()),
        );
    }
    match std::fs::metadata(&dir) {
        Ok(meta) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let mode = meta.permissions().mode() & 0o7777;
                // Defensive: 0o700 is the slice-9 ideal; widely-readable
                // (group/world) is a warn, not error — the dump's
                // worst-case payload is a prompt string the attacker
                // could already see at the terminal.
                if mode & 0o077 != 0 {
                    return DoctorCheck::warn(
                        "instant_prompt_cache_writable",
                        format!(
                            "{} has mode {:o}; expected 0o700 (group/world bits should be unset)",
                            dir.display(),
                            mode
                        ),
                    );
                }
            }
            let _ = meta;
            DoctorCheck::ok(
                "instant_prompt_cache_writable",
                format!("{} exists with safe perms", dir.display()),
            )
        }
        Err(e) => DoctorCheck::error(
            "instant_prompt_cache_writable",
            format!("stat {} failed: {e}", dir.display()),
        ),
    }
}

/// Whitelist-based OSC 7 (cwd-reporting) support probe. The shell
/// integration emit path already auto-detects this; the check is
/// informational so users running a non-OSC7 emulator know their
/// directory-tracking features will be muted.
pub(crate) fn check_osc7_supported<E: Env>(env: &E) -> DoctorCheck {
    // Known-supporting emulators per shell-integration probes.
    let supporting = [
        "iTerm.app",
        "WezTerm",
        "Apple_Terminal",
        "vscode",
        "Hyper",
        "ghostty",
    ];
    if let Some(prog) = env.var("TERM_PROGRAM") {
        if supporting.iter().any(|p| prog.eq_ignore_ascii_case(p)) {
            return DoctorCheck::ok("osc7_supported", format!("TERM_PROGRAM={prog} supports OSC 7"));
        }
        return DoctorCheck::warn(
            "osc7_supported",
            format!("TERM_PROGRAM={prog} is not in the known-good OSC 7 whitelist"),
        );
    }
    DoctorCheck::skip(
        "osc7_supported",
        "TERM_PROGRAM not set; cannot infer terminal capabilities",
    )
}

/// WSL + Windows-side font warning. Fires only when WSL is detected;
/// outside WSL the check is `Skip` and contributes nothing.
pub(crate) fn check_wsl_windows_font_warning<E: Env>(env: &E) -> DoctorCheck {
    if !is_wsl(env) {
        return DoctorCheck::skip(
            "wsl_windows_font_warning",
            "not running under WSL — Windows-side font story does not apply",
        );
    }
    DoctorCheck::warn(
        "wsl_windows_font_warning",
        "WSL detected — install MesloLGS NF on the Windows side; \
         run `p10k-rs install-fonts --windows` or see docs/src/wsl-windows.md",
    )
}

/// WSL detection: env vars first (cheap), then `/proc/sys/kernel/osrelease`.
pub(crate) fn is_wsl<E: Env>(env: &E) -> bool {
    if env.var("WSL_DISTRO_NAME").is_some() || env.var("WSL_INTEROP").is_some() {
        return true;
    }
    if let Some(s) = env.osrelease() {
        let lower = s.to_ascii_lowercase();
        if lower.contains("microsoft") || lower.contains("wsl") {
            return true;
        }
    }
    false
}

/// Run every check in stable order and return the report.
pub(crate) fn run_all_checks() -> DoctorReport {
    let env = StdEnv;
    let checks = vec![
        check_nerd_font_glyphs(&env),
        check_gitstatusd_binary(),
        check_gitstatusd_version_pin(),
        check_config_file_present(&env),
        check_config_file_parses(&env),
        check_shell_init_sourced(&env),
        check_instant_prompt_cache_writable(&env),
        check_osc7_supported(&env),
        check_wsl_windows_font_warning(&env),
    ];
    DoctorReport { checks }
}

/// `p10k-rs doctor [--json]` entrypoint. Prints the report and exits
/// with the aggregated severity code (0 / 1 / 2).
pub(crate) fn cmd_doctor(json: bool) -> ! {
    let report = run_all_checks();
    if json {
        println!("{}", report.render_json());
    } else {
        print!("{}", report.render());
    }
    std::process::exit(report.exit_code());
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_with(pairs: &[(&str, &str)]) -> MapEnv {
        let mut m = HashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_owned(), (*v).to_owned());
        }
        MapEnv(m)
    }

    #[test]
    fn status_wire_strings_are_stable() {
        assert_eq!(Status::Ok.as_wire(), "OK");
        assert_eq!(Status::Warn.as_wire(), "WARN");
        assert_eq!(Status::Error.as_wire(), "ERROR");
        assert_eq!(Status::Skip.as_wire(), "SKIP");
    }

    #[test]
    fn exit_code_aggregates_to_worst_severity() {
        let r = DoctorReport {
            checks: vec![
                DoctorCheck::ok("a", "ok"),
                DoctorCheck::skip("b", "skip"),
            ],
        };
        assert_eq!(r.exit_code(), 0);

        let r = DoctorReport {
            checks: vec![
                DoctorCheck::ok("a", "ok"),
                DoctorCheck::warn("b", "warn"),
            ],
        };
        assert_eq!(r.exit_code(), 1);

        let r = DoctorReport {
            checks: vec![
                DoctorCheck::warn("a", "warn"),
                DoctorCheck::error("b", "err"),
            ],
        };
        assert_eq!(r.exit_code(), 2);
    }

    #[test]
    fn render_includes_summary_line() {
        let r = DoctorReport {
            checks: vec![DoctorCheck::ok("a", "fine")],
        };
        let s = r.render();
        assert!(s.contains("[OK   ] a: fine"));
        assert!(s.contains("doctor: all checks passed (exit 0)"));
    }

    #[test]
    fn render_json_round_trips_required_fields() {
        let r = DoctorReport {
            checks: vec![
                DoctorCheck::ok("alpha", "fine"),
                DoctorCheck::warn("beta", "look here"),
            ],
        };
        let s = r.render_json();
        let v: serde_json::Value = serde_json::from_str(&s).expect("json parses");
        assert_eq!(v["schema"], SCHEMA_ID);
        assert_eq!(v["exit_code"], 1);
        let checks = v["checks"].as_array().expect("checks array");
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0]["name"], "alpha");
        assert_eq!(checks[0]["status"], "OK");
        assert_eq!(checks[0]["message"], "fine");
        assert_eq!(checks[1]["status"], "WARN");
    }

    #[test]
    fn render_json_escapes_message_quotes() {
        let r = DoctorReport {
            checks: vec![DoctorCheck::warn("x", r#"path "weird" and \slash"#)],
        };
        let s = r.render_json();
        let v: serde_json::Value = serde_json::from_str(&s).expect("json parses");
        assert_eq!(v["checks"][0]["message"], r#"path "weird" and \slash"#);
    }

    #[test]
    fn nerd_font_check_flags_non_utf8_locale() {
        let env = env_with(&[("LANG", "C")]);
        let c = check_nerd_font_glyphs(&env);
        assert_eq!(c.status, Status::Error);
        assert!(c.message.contains("non-UTF-8"));
    }

    #[test]
    fn nerd_font_check_warns_under_wsl() {
        let env = env_with(&[("LANG", "en_US.UTF-8"), ("WSL_DISTRO_NAME", "Ubuntu")]);
        let c = check_nerd_font_glyphs(&env);
        assert_eq!(c.status, Status::Warn);
        assert!(c.message.contains("WSL"));
    }

    #[test]
    fn nerd_font_check_skips_when_environment_is_inconclusive() {
        let env = env_with(&[("LANG", "en_US.UTF-8")]);
        let c = check_nerd_font_glyphs(&env);
        assert_eq!(c.status, Status::Skip);
    }

    #[test]
    fn osc7_recognises_known_terminals() {
        let env = env_with(&[("TERM_PROGRAM", "iTerm.app")]);
        let c = check_osc7_supported(&env);
        assert_eq!(c.status, Status::Ok);
    }

    #[test]
    fn osc7_warns_on_unknown_terminal() {
        let env = env_with(&[("TERM_PROGRAM", "weird-term-2099")]);
        let c = check_osc7_supported(&env);
        assert_eq!(c.status, Status::Warn);
    }

    #[test]
    fn osc7_skips_when_term_program_unset() {
        let env = env_with(&[]);
        let c = check_osc7_supported(&env);
        assert_eq!(c.status, Status::Skip);
    }

    #[test]
    fn wsl_warning_skips_when_not_wsl() {
        let env = env_with(&[]);
        let c = check_wsl_windows_font_warning(&env);
        assert_eq!(c.status, Status::Skip);
    }

    #[test]
    fn wsl_warning_warns_when_wsl_detected() {
        let env = env_with(&[("WSL_INTEROP", "/run/WSL/12_interop")]);
        let c = check_wsl_windows_font_warning(&env);
        assert_eq!(c.status, Status::Warn);
        assert!(c.message.contains("install-fonts --windows"));
    }

    #[test]
    fn shell_init_check_ok_when_env_present() {
        let env = env_with(&[("_P10K_RS_GITSTATUSD_PID_FILE", "/tmp/pid")]);
        let c = check_shell_init_sourced(&env);
        assert_eq!(c.status, Status::Ok);
    }

    #[test]
    fn shell_init_check_warns_when_env_missing() {
        let env = env_with(&[]);
        let c = check_shell_init_sourced(&env);
        assert_eq!(c.status, Status::Warn);
    }

    #[test]
    fn config_present_check_warns_when_no_config() {
        let env = env_with(&[("HOME", "/nonexistent-doctor-test")]);
        let c = check_config_file_present(&env);
        assert_eq!(c.status, Status::Warn);
    }

    #[test]
    fn config_parses_check_skips_when_no_file() {
        let env = env_with(&[("HOME", "/nonexistent-doctor-test")]);
        let c = check_config_file_parses(&env);
        assert_eq!(c.status, Status::Skip);
    }

    #[test]
    fn instant_prompt_cache_check_ok_when_dir_absent() {
        // Non-existent dir is the steady state for users who've never
        // run an instant-prompt-emitting precmd. Expect `Ok`.
        let env = env_with(&[("HOME", "/nonexistent-doctor-test-xyz")]);
        let c = check_instant_prompt_cache_writable(&env);
        assert_eq!(c.status, Status::Ok);
    }
}
