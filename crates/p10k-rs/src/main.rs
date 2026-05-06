//! `p10k-rs` binary entrypoint.
//!
//! Subcommands track `MVP-SPEC.md` § 1.4: `prompt`, `init`, `configure`,
//! `import`, `statusline`, `segment-list`. Behaviour lands in the
//! foundation phase; today this is a clap scaffold that compiles and runs.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use anyhow::Result;
use clap::{Parser, Subcommand};

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
        Command::Prompt { shell, json } => {
            tracing::debug!(shell, json, "prompt invoked");
            anyhow::bail!("prompt rendering lands in the foundation phase");
        }
        Command::Init { shell } => {
            tracing::debug!(shell, "init invoked");
            anyhow::bail!("init scripts land in the foundation phase");
        }
        Command::Configure => {
            tracing::debug!("configure invoked");
            anyhow::bail!("the configure wizard lands in its own roadmap phase");
        }
        Command::Import { path } => {
            tracing::debug!(?path, "import invoked");
            anyhow::bail!("p9k import lands in the foundation phase");
        }
        Command::Statusline { host } => {
            tracing::debug!(host, "statusline invoked");
            anyhow::bail!("statusline lands with the AI integration phase");
        }
        Command::SegmentList => {
            for name in p10k_rs_segments::segment_names() {
                println!("{name}");
            }
            Ok(())
        }
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
