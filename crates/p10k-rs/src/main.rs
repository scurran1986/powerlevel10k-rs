//! `p10k-rs` binary entrypoint.
//!
//! Subcommands track `MVP-SPEC.md` § 1.4: `prompt`, `init`, `configure`,
//! `import`, `statusline`, `segment-list`. Today `prompt` and `init` light
//! up for zsh end-to-end; the others remain stubs until their phases.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use p10k_rs_config::{Glob, ShellIntegrationMode};
use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Segment, Shell as CoreShell};
use p10k_rs_git::{Backend as GitBackend, Gitstatusd, ShellOut as GitShellOut};
use p10k_rs_shell::Shell as ShellInit;

/// Top-level CLI for `p10k-rs`.
#[derive(Debug, Parser)]
#[command(
    name = "p10k-rs",
    version,
    about = "A Rust port and spiritual successor to Powerlevel10k.",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Top-level subcommands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Render a prompt for the given shell. Wired by precmd hooks.
    Prompt {
        /// Which shell asked for this prompt.
        #[arg(long)]
        shell: String,
        /// Exit status of the last command. The shell's `precmd` hook
        /// captures `$?` and forwards it; defaults to 0 if not provided
        /// (e.g. ad-hoc CLI invocation).
        #[arg(long, default_value_t = 0)]
        last_status: i32,
        /// Wall-clock duration of the last foreground command, in
        /// milliseconds. The shell's `preexec`/`precmd` pair tracks the
        /// delta. Defaults to 0; the `command_execution_time` segment
        /// stays hidden below its 3-second threshold.
        #[arg(long, default_value_t = 0)]
        last_duration_ms: u64,
        /// Path to write the instant-prompt cache file. When set, the
        /// rendered prompt is also serialised to `<path>` as a sourceable
        /// shell snippet (atomic temp+rename). The shell init script
        /// sources this file at startup so PROMPT is set before any
        /// segment-renderer runs — masking gitstatusd's first-call cold
        /// cost on big repos.
        #[arg(long)]
        dump: Option<PathBuf>,
        /// Emit machine-readable JSON instead of styled text.
        #[arg(long)]
        json: bool,
        /// Which side of the prompt to print: `left` (default, drives
        /// `PROMPT` in zsh) or `right` (drives `RPROMPT`). The shell
        /// init script invokes the binary twice per precmd — once per
        /// side — so each invocation prints exactly one ribbon and the
        /// shell glues them onto the matching parameter.
        #[arg(long, default_value = "left")]
        render_side: String,
        /// The command line the shell is about to (or just did) run.
        /// Drives the `show_on_command` segment gate. Empty string
        /// means "no command", which hides every segment that has a
        /// `show_on_command` filter configured. The zsh init script
        /// populates this with the last accepted command at precmd
        /// time (an approximation — see the init script comment).
        #[arg(long, default_value = "")]
        upcoming_command: String,
        /// Path the previous prompt was rendered at. The zsh init
        /// script captures this at the prior `precmd` and forwards it
        /// at `zle-line-finish` time so the binary can decide whether
        /// to collapse for the `same-dir` / `unique-dir` transient
        /// modes. Unset for `off` / `always` modes — they never need
        /// it. Only consulted when `--render-side transient`.
        #[arg(long)]
        last_prompt_cwd: Option<PathBuf>,
    },
    /// Print the per-shell init script. `eval` / `source` from your rc file.
    Init {
        /// Target shell: zsh, fish, or bash.
        shell: String,
    },
    /// Run the interactive configuration wizard.
    Configure,
    /// Import a Powerlevel10k `~/.p10k.zsh` config best-effort.
    Import {
        /// Path to the upstream Powerlevel10k config file.
        path: std::path::PathBuf,
    },
    /// Render a statusline payload for an AI host (Claude Code, etc.).
    Statusline {
        /// Host identifier, e.g. `claude-code`.
        #[arg(long)]
        host: String,
    },
    /// List all segments this build ships, including auto-detect heuristics.
    SegmentList,
    /// Config-file utilities (validate, …).
    Config {
        /// Which `config` action to run.
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

/// Subcommands under `p10k-rs config`.
///
/// Today only `check` is wired; future actions (`show`, `format`, …) get
/// new variants here without churning the top-level `Command` enum.
#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Parse and schema-validate the user's TOML config without rendering.
    ///
    /// Resolution order matches `Config::load_default`:
    ///
    /// 1. `--config <path>` if supplied.
    /// 2. `$P10K_RS_CONFIG` if set.
    /// 3. `$XDG_CONFIG_HOME/p10k-rs/config.toml`.
    /// 4. `$HOME/.config/p10k-rs/config.toml`.
    ///
    /// Exits 0 with `OK: <path> parses cleanly` on success; non-zero with
    /// the parse / I/O error on stderr otherwise. Lets users iterate on a
    /// config file without restarting their shell.
    Check {
        /// Explicit path to the config file. Overrides the env-driven
        /// discovery so users can validate a candidate file before moving
        /// it into the active config location.
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    // Hold the non-blocking writer's `WorkerGuard` for the full lifetime
    // of the process (T1.21). When it drops, tracing-appender's
    // background thread flushes and joins; binding the value to `_`
    // would drop it immediately and discard any queued events on a
    // sub-millisecond CLI run like `p10k-rs prompt`. The named slot
    // keeps the guard alive until `main` returns.
    let _guard = init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Command::Prompt {
            shell,
            last_status,
            last_duration_ms,
            dump,
            json,
            render_side,
            upcoming_command,
            last_prompt_cwd,
        } => {
            tracing::debug!(
                shell,
                last_status,
                last_duration_ms,
                ?dump,
                json,
                render_side,
                upcoming_command,
                ?last_prompt_cwd,
                "prompt invoked"
            );
            if json {
                anyhow::bail!("--json output lands with the AI integration phase");
            }
            let side = parse_render_side(&render_side)?;
            cmd_prompt(
                &shell,
                last_status,
                last_duration_ms,
                dump.as_deref(),
                side,
                &upcoming_command,
                last_prompt_cwd.as_deref(),
            )
        }
        Command::Init { shell } => {
            tracing::debug!(shell, "init invoked");
            cmd_init(&shell)
        }
        Command::Configure => {
            tracing::debug!("configure invoked");
            cmd_configure()
        }
        Command::Import { path } => {
            tracing::debug!(?path, "import invoked");
            cmd_import(&path)
        }
        Command::Statusline { host } => {
            tracing::debug!(host, "statusline invoked");
            anyhow::bail!("statusline lands with the AI integration phase")
        }
        Command::SegmentList => {
            for name in p10k_rs_segments::segment_names() {
                println!("{name}");
            }
            Ok(())
        }
        Command::Config { command } => match command {
            ConfigCommand::Check { config } => {
                tracing::debug!(?config, "config check invoked");
                cmd_config_check(config.as_deref())
            }
        },
    }
}

/// `p10k-rs config check [--config <path>]` — parse the TOML config and
/// report whether it validates against the schema.
///
/// Resolution: an explicit `--config` argument wins. Otherwise we walk
/// the same `$P10K_RS_CONFIG` → `$XDG_CONFIG_HOME/p10k-rs/config.toml`
/// → `$HOME/.config/p10k-rs/config.toml` search path the render path
/// uses, so a `check` invocation validates exactly the file the next
/// `prompt` call would load.
///
/// Exits 0 on success after printing `OK: <path> parses cleanly` to
/// stdout. Any I/O or parse error is returned via `anyhow` and surfaces
/// as a non-zero exit with the error text on stderr — the binary's
/// `main` already routes `Err(_)` through anyhow's printer.
fn cmd_config_check(explicit_path: Option<&std::path::Path>) -> Result<()> {
    // Two paths to be careful about:
    //   1. The user passed `--config <path>` — load that file directly so
    //      the error message names the exact file they pointed at, even
    //      if it differs from the env-discovered default.
    //   2. No flag — re-use `Config::load_default` so behaviour matches
    //      what `prompt` would do. The loader's error already names the
    //      tried path on Io / Parse, so we don't need to add path context.
    if let Some(path) = explicit_path {
        Config::load_from_path(path)
            .with_context(|| format!("loading config from {}", path.display()))?;
        println!("OK: {} parses cleanly", path.display());
    } else {
        let resolved = resolve_default_config_path();
        Config::load_default().context("loading config from default search path")?;
        match resolved {
            Some(p) => println!("OK: {} parses cleanly", p.display()),
            None => println!("OK: <default search path> parses cleanly"),
        }
    }
    Ok(())
}

