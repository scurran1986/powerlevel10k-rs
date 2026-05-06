//! Criterion harness for the day-1 spike.
//!
//! Three impls × three repos × three thermal states = 27 cells. We don't
//! exercise all 27 in CI (cold runs need a page-cache flush, which needs
//! root); the matrix below is the **honest** subset we can run unattended.
//!
//! ## Thermal states
//!
//! - **cold** — page cache cleared via `vmtouch -e <repo>` before each
//!   iteration. Skipped if `vmtouch` isn't on `$PATH`. On macOS we'd need
//!   `purge` (root-only); we document that and leave hot+warm as the
//!   honest measure there. See `MVP-SPEC.md` § "Day-1 Spike".
//! - **hot** — first run after process start; some pages cold, some not.
//! - **warm** — second+ run; everything in page cache, untracked-cache
//!   populated for the hybrid impl.
//!
//! ## Repos
//!
//! Pulled from `/home/seaburdz/github/powerlevel10k-rs/bench/fixtures/repos/`,
//! laid down by the bench-infra contractor (see their `bench/README.md`).
//! Expected layout:
//!   bench/fixtures/repos/chromium/   (or a clone marker)
//!   bench/fixtures/repos/linux/
//!   bench/fixtures/repos/small/      (a 100-file synthetic repo)
//!
//! The harness gracefully skips any repo that isn't present so the spike can
//! be wired up before the fixtures land.
//!
//! Run:
//!   cargo bench -p spike-gitstatus -- --quick     (sanity check)
//!   cargo bench -p spike-gitstatus                (full report under target/criterion)

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode};

use spike_gitstatus::{gitstatusd_baseline, gix_path, hybrid};

fn fixtures_root() -> PathBuf {
    // Workspace root is two `..` up from the crate.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("bench")
        .join("fixtures")
        .join("repos")
}

fn candidate_repos() -> Vec<(&'static str, PathBuf)> {
    let root = fixtures_root();
    vec![
        ("small", root.join("small")),
        ("chromium", root.join("chromium")),
        ("linux", root.join("linux")),
    ]
    .into_iter()
    .filter(|(_, p)| p.is_dir())
    .collect()
}

fn drop_caches(repo: &Path) -> bool {
    if which("vmtouch").is_none() {
        return false;
    }
    Command::new("vmtouch")
        .arg("-e")
        .arg(repo)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn which(prog: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(prog);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn bench_hot(c: &mut Criterion) {
    let mut group = c.benchmark_group("hot");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));
    group.sampling_mode(SamplingMode::Flat);

    for (name, repo) in candidate_repos() {
        group.bench_with_input(BenchmarkId::new("gix-only", name), &repo, |b, r| {
            b.iter(|| gix_path::status(r).unwrap());
        });
        group.bench_with_input(BenchmarkId::new("hybrid", name), &repo, |b, r| {
            b.iter(|| hybrid::status(r).unwrap());
        });
        if gitstatusd_baseline::is_available() {
            group.bench_with_input(BenchmarkId::new("baseline", name), &repo, |b, r| {
                b.iter(|| gitstatusd_baseline::status(r).unwrap());
            });
        }
    }
    group.finish();
}

fn bench_warm(c: &mut Criterion) {
    // "Warm" = same as hot, but we prime the run once first so the
    // hybrid's untracked-cache file exists. Identical iteration shape.
    let mut group = c.benchmark_group("warm");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));
    group.sampling_mode(SamplingMode::Flat);

    for (name, repo) in candidate_repos() {
        let _ = hybrid::status(&repo); // prime cache
        group.bench_with_input(BenchmarkId::new("gix-only", name), &repo, |b, r| {
            b.iter(|| gix_path::status(r).unwrap());
        });
        group.bench_with_input(BenchmarkId::new("hybrid", name), &repo, |b, r| {
            b.iter(|| hybrid::status(r).unwrap());
        });
        if gitstatusd_baseline::is_available() {
            group.bench_with_input(BenchmarkId::new("baseline", name), &repo, |b, r| {
                b.iter(|| gitstatusd_baseline::status(r).unwrap());
            });
        }
    }
    group.finish();
}

fn bench_cold(c: &mut Criterion) {
    let mut group = c.benchmark_group("cold");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));
    group.sampling_mode(SamplingMode::Flat);

    if which("vmtouch").is_none() {
        eprintln!(
            "[spike-bench] vmtouch not found on PATH — skipping cold group. \
             Hot/warm are the honest measure on this host."
        );
        return;
    }

    for (name, repo) in candidate_repos() {
        group.bench_with_input(BenchmarkId::new("gix-only", name), &repo, |b, r| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    drop_caches(r);
                    let t = std::time::Instant::now();
                    let _ = gix_path::status(r).unwrap();
                    total += t.elapsed();
                }
                total
            });
        });
        group.bench_with_input(BenchmarkId::new("hybrid", name), &repo, |b, r| {
            b.iter_custom(|iters| {
                let mut total = Duration::ZERO;
                for _ in 0..iters {
                    drop_caches(r);
                    let t = std::time::Instant::now();
                    let _ = hybrid::status(r).unwrap();
                    total += t.elapsed();
                }
                total
            });
        });
        if gitstatusd_baseline::is_available() {
            group.bench_with_input(BenchmarkId::new("baseline", name), &repo, |b, r| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    for _ in 0..iters {
                        drop_caches(r);
                        let t = std::time::Instant::now();
                        let _ = gitstatusd_baseline::status(r).unwrap();
                        total += t.elapsed();
                    }
                    total
                });
            });
        }
    }
    group.finish();
}

criterion_group!(spike, bench_hot, bench_warm, bench_cold);
criterion_main!(spike);
