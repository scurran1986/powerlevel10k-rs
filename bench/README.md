# `bench/` — Day-1 Spike Benchmark Harness

This directory holds the **portable shell** scaffolding the Day-1 spike (see
`/home/seaburdz/.planning/powerlevel10k-rs/MVP-SPEC.md` § 0 "Day-1 Spike") needs
to answer one question, with numbers:

> Can `gix` (optionally augmented by a `rustix` parent-fd walker) get within
> ~2× of `gitstatusd`'s latency on a clean Chromium repo?

The Rust harness lives in `/crates/spike-gitstatus/` and is owned by another
contractor. **This directory does not contain Rust.** It contains the inputs
(fixture repos), the gitstatusd baseline runner, the aggregator, and the report
template.

---

## Layout

```
bench/
├── README.md                 # this file
├── fetch_fixtures.sh         # clones ripgrep, optionally linux + chromium, at pinned commits
├── run_baseline.sh           # times gitstatusd over each fixture, N=200 hot iters, JSON out
├── aggregate.sh              # diffs criterion JSON against gitstatusd JSON, writes verdict MD
├── RESULTS-TEMPLATE.md       # fillable report skeleton; copy + rename per spike attempt
├── fixtures/
│   ├── .gitkeep
│   └── repos/                # populated by fetch_fixtures.sh; gitignored
└── results/
    ├── .gitkeep
    ├── baseline-<host>-<utc-date>.json   # produced by run_baseline.sh
    └── SPIKE-VERDICT-<utc-date>.md       # produced by aggregate.sh
```

`bench/fixtures/repos/` is excluded by the workspace `.gitignore`.
`bench/fixtures/.gitkeep` and `bench/results/.gitkeep` keep the parent
directories tracked.

---

## Methodology — what we are measuring and why

### Repos as fixtures

| Fixture     | Pinned commit                              | Size        | Why we picked it                                                     |
|-------------|--------------------------------------------|-------------|----------------------------------------------------------------------|
| `ripgrep`   | `BurntSushi/ripgrep` v14.1.1               | ~30 MiB     | Smoke test. Tells you the harness works before paying the linux toll.|
| `linux`     | `torvalds/linux` v6.6 (LTS)                | ~5 GiB      | Kernel-hot in MVP-SPEC § 0 success table. ~80k tracked files.        |
| `chromium`  | `chromium/chromium` 120.0.6099.71          | ~25 GiB     | The headline number. ~330k tracked files. Cold + hot in MVP-SPEC § 0.|

Pins are written into `fetch_fixtures.sh` so re-running it on a different
machine reproduces byte-identical workdirs.

### Hot vs cold

- **Hot** = OS page cache warm; we run one untimed warm-up call, then N=200
  timed calls back-to-back. This is the prompt path that the user sees on
  every keypress, and is the number gitstatusd's README quotes.
- **Cold** = page cache dropped between iterations. The orchestrator (i.e. the
  human running this) is responsible for `echo 3 | sudo tee /proc/sys/vm/drop_caches`
  between a small number of timed runs. `run_baseline.sh` is **hot only** by
  design — automating sudo cache drops in a portable shell script is a sharp
  edge we deliberately don't add. The cold number is captured manually,
  pasted into `RESULTS-TEMPLATE.md`. (See § "Cold-cache caveat" below.)

### What we time

`run_baseline.sh` measures wall-clock with `/usr/bin/env time -f '%e'` (GNU)
or `gdate +%s.%N` (macOS, via Homebrew coreutils). If neither is available
the script exits with a clear error and a one-line install hint. We do not
trust shell-builtin `time` — POSIX format and precision are not portable.

Each fixture gets:
- **N = 200** hot iterations, single-threaded, sequential.
- **1** untimed warm-up before the timed loop.
- mean, p50, p95, min, max, in milliseconds, emitted as one JSON object per
  fixture.

The criterion harness in `crates/spike-gitstatus/` is expected to emit
comparable per-fixture JSON in `target/criterion/<bench>/new/estimates.json`.
`aggregate.sh` joins the two on fixture name.

