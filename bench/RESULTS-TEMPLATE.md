# Spike Report — `p10k-rs` Day-1 Latency Spike

> Copy this template to `bench/results/spike-report-<UTC-date>.md`, fill it in
> with real numbers, and commit the filled copy. Do **not** edit this template
> in place.

**Spike contractor:** _your name here_
**Date (UTC):** _YYYY-MM-DD_
**Run ID:** _matches `bench/results/baseline-<host>-<utc>.json` filename_

---

## 1. Environment

| Field | Value |
|---|---|
| OS / distro | _e.g. Ubuntu 22.04.4 LTS_ |
| Kernel | _`uname -srm`_ |
| CPU | _`lscpu \| grep 'Model name'` — include core/thread count_ |
| RAM | _`free -h` total_ |
| Filesystem (workdir) | _`stat -f -c %T <fixture>` (Linux) / `mount` line_ |
| Storage type | _NVMe / SATA SSD / spinning / tmpfs_ |
| `gitstatusd` version | _`gitstatusd --version`_ |
| `gitstatusd` build | _vendored / system / built-from-source @ commit_ |
| `gix` version | _`cargo tree -p gix --depth 0`_ |
| `rustc` | _`rustc --version` (full string)_ |
| `cargo` | _`cargo --version`_ |
| Build profile | _release / release-with-debug / lto=fat?_ |

---

## 2. Methodology

### 2.1 Hot iterations

- N = _200_ per fixture (configurable via `BENCH_ITERATIONS`).
- 1 untimed warm-up call before the timed loop.
- Single-threaded (`--num-threads=1`) for apples-to-apples vs. our prompt's
  spawn-per-prompt latency budget. Multi-threaded numbers belong in a
  follow-up.

### 2.2 Cold-cache procedure (manual)

The harness deliberately does not automate cache drops. To capture cold
numbers:

```sh
# Linux
sync
echo 3 | sudo tee /proc/sys/vm/drop_caches
./run_one_cold_iteration.sh chromium     # whatever wrapper you used

# macOS
sudo purge
```

Repeat ≥ 5 times per fixture; report the median. Note any outliers and
suspected cause (background indexer? Dropbox? Spotlight?).

### 2.3 Timing source

_e.g. `/usr/bin/time -f '%e'` on Linux x86_64 — sub-millisecond precision
verified by timing `true` (~0.001 s reported)._

### 2.4 Anything non-default

- _libgit2 fork pinned commit, if rebuilt:_ _`<sha>`_
- _Filesystem caveats:_ _e.g. WSL2 — not native Linux; numbers may regress
  on real hardware._
- _Background load during run:_ _e.g. machine quiesced, screen saver off._

---

## 3. Raw numbers

> Paste the JSON from `bench/results/baseline-<host>-<utc>.json` and the
> per-bench `target/criterion/<name>/new/estimates.json` summaries here, or
> link to them. The aggregator's `SPIKE-VERDICT-<utc>.md` table goes below.

### 3.1 Hot path (mean ms)

| Fixture   | gitstatusd | gix-only | hybrid | gix/gitstatusd | hybrid/gitstatusd |
|-----------|-----------:|---------:|-------:|---------------:|------------------:|
| ripgrep   |            |          |        |                |                   |
| linux     |            |          |        |                |                   |
| chromium  |            |          |        |                |                   |

### 3.2 Cold path (mean ms, n=≥5, median)

| Fixture   | gitstatusd | gix-only | hybrid |
|-----------|-----------:|---------:|-------:|
| chromium  |            |          |        |

### 3.3 Distribution shape

_Note any p95/p99 weirdness. A clean repo with bimodal latency usually means
either GC kicked in mid-run or we have a code path that occasionally falls
through to a slower walker._

---

## 4. Decision tree (from `MVP-SPEC.md` § 0)

| Outcome | What it means | Action |
|---|---|---|
| Hybrid hits all three targets | gix + rustix walker is fast enough | **GO** — proceed with `p10k-rs-git` per `ARCHITECTURE.md` |
| gix-only hits all three | walker is unnecessary; simpler is faster than expected | **GO** — drop the rustix walker, document the win |
| Hybrid misses kernel-hot by < 2× | We're close, not done | **HYBRID** — proceed, file optimization tickets, document |
| Hybrid misses chromium-hot by > 3× | gix is fundamentally not in the same league | **PIVOT** — talk to gitoxide maintainers; consider FFI-binding the existing C++ daemon as a fallback shim |

Mark the row that applies and explain in one paragraph below.

**Selected outcome:** _row name_

**Reasoning:** _one paragraph_

---

## 5. Recommended path forward

_Concrete bullets the next contractor can act on. Examples:_

- Land `crates/p10k-rs-git/` skeleton with the gix-only API; gate the
  rustix walker behind a feature flag; benchmark in CI.
- Open issue against `gitoxide/gix-status` requesting a `Decision::Skip`
  variant for the cap-and-stop diff path.
- Pin libgit2 fork commit to `<sha>` in `vendor/`; verify SHA-1 perf
  toggle works on aarch64.
- _etc._

---

## 6. Follow-up risks

_Things you saw but didn't have time to confirm. Anything you'd want a
second pair of eyes on. Examples:_

- p99 latency on hybrid is 4× p50; suspect rayon worker stealing under
  contention. Needs dedicated investigation.
- macOS APFS gives noticeably different mtime granularity than ext4;
  the per-dir untracked-cache may behave differently and should be
  re-benched on a real Mac before v0.1.
- Chromium fixture commit chosen for size, not realism; consider whether
  v8/skia submodules need their own benchmark fixture.

---

## 7. Contractor verdict

> **Contractor verdict: GO / HYBRID / PIVOT — _one sentence_**

Signed: ____________________
Date:   ____________________
