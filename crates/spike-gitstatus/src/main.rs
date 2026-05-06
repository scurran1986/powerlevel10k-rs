//! `spike-gitstatus` CLI — runs one of the three impls against a repo and
//! prints structured timing JSON to stdout.
//!
//! Usage:
//!   spike-gitstatus gix-only  --repo /path/to/repo
//!   spike-gitstatus hybrid    --repo /path/to/repo
//!   spike-gitstatus baseline  --repo /path/to/repo
//!
//! The JSON line is intended to be piped into `jq` or aggregated by a shell
//! script across (impl × repo × cold/hot) combinations. The criterion bench
//! does its own measurement; this binary exists for ad-hoc poking and for the
//! correctness test harness in `tests/correctness.rs`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Context;
use clap::{Parser, Subcommand};

use spike_gitstatus::{gitstatusd_baseline, gix_path, hybrid, GitStatusSummary};

/// Day-1 spike: gitstatusd vs gix vs hybrid.
#[derive(Debug, Parser)]
#[command(name = "spike-gitstatus", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Debug, Subcommand)]
enum Cmd {
    /// Pure-gix implementation. The boring Rust baseline.
    GixOnly(RepoArg),
    /// gix for index/refs + custom rustix walker for the workdir scan.
    Hybrid(RepoArg),
    /// Subprocess `gitstatusd` over its wire protocol — ground truth.
    Baseline(RepoArg),
}

#[derive(Debug, clap::Args)]
struct RepoArg {
    /// Path to the repo (or any subdirectory inside one).
    #[arg(long)]
    repo: PathBuf,
}

fn main() -> anyhow::Result<()> {
    // Stay quiet by default; `RUST_LOG=spike_gitstatus=debug` for noise.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let (label, repo, summary) = match cli.cmd {
        Cmd::GixOnly(a) => {
            let s = run("gix-only", &a.repo, |p| {
                gix_path::status(p).map_err(anyhow::Error::from)
            })?;
            ("gix-only", a.repo, s)
        }
        Cmd::Hybrid(a) => {
            let s = run("hybrid", &a.repo, |p| {
                hybrid::status(p).map_err(anyhow::Error::from)
            })?;
            ("hybrid", a.repo, s)
        }
        Cmd::Baseline(a) => {
            let s = run("baseline", &a.repo, |p| {
                gitstatusd_baseline::status(p).map_err(anyhow::Error::from)
            })?;
            ("baseline", a.repo, s)
        }
    };

    // Recompute the whole thing once more so the first run pays the cold
    // cost and we report the warm number. Two timings are emitted: cold +
    // warm, in the same JSON line for easy diffing.
    let (cold_us, warm_us) = match label {
        "gix-only" => time_pair(&repo, gix_path::status)?,
        "hybrid" => time_pair(&repo, hybrid::status)?,
        "baseline" => time_pair(&repo, gitstatusd_baseline::status)?,
        _ => unreachable!(),
    };

    let report = serde_json::json!({
        "impl": label,
        "repo": repo.display().to_string(),
        "cold_us": cold_us,
        "warm_us": warm_us,
        "cold_human": humantime::format_duration(std::time::Duration::from_micros(cold_us as u64)).to_string(),
        "warm_human": humantime::format_duration(std::time::Duration::from_micros(warm_us as u64)).to_string(),
        "summary": summary,
    });
    println!("{report}");
    Ok(())
}

fn run(
    label: &str,
    repo: &Path,
    f: impl FnOnce(&Path) -> anyhow::Result<GitStatusSummary>,
) -> anyhow::Result<GitStatusSummary> {
    let start = Instant::now();
    let s = f(repo).with_context(|| format!("{label} failed for {}", repo.display()))?;
    let elapsed = start.elapsed();
    tracing::debug!(?elapsed, ?s, "first run");
    Ok(s)
}

fn time_pair<F>(repo: &Path, f: F) -> anyhow::Result<(u128, u128)>
where
    F: Fn(&Path) -> spike_gitstatus::Result<GitStatusSummary>,
{
    let t0 = Instant::now();
    let _ = f(repo)?;
    let cold = t0.elapsed().as_micros();

    let t1 = Instant::now();
    let _ = f(repo)?;
    let warm = t1.elapsed().as_micros();

    Ok((cold, warm))
}