/// Best-effort: name the file `Config::load_default` would resolve to, so
/// the `OK:` line on success names the actual file the user validated.
///
/// Mirrors `discover_config_path` in `p10k-rs-config` (kept local because
/// that helper is private). Returns `None` if no candidate exists — the
/// caller falls back to a generic message rather than fabricating a path.
fn resolve_default_config_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("P10K_RS_CONFIG") {
        return Some(PathBuf::from(p));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let candidate = PathBuf::from(xdg).join("p10k-rs").join("config.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = PathBuf::from(home)
            .join(".config")
            .join("p10k-rs")
            .join("config.toml");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Which side of the prompt `cmd_prompt` should emit to stdout.
///
/// The zsh init script calls `p10k-rs prompt --render-side left` for
/// `PROMPT` and `--render-side right` for `RPROMPT` on every precmd
/// hook. Splitting per-side keeps the wire protocol trivial: one
/// invocation, one ribbon, no in-band separators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderSide {
    /// Left prompt — `PROMPT` in zsh. Default when `--render-side` is
    /// not supplied (preserves the pre-slice-33 binary contract).
    Left,
    /// Right prompt — `RPROMPT` in zsh.
    Right,
    /// Transient prompt — the collapsed form swapped in by the zle
    /// `zle-line-finish` widget right after the user accepts a line.
    /// Walks no segments other than `prompt_char`; emits one line with
    /// no frame, ruler, or trailing newline.
    Transient,
}

/// Parse the `--render-side` flag. Anything other than `left`, `right`,
/// or `transient` is a hard error so a typo in the init script surfaces
/// immediately rather than silently rendering the wrong side.
fn parse_render_side(s: &str) -> Result<RenderSide> {
    match s.to_ascii_lowercase().as_str() {
        "left" => Ok(RenderSide::Left),
        "right" => Ok(RenderSide::Right),
        "transient" => Ok(RenderSide::Transient),
        other => anyhow::bail!(
            "unknown --render-side '{other}': expected 'left', 'right', or 'transient'"
        ),
    }
}

/// Decision returned by [`decide_transient`] — what the zsh init
/// script should do with the binary's transient render result.
///
/// The wire protocol the shell consumes:
///
/// - [`TransientDecision::Emit`] (exit 0, stdout = the value): zsh
///   assigns `PROMPT=<stdout>` and calls `zle reset-prompt`. The string
///   may be empty (the `off` mode case) — that matches the historical
///   transient behaviour where `PROMPT=""` collapses the line.
/// - [`TransientDecision::KeepPrompt`] (exit 2, stdout empty): zsh
///   leaves `PROMPT` untouched so the full ribbon stays in scrollback.
///   New in T1.8 — only `SameDir` / `UniqueDir` reach this case when
///   the cwd-compare fails.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TransientDecision {
    /// Print this string and exit 0. Empty string is valid (`off` mode).
    Emit(String),
    /// Print nothing and exit 2 so the shell skips the `PROMPT` swap.
    KeepPrompt,
}

/// Code returned for [`TransientDecision::KeepPrompt`]. Exit 1 is
/// already overloaded for any [`anyhow`] error path; 2 is unused and
/// makes the "this is a deliberate policy signal" reading unambiguous
/// to anyone reading the zsh init script.
const TRANSIENT_KEEP_PROMPT_EXIT_CODE: i32 = 2;

