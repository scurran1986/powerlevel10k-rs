//! `p10k-rs` binary entrypoint.
//!
//! Subcommands track `MVP-SPEC.md` § 1.4: `prompt`, `init`, `configure`,
//! `import`, `statusline`, `segment-list`. Slice 1 lights up `prompt` and
//! `init` for zsh end-to-end; the others remain stubs until their phases.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use p10k_rs_core::{Config, EnvSnapshot, HostKind, RenderCtx, Shell as CoreShell};
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
            json,
        } => {
            tracing::debug!(shell, last_status, json, "prompt invoked");
            if json {
                anyhow::bail!("--json output lands with the AI integration phase");
            }
            cmd_prompt(&shell, last_status)
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
            anyhow::bail!("p9k import lands in the foundation phase")
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

/// Render the prompt: hardcoded `[dir, prompt_char]` layout for now.
fn cmd_prompt(shell: &str, last_status: i32) -> Result<()> {
    let core_shell = parse_core_shell(shell)?;
    let cwd: PathBuf = std::env::current_dir().context("read cwd")?;

    let cfg = Config::default();
    let env = EnvSnapshot::default();
    let ctx = RenderCtx {
        config: &cfg,
        shell: core_shell,
        host: HostKind::None,
        cwd: cwd.as_path(),
        git: None,
        last_status,
        last_duration: Duration::ZERO,
        jobs: 0,
        now: SystemTime::now(),
        env: &env,
    };

    let segments = p10k_rs_segments::default_layout();
    let prompt = p10k_rs_core::render_prompt(&segments, &ctx);

    // Slice 1: left side only, plain text, no trailing newline. The init
    // script appends a single space when assigning to PROMPT.
    print!("{}", prompt.left);
    Ok(())
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
    if exe_str.contains('\'') {
        anyhow::bail!(
            "exe path contains a single quote: {exe_str:?}. Won't risk emitting a malformed shell single-quoted literal — move/symlink the binary first."
        );
    }
    let script = template.replace("__P10K_RS_BIN__", exe_str);
    print!("{script}");
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

/// Install `tracing-subscriber` with sane defaults.
///
/// Default level is `warn` so the binary is silent in normal use; users get
/// debug output by setting `RUST_LOG=p10k_rs=debug`. See `ARCHITECTURE.md`
/// § 3.3.
fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
