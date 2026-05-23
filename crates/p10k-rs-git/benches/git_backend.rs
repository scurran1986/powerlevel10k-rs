//! Slice 60 phase 6 — criterion bench comparing [`ShellOut`] vs
//! [`GixBackend`] latency on a real git repository (the workspace
//! itself, located two directories up from this crate).
//!
//! ## Reading the numbers
//!
//! The bench reports per-call latency for one `.status(repo)` call —
//! the exact thing the prompt does once per shell prompt. Both
//! backends are end-to-end (process spawn + git ops for ShellOut;
//! gix discovery + status iter for GixBackend), so the numbers map
//! 1:1 onto perceived prompt latency on hosts where each tier serves.
//!
//! On a clean workspace, GixBackend should be modestly faster
//! (no `fork`/`execve` overhead, no zsh-side parsing). On a *very*
//! dirty workspace with many untracked files, the gap narrows
//! because the gix dirwalk has to do the same I/O `git status`
//! does. Kernel-class repos (millions of objects) are intentionally
//! out of scope for CI — cloning the kernel just to benchmark
//! exceeds reasonable CI time. Run this bench against a checked-out
//! linux tree locally if you want those numbers; the harness
//! parameterises on `CARGO_MANIFEST_DIR` so a different repo is one
//! env-var change away.
//!
//! ## Running
//!
//! ```text
//! cargo bench -p p10k-rs-git --bench git_backend
//! ```
//!
//! Outputs land in `target/criterion/`, including an HTML report
//! at `target/criterion/git_backend/report/index.html` (the
//! `html_reports` feature on criterion is workspace-pinned).

// The `criterion_group!` macro expands to a `pub fn benches(...)` with
// no doc comment, which the workspace's `missing_docs = "warn"` would
// otherwise flag. Benches aren't a lib surface anyone reads docs for,
// so suppressing the lint locally is correct here. `doc_markdown` is
// also suppressed: backticking every `ShellOut` / `GixBackend` mention
// in the module-level prose hurts readability without adding rigour.
#![allow(
    missing_docs,
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic
)]

use std::path::PathBuf;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use p10k_rs_git::{Backend as _, GixBackend, ShellOut};

/// Walk two parent directories from this crate's manifest dir to
/// reach the workspace root. The bench targets the *workspace's*
/// git repo (which always exists during local + CI runs); pointing
/// at `CARGO_MANIFEST_DIR` itself would land inside a sub-crate
/// directory but still hit the same `.git`, so functionally
/// identical — the parent walk just makes the intent explicit.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root sits two dirs above the crate manifest")
        .to_path_buf()
}

fn bench_backends(c: &mut Criterion) {
    let repo = workspace_root();

    // Sanity: don't burn bench cycles on a non-existent path. If the
    // workspace layout changes such that the walk-up no longer lands
    // on a git repo, fail loudly instead of producing meaningless
    // zero-time samples.
    assert!(
        repo.join(".git").exists(),
        "bench expects a git repo at {} (workspace root)",
        repo.display()
    );

    let mut group = c.benchmark_group("git_backend");
    // The default sample count (100) is appropriate here — a single
    // `.status()` call is fast enough that criterion's variance
    // estimation needs a meaningful sample, but slow enough that we
    // don't want to drag CI runtime out with thousands.

    group.bench_function("gix_workspace", |b| {
        b.iter(|| {
            let out = GixBackend.status(black_box(&repo));
            black_box(out)
        });
    });

    group.bench_function("shellout_workspace", |b| {
        b.iter(|| {
            let out = ShellOut.status(black_box(&repo));
            black_box(out)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_backends);
criterion_main!(benches);