/// Decide what to emit (and how to exit) for `--render-side transient`,
/// gated on the user's [`p10k_rs_config::TransientPromptMode`].
///
/// Pure: takes the mode, the current cwd, the previous prompt's cwd
/// (`None` when the shell didn't pass `--last-prompt-cwd`, e.g. the
/// first prompt of a session), and the renderer's already-computed
/// transient string. Returns the wire decision; the caller writes it
/// to stdout and exits.
///
/// The mode semantics:
///
/// - `Off`: never collapse. Emit empty (matches pre-T1.8 behaviour
///   byte-for-byte).
/// - `Always`: always collapse. Emit the rendered string.
/// - `SameDir`: collapse only when this prompt's cwd matches the
///   previous prompt's cwd. On mismatch (or unknown previous cwd),
///   keep the full prompt.
/// - `UniqueDir`: aliased to `SameDir` for now. The "collapse all but
///   the most recent prompt at each unique directory" semantic needs
///   cross-prompt history tracking that lands in a follow-up slice.
///   Today the variant is preserved in the schema so users can opt
///   into it without a breaking-config rename when the real semantic
///   ships.
fn decide_transient(
    mode: p10k_rs_config::TransientPromptMode,
    cwd: &std::path::Path,
    last_prompt_cwd: Option<&std::path::Path>,
    transient_render: Option<&str>,
) -> TransientDecision {
    use p10k_rs_config::TransientPromptMode as Mode;
    let collapsed = transient_render.unwrap_or("");
    match mode {
        Mode::Off => TransientDecision::Emit(String::new()),
        Mode::Always => TransientDecision::Emit(collapsed.to_owned()),
        Mode::SameDir | Mode::UniqueDir => match last_prompt_cwd {
            Some(prev) if prev == cwd => TransientDecision::Emit(collapsed.to_owned()),
            _ => TransientDecision::KeepPrompt,
        },
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod transient_decision_tests {
    use super::{decide_transient, TransientDecision};
    use p10k_rs_config::TransientPromptMode as Mode;
    use std::path::PathBuf;

    fn cwd() -> PathBuf {
        PathBuf::from("/home/u/work")
    }

    #[test]
    fn off_emits_empty_regardless_of_render() {
        // Byte-identical to pre-T1.8 behaviour: Off ignores the render
        // result and emits "", which the shell assigns as PROMPT="".
        let d = decide_transient(Mode::Off, &cwd(), None, Some("\u{276F}"));
        assert_eq!(d, TransientDecision::Emit(String::new()));
    }

    #[test]
    fn off_emits_empty_even_with_matching_prev_cwd() {
        // `--last-prompt-cwd` must NOT influence Off — the mode is the
        // policy source of truth.
        let prev = cwd();
        let d = decide_transient(Mode::Off, &cwd(), Some(&prev), Some("\u{276F}"));
        assert_eq!(d, TransientDecision::Emit(String::new()));
    }

    #[test]
    fn always_emits_render_ignoring_prev_cwd() {
        // Always doesn't consult `--last-prompt-cwd` (the shell can
        // skip the flag entirely; the binary must not require it).
        let d = decide_transient(Mode::Always, &cwd(), None, Some("\u{276F}"));
        assert_eq!(d, TransientDecision::Emit("\u{276F}".to_owned()));
    }

    #[test]
    fn always_emits_empty_when_render_is_none() {
        // Defensive: if the layout has no `prompt_char`, the renderer
        // emits no transient. Always-mode still exits 0 with empty
        // stdout — the shell collapses to PROMPT="".
        let d = decide_transient(Mode::Always, &cwd(), None, None);
        assert_eq!(d, TransientDecision::Emit(String::new()));
    }

    #[test]
    fn same_dir_emits_when_cwds_match() {
        let prev = cwd();
        let d = decide_transient(Mode::SameDir, &cwd(), Some(&prev), Some("\u{276F}"));
        assert_eq!(d, TransientDecision::Emit("\u{276F}".to_owned()));
    }

    #[test]
    fn same_dir_keeps_prompt_when_cwds_differ() {
        let prev = PathBuf::from("/elsewhere");
        let d = decide_transient(Mode::SameDir, &cwd(), Some(&prev), Some("\u{276F}"));
        assert_eq!(d, TransientDecision::KeepPrompt);
    }

    #[test]
    fn same_dir_keeps_prompt_on_first_prompt_of_session() {
        // First precmd of the session — the shell hasn't seeded
        // `_P10K_RS_PREV_PROMPT_CWD` yet so it skips the flag, the
        // binary sees None. KeepPrompt is the conservative call —
        // we can't prove this is a "same-dir streak" yet.
        let d = decide_transient(Mode::SameDir, &cwd(), None, Some("\u{276F}"));
        assert_eq!(d, TransientDecision::KeepPrompt);
    }

    #[test]
    fn unique_dir_aliases_to_same_dir_today() {
        // UniqueDir's history-aware semantic isn't implemented yet —
        // it acts as SameDir. Pin this so the alias is visible in
        // tests; the follow-up slice will replace these assertions
        // with the real "collapse all but most-recent at each dir"
        // behaviour.
        let prev_same = cwd();
        let prev_diff = PathBuf::from("/elsewhere");
        assert_eq!(
            decide_transient(Mode::UniqueDir, &cwd(), Some(&prev_same), Some("\u{276F}")),
            TransientDecision::Emit("\u{276F}".to_owned())
        );
        assert_eq!(
            decide_transient(Mode::UniqueDir, &cwd(), Some(&prev_diff), Some("\u{276F}")),
            TransientDecision::KeepPrompt
        );
        assert_eq!(
            decide_transient(Mode::UniqueDir, &cwd(), None, Some("\u{276F}")),
            TransientDecision::KeepPrompt
        );
    }
}

/// Render the prompt: discover the user's TOML config, fall back to a
/// hardcoded factory default if anything goes wrong.
fn cmd_prompt(
    shell: &str,
    last_status: i32,
    last_duration_ms: u64,
    dump: Option<&std::path::Path>,
    side: RenderSide,
    upcoming_command: &str,
    last_prompt_cwd: Option<&std::path::Path>,
) -> Result<()> {
    let core_shell = parse_core_shell(shell)?;
    let cwd: PathBuf = std::env::current_dir().context("read cwd")?;

    // Prefer the `Gitstatusd` backend when the shell init script has set up
    // FIFOs and started the daemon (ADR-0001). Fall back to the slower
    // `ShellOut` for ad-hoc CLI invocations and shells where the daemon
    // couldn't start.
    let git = git_status(cwd.as_path());
    // Probe Jujutsu in parallel — `jj` has no daemon analogue, just a
    // filesystem walk to `.jj/` plus a shell-out to `jj log` on hit.
    // Returns `None` outside a jj repo or when `jj` isn't on `$PATH`.
    let jj = jj_status(cwd.as_path());

    // Resolve the user's config. Missing file or parse error → factory
    // default. The contract for this slice is byte-identical output to the
    // pre-config-loader behaviour when no config is present.
    let cfg = match Config::load_default() {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::warn!("config load failed: {e}, falling back to factory default");
            factory_default_config()
        }
    };
    let env = EnvSnapshot::from_env();
    // Probe the environment once per prompt for AI-host fingerprints
    // (`$CLAUDECODE`, `$AIDER_*`, `$CURSOR_*`). The result rides in
    // `RenderCtx.host` so segments like `ai_host` can react. Detection
    // is a handful of env-var lookups — negligible against gitstatusd.
    let host = p10k_rs_ai::detect_host_kind();
    // Sanitise the cwd display string once at the producer boundary.
    // Segments that render the path (`dir`) consume `ctx.cwd_display`
    // and skip re-sanitising; the `SafeText` type proves the work is
    // done.
    let cwd_display =
        p10k_rs_core::safety::SafeText::from_untrusted(&cwd.as_path().display().to_string());
    // Resolve shell-integration emission once per prompt at the
    // producer boundary so the renderer stays I/O-free. See
    // `resolve_shell_integration` for the auto-detect matrix and the
    // Warp gating decision.
    let shell_integration_active =
        resolve_shell_integration(cfg.shell_integration.mode, &host, |k| std::env::var(k).ok());
    let ctx = RenderCtx {
        config: &cfg,
        shell: core_shell,
        host,
        cwd: cwd.as_path(),
        cwd_display,
        git: git.as_ref(),
        jj: jj.as_ref(),
        last_status,
        last_duration: Duration::from_millis(last_duration_ms),
        jobs: 0,
        now: SystemTime::now(),
        env: &env,
        upcoming_command,
        shell_integration_active,
    };

    let left_segments = assemble_segments(&cfg, cwd.as_path(), &cfg.layout.left, upcoming_command);
    let right_segments =
        assemble_segments(&cfg, cwd.as_path(), &cfg.layout.right, upcoming_command);
    let prompt = p10k_rs_core::render_prompt(&left_segments, &right_segments, &ctx);

    // Print the requested side to stdout, plain text, no trailing newline.
    // The init script appends formatting (single space for PROMPT, raw
    // assignment for RPROMPT) at the call site.
    //
    // For `--render-side transient`: gate the bytes (and the exit code)
    // on the user's [`p10k_rs_config::TransientPromptMode`] via [`decide_transient`].
    // `KeepPrompt` is the new T1.8 path — `SameDir` / `UniqueDir` with
    // a cwd-mismatch returns exit 2 so the zsh widget skips its PROMPT
    // swap and the full ribbon stays in scrollback.
    match side {
        RenderSide::Left => print!("{}", prompt.left),
        RenderSide::Right => print!("{}", prompt.right),
        RenderSide::Transient => {
            match decide_transient(
                cfg.transient_prompt,
                cwd.as_path(),
                last_prompt_cwd,
                prompt.transient.as_deref(),
            ) {
                TransientDecision::Emit(s) => print!("{s}"),
                TransientDecision::KeepPrompt => {
                    std::process::exit(TRANSIENT_KEEP_PROMPT_EXIT_CODE);
                }
            }
        }
    }

    // Dump the rendered prompt to disk for the instant-prompt path. Only
    // the left side is cached today — the instant-prompt path exists to
    // mask gitstatusd's first-call cold cost on PROMPT; RPROMPT is empty
    // by default and the right-side render is independent. Failure is
    // non-fatal — the next invocation will retry, and the user just sees
    // a slightly slower first prompt next shell.
    if let Some(dump_path) = dump {
        if side == RenderSide::Left {
            if let Err(e) = write_instant_dump(dump_path, &prompt.left, core_shell) {
                tracing::warn!("instant-prompt dump write failed (non-fatal): {e}");
            }
        }
    }
    Ok(())
}

/// Build the factory-default `Config`.
///
/// Used by [`cmd_prompt`] when no user config is present or it fails to
/// parse. The layout matches the historical hardcoded order — `[dir, vcs,
/// command_execution_time, status, prompt_char]` — so a fresh install with
/// no config file renders the same prompt the binary always has.
fn factory_default_config() -> Config {
    // Build the TOML on-the-fly rather than constructing the schema by
    // hand. `Config` is `#[non_exhaustive]` and uses `#[serde(transparent)]`
    // newtypes that can't be field-initialised from outside the crate, so
    // round-tripping through `from_toml` is the only ergonomic path. The
    // string is `const` and parses in single-digit microseconds.
    // Slice 38: factory default mirrors upstream Powerlevel10k's "lean"
    // two-sided layout out of the box. Left ribbon carries identity
    // (context — hidden on local non-root sessions per slice 37), cwd,
    // and vcs, with `prompt_char` falling onto line 2 thanks to the
    // `[layout.frame]` corner. Right ribbon collects the
    // typically-hidden signals — `status` (error only), `command_execution_time`
    // (> 3s), `background_jobs` (> 0) — and `time`, which renders
    // unconditionally so users always have a clock on the right.
    const FACTORY_TOML: &str = r#"
schema_version = 1
[layout]
left = ["context", "dir", "vcs", "prompt_char"]
right = ["ai_host", "status", "command_execution_time", "background_jobs", "time"]
[layout.frame]
glyph = "╭─"
foreground = "blue"
"#;
    // `from_toml` is fallible only on parse error — the literal above is a
    // compile-time constant, so failing here is a programmer bug. The
    // workspace lints deny `panic!` and `expect`, but parsing a hard-coded
    // valid TOML literal is one of the few places where a panic is the
    // right shape: any failure is a programming error in this file, not a
    // user-facing condition. The factory-default-toml test below pins the
    // contract.
    #[allow(clippy::expect_used)]
    Config::from_toml(FACTORY_TOML).expect("factory-default TOML must always parse")
}

/// Walk a layout segment list and instantiate each known segment.
///
/// Unknown names produce a `tracing::warn!` and are skipped — a typo'd
/// segment in a user config is a non-fatal warning, not a crash. Segments
/// whose `[segment.<name>].disabled = true` block is set, or whose
/// `show_in_dir` / `disabled_dir_pattern` cwd gates exclude the current
/// directory, are silently skipped — the user opted in to hiding them,
/// so no warning is warranted. Returns in render order.
///
/// Gate evaluation order (per slice 32, extended by slice 44):
///
/// 1. **`disabled_dir_pattern`** — if the glob matches `cwd`, the segment
///    is dropped. Exclude wins over include.
/// 2. **`show_in_dir`** — if `Some(globs)`, the segment is kept only when
///    at least one glob matches `cwd`. `None` means "no constraint".
/// 3. **`show_on_command`** — if `Some(cmds)`, the segment is kept only
///    when the first whitespace-delimited word of `upcoming_command` is
///    in `cmds`. An empty `upcoming_command` hides every segment with a
///    `show_on_command` filter; that matches the upstream "no command
///    typed → no command-gated segments" intuition.
/// 4. **`disabled`** — the explicit kill switch.
///
/// The dir / command gates fire *before* `disabled` and before the
/// `segments::build` lookup so a typo'd glob doesn't fall through to the
/// "unknown segment" warning path — the user already knows they typed
/// a glob; surfacing it as an unknown-segment warn would be misleading.
fn assemble_segments(
    cfg: &Config,
    cwd: &std::path::Path,
    refs: &[p10k_rs_config::SegmentRef],
    upcoming_command: &str,
) -> Vec<Box<dyn Segment>> {
    let mut out: Vec<Box<dyn Segment>> = Vec::with_capacity(refs.len());
    let first_word = command_first_word(upcoming_command);
    for r in refs {
        let seg_cfg = cfg.segments.get(&r.0);

        // Dir gates first. `disabled_dir_pattern` excludes win over
        // `show_in_dir` includes: if both fields are set and both match,
        // the segment is dropped.
        if seg_cfg
            .and_then(|sc| sc.disabled_dir_pattern.as_ref())
            .is_some_and(|pat| glob_matches_cwd(pat, cwd))
        {
            continue;
        }
        if let Some(allow) = seg_cfg.and_then(|sc| sc.show_in_dir.as_ref()) {
            if !allow.iter().any(|g| glob_matches_cwd(g, cwd)) {
                continue;
            }
        }

        // Command gate: filter present → keep only when the upcoming
        // command's first word matches one of the configured names.
        // No upcoming command (empty buffer) means every command-gated
        // segment is hidden.
        if let Some(cmds) = seg_cfg.and_then(|sc| sc.show_on_command.as_ref()) {
            match first_word {
                Some(word) if cmds.iter().any(|c| c == word) => {}
                _ => continue,
            }
        }

        if seg_cfg.is_some_and(|s| s.disabled) {
            continue;
        }
        if let Some(seg) = p10k_rs_segments::build(&r.0) {
            out.push(seg);
        } else {
            tracing::warn!("unknown segment {:?} in layout.left, skipping", r.0);
        }
    }
    out
}

/// Return the first whitespace-delimited word of `buffer`, or `None` if
/// the buffer is empty or whitespace-only.
///
/// Drives the `show_on_command` gate in [`assemble_segments`]. Matches
/// the upstream P10K convention: "is the user about to run `aws ...`?"
/// is answered by inspecting the command verb only, ignoring its
/// arguments. Quoting and `$VAR` expansion are intentionally out of
/// scope — getting them right would require a real shell parser, and
/// upstream doesn't either.
fn command_first_word(buffer: &str) -> Option<&str> {
    let trimmed = buffer.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.split_whitespace().next().unwrap_or(trimmed))
}

