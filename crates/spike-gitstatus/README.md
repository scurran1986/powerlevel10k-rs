# `spike-gitstatus` — Day-1 latency spike

This crate is a **throwaway**. It exists to answer one question, with numbers:

> Can a pure-Rust path get within ~2× of `gitstatusd`'s latency on a clean
> chromium repo, or do we need to pivot the architecture?

See `.planning/powerlevel10k-rs/MVP-SPEC.md` § "Day-1 Spike" for the decision
tree these numbers feed into. The shippable code lives in `p10k-rs-git`,
which copies the winning approach from this spike's verdict — not this crate.

## Three implementations

| Impl       | Source                  | What it does                                                                |
|------------|-------------------------|------------------------------------------------------------------------------|
| `gix-only` | `src/gix_path.rs`       | Pure `gix` high-level API. The boring Rust baseline.                         |
| `hybrid`   | `src/hybrid.rs`         | `gix` for index/refs + `rustix` `openat`/`fstatat` walker + per-dir mtime cache. |
| `baseline` | `src/gitstatusd_baseline.rs` | Subprocess `gitstatusd` over its wire protocol. Ground truth.            |

All three return the same `GitStatusSummary` subset of `VCS_STATUS_*` fields
(branch, commit, ahead, behind, staged_count, unstaged_count, untracked_count,
has_conflicts). The integration test in `tests/correctness.rs` is the
**acceptance gate** — if the three disagree, no benchmark number matters.

## Running it

### Sanity check (no fixtures yet)

```bash
cargo test -p spike-gitstatus
```

The integration test builds its own tiny fixture with `git init` and asserts
the three impls produce identical output for the 8 load-bearing fields.

### Ad-hoc poke

```bash
cargo run -p spike-gitstatus --release -- gix-only --repo /path/to/repo
cargo run -p spike-gitstatus --release -- hybrid    --repo /path/to/repo
cargo run -p spike-gitstatus --release -- baseline  --repo /path/to/repo
```

Each prints a JSON line with cold + warm timings and the resulting summary.

### Full bench

```bash
cargo bench -p spike-gitstatus
```

Reports land under `target/criterion/`. The harness skips repos that don't
exist (the bench-infra contractor populates
`bench/fixtures/repos/{small,chromium,linux}`) and skips the cold group on
hosts without `vmtouch` (you can't drop the page cache without root, so we
don't pretend).

To run only one group:

```bash
cargo bench -p spike-gitstatus -- hot
cargo bench -p spike-gitstatus -- warm
cargo bench -p spike-gitstatus -- cold     # needs vmtouch
```

## Success criteria

From `MVP-SPEC.md` § "Day-1 Spike":

| Scenario          | gitstatusd target | gix-only allowable | hybrid target |
|-------------------|------------------:|-------------------:|--------------:|
| Chromium hot      |           30.9 ms |             ≤100 ms|         ≤60 ms|
| Chromium cold     |            291 ms |             ≤600 ms|        ≤400 ms|
| Linux kernel hot  |           ~25 ms  |              ≤80 ms|         ≤50 ms|

### Decision tree

- **Hybrid hits all three** → ship `p10k-rs-git` as planned.
- **gix-only hits all three** → drop the custom walker, simpler architecture.
- **Hybrid misses kernel-hot by < 2×** → keep going; document gap.
- **Hybrid misses chromium-hot by > 3×** → pause; talk to gitoxide; consider
  FFI-binding the C++ daemon as a fallback.

## Where the verdict lives

After running the bench, summarise findings in
`.planning/powerlevel10k-rs/spike-verdict.md` (Sean owns the directory; the
contractor drafts the document for review). The verdict is a one-page memo,
not a slide deck — numbers and a recommendation.

## Notes

- **`unsafe`** is allowed in this crate (overriding the workspace lint) only
  for `hybrid.rs` syscall wrappers — currently unused; `rustix`'s safe API
  covers everything we need. If a future optimisation needs FFI, every
  `unsafe` block carries a `// SAFETY:` comment per
  `contractor-brief.md` § 2.
- **Single-threaded by design** for the spike. Adding `rayon` is a follow-up
  once the architecture is locked in.
- **No rename detection** in any impl — matches `gitstatusd` defaults.
- **Cache file** lives at `$XDG_CACHE_HOME/p10k-rs-spike/<hash>-untracked-cache.bin`
  and is keyed by the workdir path so multiple repos don't collide.
