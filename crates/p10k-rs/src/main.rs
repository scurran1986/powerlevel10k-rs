//! `p10k-rs` binary entrypoint.
//!
//! Subcommands track `MVP-SPEC.md` § 1.4: `prompt`, `init`, `configure`,
//! `import`, `statusline`, `segment-list`. Today `prompt` and `init` light
//! up for zsh end-to-end; the others remain stubs until their phases.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

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
}

fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Command::Prompt {
            shell,
            last_status,
            last_duration_ms,
            dump,
            json,
        } => {
            tracing::debug!(
                shell,
                last_status,
                last_duration_ms,
                ?dump,
                json,
                "prompt invoked"
            );
            if json {
                anyhow::bail!("--json output lands with the AI integration phase");
            }
            cmd_prompt(&shell, last_status, last_duration_ms, dump.as_deref())
        }
        Command::Init { shell } => {
            tracing::debug!(shell, "init invoked");
            cmd_init(&shell)
        }
        Command::Configure => {
            tracing::debug!("configure invoked");
            anyhow::bail!("the configure wizard lands in its own roadmap phase")
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
    }
}

/// Render the prompt: discover the user's TOML config, fall back to a
/// hardcoded factory default if anything goes wrong.
fn cmd_prompt(
    shell: &str,
    last_status: i32,
    last_duration_ms: u64,
    dump: Option<&std::path::Path>,
) -> Result<()> {
    let core_shell = parse_core_shell(shell)?;
    let cwd: PathBuf = std::env::current_dir().context("read cwd")?;

    // Prefer the `Gitstatusd` backend when the shell init script has set up
    // FIFOs and started the daemon (ADR-0001). Fall back to the slower
    // `ShellOut` for ad-hoc CLI invocations and shells where the daemon
    // couldn't start.
    let git = git_status(cwd.as_path());

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
    let env = EnvSnapshot::default();
    let ctx = RenderCtx {
        config: &cfg,
        shell: core_shell,
        host: HostKind::None,
        cwd: cwd.as_path(),
        git: git.as_ref(),
        last_status,
        last_duration: Duration::from_millis(last_duration_ms),
        jobs: 0,
        now: SystemTime::now(),
        env: &env,
    };

    let segments = assemble_segments(&cfg.layout.left);
    let prompt = p10k_rs_core::render_prompt(&segments, &ctx);

    // Stdout: left side only, plain text, no trailing newline. The init
    // script appends a single space when assigning to PROMPT.
    print!("{}", prompt.left);

    // Dump the rendered prompt to disk for the instant-prompt path. Failure
    // is non-fatal — the next invocation will retry, and the user just sees
    // a slightly slower first prompt next shell.
    if let Some(dump_path) = dump {
        if let Err(e) = write_instant_dump(dump_path, &prompt.left, core_shell) {
            tracing::warn!("instant-prompt dump write failed (non-fatal): {e}");
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
    const FACTORY_TOML: &str = r#"
schema_version = 1
[layout]
left = ["dir", "vcs", "command_execution_time", "status", "prompt_char"]
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
/// segment in a user config is a non-fatal warning, not a crash. Returns
/// in render order.
fn assemble_segments(refs: &[p10k_rs_config::SegmentRef]) -> Vec<Box<dyn Segment>> {
    let mut out: Vec<Box<dyn Segment>> = Vec::with_capacity(refs.len());
    for r in refs {
        if let Some(seg) = p10k_rs_segments::build(&r.0) {
            out.push(seg);
        } else {
            tracing::warn!("unknown segment {:?} in layout.left, skipping", r.0);
        }
    }
    out
}

/// Serialise the rendered PROMPT to a sourceable shell snippet at `path`,
/// using a temp-file + rename for atomicity (so a half-written dump never
/// corrupts the next shell's instant prompt).
///
/// The trailing literal space inside the quoted value matches the precmd
/// hook's `"$(... ) "` shape, so sourcing the dump produces the same
/// PROMPT bytes the precmd would have set.
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
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content.as_bytes())?;
    std::fs::rename(&tmp, path)?;
    Ok(())
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
    let template = p10k_rs_shell::init_script(target).map_err(anyhow::Error::from)?;
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

/// Install `tracing-subscriber` only when the user explicitly opts in via
/// `RUST_LOG`. Skipping the subscriber on the silent path saves ~100-300 µs
/// per prompt invocation — significant against gitstatusd's sub-ms response
/// on small repos. Users get debug output with `RUST_LOG=p10k_rs=debug`.
fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    if std::env::var_os("RUST_LOG").is_none() {
        return;
    }
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