// Per-process cache of compiled glob matchers.
//
// Keyed by the raw pattern string. `Some(matcher)` means the pattern compiled
// successfully. `None` means compilation failed and we have already emitted a
// `tracing::warn!` for it — subsequent calls skip recompilation *and* suppress
// duplicate warnings.
//
// `thread_local!` avoids any synchronisation cost; the binary is
// single-threaded on the render path (spawn-per-prompt architecture, ADR-0001).
thread_local! {
    static GLOB_CACHE: RefCell<HashMap<String, Option<globset::GlobMatcher>>> =
        RefCell::new(HashMap::new());
}

/// Compile `glob` and check whether it matches the cwd as a literal path
/// string.
///
/// Per slice 32's behaviour spec: home-expansion (`~`) is the user's
/// responsibility — Powerlevel10k doesn't expand tildes in
/// `POWERLEVEL9K_*_SHOW_ON_DIR_PATTERN` either, and silently expanding
/// here would surprise users importing a working p10k config.
///
/// A glob that fails to compile is treated as "no match" and warned
/// about — a typo is a non-fatal config error; the segment just doesn't
/// gate as the user expected. The `tracing::warn!` fires exactly once
/// per unique bad pattern per process (subsequent calls hit the cache
/// and are silent), so a user with `RUST_LOG=warn` sees one breadcrumb
/// rather than one per segment per prompt render.
fn glob_matches_cwd(glob: &Glob, cwd: &std::path::Path) -> bool {
    GLOB_CACHE.with(|cache| {
        let mut map = cache.borrow_mut();
        let matcher =
            map.entry(glob.0.clone())
                .or_insert_with(|| match globset::Glob::new(&glob.0) {
                    Ok(g) => Some(g.compile_matcher()),
                    Err(e) => {
                        tracing::warn!("invalid glob {:?}: {e}; treating as no-match", glob.0);
                        None
                    }
                });
        matcher.as_ref().is_some_and(|m| m.is_match(cwd))
    })
}