### Cold-cache caveat

Dropping the OS page cache portably across Linux + macOS + WSL2 is
non-trivial:

- Linux: `echo 3 | sudo tee /proc/sys/vm/drop_caches` (needs root).
- macOS: `sudo purge` (needs root, takes seconds).
- WSL2: `drop_caches` works but the host Windows cache layer is not flushed.

Rather than embed sudo into the harness, we **document** the cold-run
procedure in `RESULTS-TEMPLATE.md` and leave it to the contractor to record
those numbers manually. This keeps the harness ShellCheck-clean and free of
surprise privilege escalation.

---

## End-to-end run

```sh
# Once, per machine:
chmod +x bench/fetch_fixtures.sh bench/run_baseline.sh bench/aggregate.sh

# Step 1 — fetch fixtures. ripgrep alone is enough for smoke testing.
./bench/fetch_fixtures.sh                    # ripgrep only
./bench/fetch_fixtures.sh --with-linux       # adds linux (~5 GB)
./bench/fetch_fixtures.sh --with-chromium    # adds chromium (~25 GB, slow)

# Step 2 — run the Rust criterion harness (other contractor's deliverable).
cargo bench -p spike-gitstatus

# Step 3 — run the gitstatusd baseline.
#   Resolution order for the daemon binary:
#     1. $GITSTATUSD_BIN
#     2. /home/seaburdz/github/powerlevel10k/gitstatus/usrbin/gitstatusd-linux-x86_64
#     3. `command -v gitstatusd` on PATH
./bench/run_baseline.sh

# Step 4 — fold criterion + baseline into a verdict table.
./bench/aggregate.sh

# Step 5 — copy the template, fill in env + decision.
cp bench/RESULTS-TEMPLATE.md "bench/results/spike-report-$(date -u +%Y%m%d).md"
$EDITOR "bench/results/spike-report-$(date -u +%Y%m%d).md"
```

The `chmod +x` is the orchestrator's job (we don't ship file modes through
git review cleanly), so do it once per checkout.

---

## Cross-references

- `MVP-SPEC.md` § 0 — success criteria table this harness is graded against.
  The thresholds `aggregate.sh` enforces (`ms`):

  | Scenario          | gitstatusd target | gix-only allowable | hybrid target |
  |-------------------|------------------:|-------------------:|--------------:|
  | Chromium hot      | 30.9             | 100                | 60            |
  | Chromium cold     | 291              | 600                | 400           |
  | Linux kernel hot  | ~25              | 80                 | 50            |

- `07-gitstatus.md` § "Performance recipe" — the three perf tricks this spike
  is trying to determine whether we have to reproduce.
- `09-rust-ecosystem.md` § "Single biggest risk" — the framing for why this
  spike exists at all.

---

## Portability matrix

| Tool needed       | Linux            | macOS            | Notes                                                |
|-------------------|------------------|------------------|------------------------------------------------------|
| `bash` 4+         | default          | install via brew | scripts use `set -euo pipefail`; `bash` is a hard dep|
| `git`             | default          | default          | needed by `fetch_fixtures.sh`                        |
| GNU `time`        | `/usr/bin/time`  | `gtime` (brew)   | preferred timer; `gdate +%s.%N` is the fallback      |
| `awk`             | default          | default          | replaces `jq` for the small JSON we produce          |

If you find yourself reaching for `jq`, stop. The JSON is intentionally
shallow so `awk` handles it; adding `jq` as a dep makes onboarding a fresh
Ubuntu CI image one `apt` away from broken.

---

## Non-goals for this directory

- No CI wiring. (That's `.github/workflows/`, not here.)
- No Rust. (That's `crates/spike-gitstatus/`.)
- No invasive sudo / cache-drop automation. Documented in
  `RESULTS-TEMPLATE.md`, run by the human.
- No `curl | bash`. All network access is `git clone` against pinned
  commits, with the host visible in plain text.