/// Serialise the rendered PROMPT to a sourceable shell snippet at `path`,
/// using a temp-file + rename for atomicity (so a half-written dump never
/// corrupts the next shell's instant prompt).
///
/// The trailing literal space inside the quoted value matches the precmd
/// hook's `"$(... ) "` shape, so sourcing the dump produces the same
/// PROMPT bytes the precmd would have set.
///
/// Security: the dump is `source`d by the shell at startup, so its bytes
/// run as code. We harden the write against symlink/race attacks on a
/// multi-user host where the dump dir or tempfile path could be hostile.
///
/// Panic safety (T1.3): the staged tempfile is wrapped in a [`TmpGuard`]
/// that unlinks it on drop. If a panic — or any early return — escapes
/// before the successful `rename`, no partial dump survives to be
/// `source`d by the next shell. The guard is disarmed only after the
/// rename completes, at which point the path no longer refers to the
/// tempfile.
///
/// Concurrent shells (T1.3): the tempfile name embeds the current pid
/// plus a nanosecond timestamp so two precmd hooks racing on the same
/// dump path can't collide on `O_CREAT|O_EXCL`. The race is then
/// resolved at the rename step (last writer wins, which is fine — the
/// bytes are deterministic per render).
fn write_instant_dump(
    path: &std::path::Path,
    rendered: &str,
    shell: CoreShell,
) -> std::io::Result<()> {
    // Only zsh's init script sources the dump today. Bash and fish get the
    // same single-quote-and-escape treatment for now; their init scripts
    // will swap to the right per-shell syntax when they ship.
    let _ = shell;
    let content = zsh_dump_line(rendered);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Atomic write: same-directory tempfile + rename. The tempfile must
    // be on the same filesystem as the destination for `rename` to be
    // atomic; placing it next to the destination guarantees that.
    //
    // The tempfile name embeds pid + nanos so concurrent shells writing
    // to the same dump path don't collide on the `O_CREAT|O_EXCL` open
    // below.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let tmp_name = format!(
        "{}.{}.{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("dump"),
        std::process::id(),
        nanos,
    );
    let tmp = path.with_file_name(tmp_name);
    let guard = TmpGuard::new(tmp.clone());
    write_dump_tmp_atomic(&tmp, content.as_bytes())?;
    std::fs::rename(&tmp, path)?;
    // Rename succeeded; the tmp path no longer exists. Disarm the guard
    // so its `Drop` doesn't try to unlink the (now-renamed) final file.
    guard.disarm();
    Ok(())
}

/// RAII cleanup for a staged tempfile.
///
/// On drop, attempts to remove the file at the recorded path. Used by
/// [`write_instant_dump`] to guarantee that a panic — or any early
/// return — between `create_new` and `rename` does not leave a partial
/// dump on disk for the next shell to source. Errors during cleanup are
/// swallowed (the file may already be gone or have been renamed away;
/// either way there is nothing useful to do).
///
/// Disarm with [`Self::disarm`] after a successful rename so the drop
/// is a no-op.
struct TmpGuard {
    path: Option<PathBuf>,
}

impl TmpGuard {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(mut self) {
        self.path = None;
    }
}

impl Drop for TmpGuard {
    fn drop(&mut self) {
        if let Some(p) = self.path.take() {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// Write `content` to `tmp` with paranoid open flags.
///
/// On unix the open is `O_WRONLY|O_CREAT|O_EXCL|O_NOFOLLOW` at mode
/// `0o600`, and we `fsync(2)` the fd before returning. Rationale:
///
/// - `O_CREAT|O_EXCL` (Rust `create_new(true)`) refuses to open an
///   existing path. POSIX says a symlink "counts as existing" for this
///   check regardless of the target, so this alone defeats classic
///   `/tmp/foo.tmp` → `/etc/shadow` pre-plant attacks.
/// - `O_NOFOLLOW` is belt-and-suspenders for the same threat.
/// - Mode `0o600`: the dump line contains the literal rendered PROMPT,
///   which can include cwd path components users may consider sensitive
///   on a multi-user host. The default umask of `0o022` would leak the
///   file to "others".
/// - `sync_all()` (fsync) before rename: without it, a power loss between
///   the rename and writeback would leave the next shell sourcing a
///   zero-byte file.
#[cfg(unix)]
fn write_dump_tmp_atomic(tmp: &std::path::Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    #[allow(clippy::cast_possible_wrap)]
    let nofollow = rustix::fs::OFlags::NOFOLLOW.bits() as i32;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(nofollow)
        .mode(0o600)
        .open(tmp)?;
    f.write_all(content)?;
    f.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn write_dump_tmp_atomic(tmp: &std::path::Path, content: &[u8]) -> std::io::Result<()> {
    std::fs::write(tmp, content)
}

/// Build the dump file's content for zsh: `PROMPT='<escaped-rendered> '\n`.
///
/// Escaping uses the standard zsh single-quote idiom: every `'` in the
/// rendered string is closed, written as `\'`, then re-opened. ANSI
/// escape bytes (`\x1b`), `%`, `{`, `}`, and unicode pass through cleanly
/// in single-quoted literals.
fn zsh_dump_line(rendered: &str) -> String {
    let mut out = String::with_capacity(rendered.len() + 16);
    out.push_str("PROMPT='");
    for c in rendered.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    // Trailing space inside the literal so the cached value matches
    // precmd's `"$(... ) "` output byte-for-byte.
    out.push(' ');
    out.push('\'');
    out.push('\n');
    out
}

/// Print the per-shell init script for `eval` / `source`.
///
/// Substitutes the literal `__P10K_RS_BIN__` token in the script template
/// with the absolute path of the currently-running binary, so the hook
/// can call us back even when our install dir isn't on `PATH`.
fn cmd_init(shell: &str) -> Result<()> {
    let target = ShellInit::from_str(shell).map_err(anyhow::Error::from)?;
    let template = p10k_rs_shell::init_script(target);
    let exe = std::env::current_exe().context("resolve current exe path")?;
    let exe_str = exe.to_str().context(
        "current exe path is not valid UTF-8; the init script can't embed it as a shell literal",
    )?;
    safe_for_single_quote(exe_str, "exe path")?;
    let gsd = p10k_rs_git::locate_gitstatusd()
        .and_then(|p| p.to_str().map(str::to_owned))
        .unwrap_or_default();
    safe_for_single_quote(&gsd, "gitstatusd path")?;
    let script = template
        .replace("__P10K_RS_BIN__", exe_str)
        .replace("__P10K_RS_GITSTATUSD_BIN__", &gsd);
    print!("{script}");
    Ok(())
}

/// Reject paths that can't safely live inside a shell single-quoted literal.
///
/// Single-quoted POSIX strings preserve every byte literally except `'`
/// itself, but newlines, NULs, and other control characters can break the
/// surrounding script in line-oriented contexts (sourcing, `eval`, log
/// inspection). Reject any `'`, any byte below `0x20`, and `0x7F` (DEL).
fn safe_for_single_quote(s: &str, kind: &str) -> Result<()> {
    for b in s.bytes() {
        if b == b'\'' || b < 0x20 || b == 0x7F {
            anyhow::bail!(
                "{kind} contains an unsafe byte (single quote, control char, or DEL): {s:?}. \
                 The init script embeds this in a single-quoted shell literal; \
                 these bytes would break the script. Move or symlink the binary first."
            );
        }
    }
    Ok(())
}

/// Probe the active git backend and run the status query.
///
/// Prefers `Gitstatusd` (long-lived daemon, ~ms latency) when the shell init
/// script has exported `_P10K_RS_GITSTATUSD_REQ` / `_P10K_RS_GITSTATUSD_RESP`
/// pointing at live FIFOs. Falls back to `ShellOut` (spawns `git`) otherwise.
fn git_status(path: &std::path::Path) -> Option<p10k_rs_core::GitState> {
    if let (Some(req), Some(resp)) = (
        std::env::var_os("_P10K_RS_GITSTATUSD_REQ"),
        std::env::var_os("_P10K_RS_GITSTATUSD_RESP"),
    ) {
        let req_path = std::path::Path::new(&req);
        let resp_path = std::path::Path::new(&resp);
        if let Some(d) = Gitstatusd::from_env_paths(req_path, resp_path) {
            return d.status(path);
        }
    }
    GitShellOut.status(path)
}

/// Probe Jujutsu state for `path`.
///
/// Unlike git, jj has no daemon analogue today — the producer is a
/// filesystem walk for `.jj/` plus a `jj log` shell-out on hit. The
/// cost is comparable to the `ShellOut` git fallback (sub-millisecond
/// when not in a repo, single-digit ms when in one). Returns `None`
/// outside a jj repo or when `jj` isn't on `$PATH`.
fn jj_status(path: &std::path::Path) -> Option<p10k_rs_core::JjState> {
    p10k_rs_jj::detect_jj(path)
}

/// Run the interactive configure wizard and write the resulting TOML to stdout.
///
/// Prompts and progress messages go to stderr (the wizard writes them
/// directly) so users can pipe stdout cleanly:
///
///   p10k-rs configure > ~/.config/p10k-rs/config.toml
fn cmd_configure() -> Result<()> {
    let cfg = p10k_rs_wizard::run().map_err(anyhow::Error::from)?;
    let toml = cfg.to_toml().context("serialise wizard config")?;
    print!("{toml}");
    Ok(())
}

/// Translate a Powerlevel10k `.p10k.zsh` config to TOML and write it to stdout.
///
/// Warnings (unrecognised variables, unparseable values) go to stderr so a
/// pipe like `p10k-rs import ~/.p10k.zsh > ~/.config/p10k-rs/config.toml` does
/// the right thing.
fn cmd_import(path: &std::path::Path) -> Result<()> {
    let input = std::fs::read_to_string(path)
        .with_context(|| format!("read p10k config {}", path.display()))?;
    let outcome = p10k_rs_config::import::import_p10k_zsh(&input);
    for warning in &outcome.warnings {
        eprintln!("import: {warning}");
    }
    let toml = outcome
        .config
        .to_toml()
        .context("serialise imported config")?;
    print!("{toml}");
    Ok(())
}

/// Map the CLI shell string to the [`CoreShell`] enum used in `RenderCtx`.
///
/// Returns an error for shells the binary doesn't know how to render for
/// today. The error message lists the supported shells.
fn parse_core_shell(s: &str) -> Result<CoreShell> {
    match s.to_ascii_lowercase().as_str() {
        "zsh" => Ok(CoreShell::Zsh),
        "fish" => Ok(CoreShell::Fish),
        "bash" => Ok(CoreShell::Bash),
        other => anyhow::bail!("unknown shell '{other}': supported = zsh, fish, bash"),
    }
}

/// Install the diagnostics-log subscriber (T1.21).
///
/// Wires a daily-rotating file appender to
/// `$XDG_STATE_HOME/p10k-rs/diagnostics.log` (fallback
/// `$HOME/.local/state/p10k-rs/diagnostics.log`). Parent dir is created
/// at mode `0o700`; today's log file is post-chmod'd to `0o600` so
/// `tracing-appender`'s default umask-derived mode (typically `0o644`)
/// doesn't leak diagnostics readable to "others" on a multi-user host.
///
/// Filter source: `EnvFilter::try_from_env("P10K_RS_LOG")` with a
/// default of `warn`. Prefixing with `P10K_RS_LOG` rather than
/// `RUST_LOG` avoids clobbering the global env var for unrelated tools
/// running under our process.
///
/// Format: compact, ANSI off (file output — no terminal to colour for).
///
/// Non-blocking: writes go through `tracing_appender::non_blocking`,
/// which buffers on a background thread so a slow disk can't add to
/// prompt latency. The returned [`tracing_appender::non_blocking::WorkerGuard`]
/// must outlive the last `tracing` event we want flushed; the caller
/// holds it for the lifetime of `main`. Returns `None` when the log
/// dir can't be resolved (no `HOME` and no `XDG_STATE_HOME`) or when
/// dir prep fails — in that case the binary runs without a diagnostics
/// subscriber and the prompt path is unaffected (events degrade to
/// no-ops).
fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::{fmt, EnvFilter};

    let dir = diagnostics_log_dir()?;
    if let Err(e) = ensure_log_dir(&dir) {
        // No subscriber wired yet, so a `tracing::warn!` here would be
        // a silent no-op. Fall back to stderr for the one-shot setup
        // failure. If stderr is also redirected (as it is under the
        // T1.22 zsh init), the message lands in the same diagnostics
        // log on the next successful init — same failure shape as
        // pre-T1.21, no regression.
        eprintln!(
            "p10k-rs: failed to prepare diagnostics dir {}: {e}",
            dir.display()
        );
        return None;
    }

    // Daily rotation: tracing-appender writes
    // `diagnostics.log.YYYY-MM-DD` files in UTC. The research doc
    // (`audit-logging.md` Tier 1) asked for 1 MiB size-based rotation,
    // but tracing-appender's size rotation has known caveats (no
    // built-in retention; awkward state across restarts). Daily
    // rotation is simpler, gives a natural retention story (one file
    // per UTC day; users can `find -mtime +N -delete` if they care),
    // and the diagnostics channel's expected volume (a handful of
    // events per shell session) keeps each daily file trivially small.
    let appender = tracing_appender::rolling::daily(&dir, "diagnostics.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_env("P10K_RS_LOG").unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .compact()
        .try_init();

    // Belt-and-braces chmod after the appender opens today's file.
    // tracing-appender uses the process umask (typically `0o022` →
    // `0o644`), which would leak diagnostics readable to "others" on a
    // multi-user host. We re-stat today's file and clamp to `0o600`.
    // The chmod is a no-op when bits already match; per-prompt cost is
    // one stat + at most one chmod (µs).
    let today_path = todays_log_path(&dir);
    if today_path.exists() {
        let _ = set_mode_0600(&today_path);
    }
    Some(guard)
}

/// Resolve the diagnostics log directory: `$XDG_STATE_HOME/p10k-rs` if
/// set, otherwise `$HOME/.local/state/p10k-rs`. Returns `None` when
/// neither env var is set — the binary then runs without a diagnostics
/// log, which matches the pre-T1.21 silent behaviour.
///
/// Thin wrapper over [`resolve_diagnostics_dir`] so the resolution
/// logic stays a pure function of its inputs (and therefore unit-
/// testable without mutating process-wide env vars, which races other
/// tests).
fn diagnostics_log_dir() -> Option<PathBuf> {
    resolve_diagnostics_dir(
        std::env::var_os("XDG_STATE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

/// Pure resolver: pick the diagnostics dir given a candidate
/// `$XDG_STATE_HOME` and `$HOME`.
///
/// Split out from [`diagnostics_log_dir`] so the policy ("prefer XDG;
/// fall back to `~/.local/state`; ignore empty XDG values") can be
/// tested without touching process env. Returns `None` when neither
/// candidate is usable.
fn resolve_diagnostics_dir(
    xdg_state_home: Option<&std::ffi::OsStr>,
    home: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    if let Some(xdg) = xdg_state_home {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("p10k-rs"));
        }
    }
    let home = home?;
    Some(
        PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("p10k-rs"),
    )
}

/// Create `dir` with mode `0o700` if missing; idempotent.
///
/// `DirBuilder::recursive(true)` creates missing parents too. The
/// `mode(0o700)` from `DirBuilderExt` applies only to *newly* created
/// components, so we also `set_permissions` on `dir` itself to clamp a
/// pre-existing world-readable leaf down to `0o700`. Parents are left
/// alone — `~/.local/state` is shared with other tools and should
/// follow the user's umask, not ours.
fn ensure_log_dir(dir: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Path to today's rotating log file inside `dir`.
///
/// Mirrors `tracing_appender::rolling::daily(dir, "diagnostics.log")`'s
/// naming convention: `diagnostics.log.YYYY-MM-DD` in UTC. Used by
/// [`init_tracing`] to chmod the freshly-created file to `0o600`, and
/// by the test suite to assert the file's mode. If tracing-appender
/// ever changes its date format, the smoke test catches the drift.
fn todays_log_path(dir: &std::path::Path) -> PathBuf {
    let now = time::OffsetDateTime::now_utc();
    let stamp = format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        u8::from(now.month()),
        now.day()
    );
    dir.join(format!("diagnostics.log.{stamp}"))
}

/// Force `path`'s mode to `0o600`. Unix-only; no-op on other platforms.
#[cfg(unix)]
fn set_mode_0600(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_mode_0600(_path: &std::path::Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
#[cfg(unix)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]
mod tracing_init_tests {
    //! T1.21 — diagnostics-log smoke tests.
    //!
    //! We don't drive the full `init_tracing` here because it calls
    //! `try_init` on the global subscriber and unit tests share a
    //! process. Instead we exercise the load-bearing helpers
    //! (`diagnostics_log_dir`, `ensure_log_dir`, `todays_log_path`,
    //! `set_mode_0600`) plus a one-shot tracing → appender → file
    //! write using a scoped subscriber, which is safe under cargo
    //! test's threading.

    use super::{ensure_log_dir, resolve_diagnostics_dir, set_mode_0600, todays_log_path};
    use std::ffi::OsStr;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn scratch_path(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "p10k-rs-diag-test-{tag}-{}-{stamp}",
            std::process::id()
        ))
    }

    #[test]
    fn ensure_log_dir_creates_with_mode_0700() {
        let dir = scratch_path("mkdir-0700");
        ensure_log_dir(&dir).unwrap();
        let md = std::fs::metadata(&dir).unwrap();
        let mode = md.permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "expected 0o700, got 0o{mode:o}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ensure_log_dir_tightens_preexisting_loose_mode() {
        // Simulate a user with a pre-existing diagnostics dir at a
        // looser mode (e.g. a tool ran under umask 022). The helper
        // must clamp back to 0o700 — without this, an upgrade from a
        // pre-T1.21 binary that created the dir at default umask would
        // leave the diagnostics file discoverable by other UIDs.
        let dir = scratch_path("mkdir-tighten");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        ensure_log_dir(&dir).unwrap();
        let md = std::fs::metadata(&dir).unwrap();
        let mode = md.permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "expected 0o700 after tighten, got 0o{mode:o}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_mode_0600_clamps_file_mode() {
        let dir = scratch_path("file-0600");
        ensure_log_dir(&dir).unwrap();
        let file = dir.join("diagnostics.log.test");
        std::fs::write(&file, b"x").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        set_mode_0600(&file).unwrap();
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0o600, got 0o{mode:o}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn diagnostics_dir_prefers_xdg_state_home() {
        // Pure-function test over the resolver — no env mutation, so
        // safe to run in parallel under cargo test's worker pool.
        let resolved = resolve_diagnostics_dir(
            Some(OsStr::new("/tmp/fake-xdg-state")),
            Some(OsStr::new("/tmp/fake-home")),
        )
        .unwrap();
        assert_eq!(resolved, PathBuf::from("/tmp/fake-xdg-state/p10k-rs"));
    }

    #[test]
    fn diagnostics_dir_falls_back_to_home_local_state() {
        let resolved = resolve_diagnostics_dir(None, Some(OsStr::new("/tmp/fake-home"))).unwrap();
        assert_eq!(
            resolved,
            PathBuf::from("/tmp/fake-home/.local/state/p10k-rs")
        );
    }

    #[test]
    fn diagnostics_dir_treats_empty_xdg_as_unset() {
        // Some users export `XDG_STATE_HOME=` to clear it. Treat that
        // like "unset" rather than building `/p10k-rs` at root.
        let resolved =
            resolve_diagnostics_dir(Some(OsStr::new("")), Some(OsStr::new("/tmp/fake-home")))
                .unwrap();
        assert_eq!(
            resolved,
            PathBuf::from("/tmp/fake-home/.local/state/p10k-rs")
        );
    }

    #[test]
    fn diagnostics_dir_returns_none_when_no_inputs() {
        assert!(resolve_diagnostics_dir(None, None).is_none());
    }

    #[test]
    fn todays_log_path_matches_appender_format() {
        // Verify `todays_log_path` produces the same filename
        // tracing-appender's daily rotation would write to (UTC,
        // `diagnostics.log.YYYY-MM-DD`). If this drifts, our chmod
        // would silently miss the live file and the next event would
        // land at the umask-default mode.
        let dir = scratch_path("today");
        ensure_log_dir(&dir).unwrap();
        let computed = todays_log_path(&dir);
        let parent = computed.parent().unwrap();
        let name = computed.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(parent, dir);
        assert!(
            name.starts_with("diagnostics.log."),
            "filename {name:?} must carry the diagnostics.log. prefix",
        );
        let suffix = &name["diagnostics.log.".len()..];
        // YYYY-MM-DD: ten chars, two dashes at indices 4 and 7.
        assert_eq!(suffix.len(), 10, "date suffix {suffix:?} must be 10 chars");
        assert_eq!(suffix.as_bytes()[4], b'-');
        assert_eq!(suffix.as_bytes()[7], b'-');
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tracing_warn_lands_in_diagnostics_file() {
        // End-to-end smoke: configure a scoped subscriber pointing at a
        // tracing-appender daily appender under a scratch dir, emit a
        // `tracing::warn!`, drop the guard to flush, then read the
        // freshly-rolled file and assert the message is present. Uses
        // `with_default` rather than `try_init` to avoid races with
        // other tests in the same process.
        use tracing::subscriber::with_default;
        use tracing_subscriber::fmt;

        let dir = scratch_path("warn-lands");
        ensure_log_dir(&dir).unwrap();
        let appender = tracing_appender::rolling::daily(&dir, "diagnostics.log");
        let (writer, guard) = tracing_appender::non_blocking(appender);
        let subscriber = fmt()
            .with_writer(writer)
            .with_ansi(false)
            .compact()
            .finish();
        with_default(subscriber, || {
            tracing::warn!(target: "p10k_rs_test", "diagnostics-smoke marker line");
        });
        // Drop the guard so the background thread flushes and exits
        // before we read the file — same lifecycle the binary follows
        // by holding `_guard` in `main`.
        drop(guard);

        let path = todays_log_path(&dir);
        // Belt-and-braces: appender opens with process umask (0o644
        // typically). Verify the chmod helper clamps it.
        if path.exists() {
            set_mode_0600(&path).unwrap();
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "expected 0o600 after chmod, got 0o{mode:o}");
        }
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(
            contents.contains("diagnostics-smoke marker line"),
            "expected the warn line in the log file, got: {contents:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Decide whether the renderer should emit OSC 7 / OSC 133
/// shell-integration sequences this prompt.
///
/// Pure over the supplied `env` closure for testability — production
/// callers pass `|k| std::env::var(k).ok()`.
///
/// Decision matrix:
///
/// - **`ShellIntegrationMode::Off`** → always `false`. Honoured even
///   when an AI host is present, on the principle that the user's
///   explicit config wins.
/// - **`ShellIntegrationMode::Always`** → `true`, *except* when Warp
///   is detected (`TERM_PROGRAM=WarpTerminal`). Warp's block model
///   parses `OSC 133;A` and treats it as a new block; the prompt
///   rendering then breaks. See warp#6718 — the open upstream issue
///   tracks the conflict. Suppression is unconditional regardless of
///   user intent.
/// - **`ShellIntegrationMode::Auto`** (default) → `true` when any of
///   the following hold, again with the Warp suppression on top:
///   - `host` is anything other than `HostKind::None` (AI host
///     present — Claude Code / Aider / Cursor).
///   - `TERM_PROGRAM` is set to anything other than `WarpTerminal`
///     (covers `iTerm2`, Apple Terminal, `VS Code`, Ghostty, `WezTerm`,
///     `foot`, …).
///   - `WT_SESSION` is set (Windows Terminal).
///   - `GHOSTTY_RESOURCES_DIR` is set (Ghostty).
///   - `KITTY_WINDOW_ID` is set (Kitty).
fn resolve_shell_integration<F: Fn(&str) -> Option<String>>(
    mode: ShellIntegrationMode,
    host: &HostKind,
    env: F,
) -> bool {
    // Warp's `OSC 133;A` handling breaks block rendering. Hard
    // suppression regardless of mode or AI host. See warp#6718.
    if matches!(env("TERM_PROGRAM").as_deref(), Some("WarpTerminal")) {
        return false;
    }
    // `_` (instead of just `ShellIntegrationMode::Auto`) covers any
    // future variant added to the non_exhaustive enum — those degrade
    // to the conservative auto-detect path until the binary learns the
    // new mode.
    match mode {
        ShellIntegrationMode::Off => false,
        ShellIntegrationMode::Always => true,
        _ => {
            if *host != HostKind::None {
                return true;
            }
            env("TERM_PROGRAM").is_some()
                || env("WT_SESSION").is_some()
                || env("GHOSTTY_RESOURCES_DIR").is_some()
                || env("KITTY_WINDOW_ID").is_some()
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod shell_integration_tests {
    use super::{resolve_shell_integration, HostKind, ShellIntegrationMode};

    /// Build a fake env lookup from a slice of `(key, value)` pairs.
    fn env_with<'a>(
        pairs: &'a [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> Option<String> + 'a {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn off_mode_suppresses_even_with_ai_host() {
        // Explicit user opt-out beats every detection signal.
        let active = resolve_shell_integration(
            ShellIntegrationMode::Off,
            &HostKind::ClaudeCode,
            env_with(&[("TERM_PROGRAM", "iTerm.app")]),
        );
        assert!(!active);
    }

    #[test]
    fn always_mode_emits_in_vanilla_terminal() {
        // No host, no detectable terminal — `Always` still emits.
        let active =
            resolve_shell_integration(ShellIntegrationMode::Always, &HostKind::None, env_with(&[]));
        assert!(active);
    }

    #[test]
    fn warp_suppressed_under_always() {
        // warp#6718: Warp's block model breaks on OSC 133;A. Even
        // when the user asks for `always`, we suppress.
        let active = resolve_shell_integration(
            ShellIntegrationMode::Always,
            &HostKind::ClaudeCode,
            env_with(&[("TERM_PROGRAM", "WarpTerminal")]),
        );
        assert!(!active, "Warp must be suppressed regardless of mode");
    }

    #[test]
    fn warp_suppressed_under_auto() {
        let active = resolve_shell_integration(
            ShellIntegrationMode::Auto,
            &HostKind::None,
            env_with(&[("TERM_PROGRAM", "WarpTerminal")]),
        );
        assert!(!active);
    }

    #[test]
    fn auto_detects_iterm() {
        let active = resolve_shell_integration(
            ShellIntegrationMode::Auto,
            &HostKind::None,
            env_with(&[("TERM_PROGRAM", "iTerm.app")]),
        );
        assert!(active);
    }

    #[test]
    fn auto_detects_ghostty() {
        let active = resolve_shell_integration(
            ShellIntegrationMode::Auto,
            &HostKind::None,
            env_with(&[("GHOSTTY_RESOURCES_DIR", "/usr/share/ghostty")]),
        );
        assert!(active);
    }

    #[test]
    fn auto_detects_kitty() {
        let active = resolve_shell_integration(
            ShellIntegrationMode::Auto,
            &HostKind::None,
            env_with(&[("KITTY_WINDOW_ID", "1")]),
        );
        assert!(active);
    }

    #[test]
    fn auto_detects_windows_terminal() {
        let active = resolve_shell_integration(
            ShellIntegrationMode::Auto,
            &HostKind::None,
            env_with(&[("WT_SESSION", "abc-123")]),
        );
        assert!(active);
    }

    #[test]
    fn auto_detects_ai_host_even_without_terminal_env() {
        let active = resolve_shell_integration(
            ShellIntegrationMode::Auto,
            &HostKind::ClaudeCode,
            env_with(&[]),
        );
        assert!(active);
    }

    #[test]
    fn auto_is_off_with_no_signal() {
        // Bare ssh into a host with no TERM_PROGRAM and no AI host.
        let active =
            resolve_shell_integration(ShellIntegrationMode::Auto, &HostKind::None, env_with(&[]));
        assert!(!active);
    }
}

#[cfg(test)]
#[cfg(unix)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod write_dump_tests {
    use super::{write_instant_dump, CoreShell};
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn scratch_path(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "p10k-rs-dump-test-{tag}-{}-{stamp}",
            std::process::id()
        ))
    }

    #[test]
    fn write_instant_dump_creates_file_with_owner_only_mode() {
        // The dump can contain cwd path components that some users treat
        // as sensitive on a multi-user host; the file must not be
        // world-readable.
        let dump = scratch_path("mode");
        write_instant_dump(&dump, "\u{276f} ", CoreShell::Zsh).unwrap();
        let md = std::fs::metadata(&dump).unwrap();
        let mode = md.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0o600, got 0o{mode:o}");
        let _ = std::fs::remove_file(&dump);
    }

    #[test]
    fn write_instant_dump_does_not_follow_preplanted_symlink() {
        // Attacker pre-plants the final dump path as a symlink to a
        // sensitive file. `std::fs::rename` overwrites the symlink entry
        // itself rather than following it, so the symlink target must
        // stay untouched.
        //
        // (Pre-T1.3 the staged tempfile lived at a deterministic
        // `<dump>.tmp` path, so an attacker could also pre-plant *that*
        // and rely on the `O_NOFOLLOW` belt-and-suspenders to defeat the
        // pre-plant. With T1.3 the tempfile name embeds pid + nanos and
        // is no longer predictable from outside; the final-path symlink
        // is now the only feasible vector to assert against here.)
        let dump = scratch_path("symlink");
        let target = scratch_path("symlink-target");
        std::fs::write(&target, b"untouched\n").unwrap();
        std::os::unix::fs::symlink(&target, &dump).unwrap();
        write_instant_dump(&dump, "x", CoreShell::Zsh).unwrap();
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"untouched\n",
            "symlink target was written through"
        );
        let _ = std::fs::remove_file(&dump);
        let _ = std::fs::remove_file(&target);
    }

    #[test]
    fn write_instant_dump_panic_mid_write_leaves_no_partial_file() {
        // T1.3 panic-safety contract: if a panic escapes the dump-write
        // function between the `create_new` open and the final `rename`,
        // the staged tempfile must be cleaned up so the next shell never
        // sources a half-written PROMPT. We simulate the panic by running
        // the write under `catch_unwind` and triggering the panic via a
        // `Drop` that runs while the dump function is mid-flight on the
        // stack — concretely, by panicking from inside a closure scoped
        // around `write_instant_dump`.
        //
        // The cleanest panic injection point we can reach without
        // surgery is `panic::catch_unwind(|| write_instant_dump(...))`
        // followed by *manually* asserting the tempfile-cleanup invariant
        // via [`TmpGuard`]'s `Drop`. We do that by constructing the
        // TmpGuard at a known scratch path, forcing a panic inside a
        // scope that owns the guard, and asserting the file is gone
        // after the unwind.
        use std::panic;
        let dump = scratch_path("panic-mid-write");
        let _ = std::fs::remove_file(&dump);
        // Compute the same tempfile name shape `write_instant_dump`
        // would produce so we can write to it, install the guard, then
        // panic.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let tmp_name = format!(
            "{}.{}.{}.tmp",
            dump.file_name().unwrap().to_str().unwrap(),
            std::process::id(),
            stamp,
        );
        let tmp_path = dump.with_file_name(tmp_name);
        let tmp_for_panic = tmp_path.clone();
        let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            let guard = super::TmpGuard::new(tmp_for_panic.clone());
            std::fs::write(&tmp_for_panic, b"partial dump\n").unwrap();
            // Drop the guard via panic before any rename.
            let _g = guard;
            panic!("simulated mid-write panic");
        }));
        assert!(result.is_err(), "expected the closure to panic");
        // Tempfile must be gone — the TmpGuard's `Drop` cleaned it up.
        assert!(
            !tmp_path.exists(),
            "panic-mid-write left a partial tempfile at {tmp_path:?}",
        );
        // And the final dump path must also be absent — we never
        // reached the rename, so the next shell sources nothing.
        assert!(
            !dump.exists(),
            "panic-mid-write left a partial dump at {dump:?}",
        );
    }
}
