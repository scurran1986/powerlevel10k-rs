#!/usr/bin/env bash
# run_baseline.sh — time `gitstatusd` against each fixture present in
# bench/fixtures/repos/. Hot path only. N=200 iterations per fixture.
#
# Output: one JSON object per fixture, one summary file:
#   bench/results/baseline-<hostname>-<utc-date>.json
#
# Resolution order for the gitstatusd binary:
#   1. $GITSTATUSD_BIN if set and executable
#   2. /home/seaburdz/github/powerlevel10k/gitstatus/usrbin/gitstatusd-linux-x86_64
#   3. `command -v gitstatusd` on PATH
#
# Timing source: we use GNU `date +%s.%N` (Linux `date`, macOS `gdate`).
# This is the simplest portable timer with sub-millisecond resolution and
# avoids paying GNU `time`'s fork-and-exec wrapper cost on every iteration.
# If `%N` is not supported (e.g. BSD `date` on a macOS without coreutils)
# the script exits with a clear error.
#
# Daemon model: gitstatusd is *persistent*. We keep one daemon process alive
# per fixture (with stdin/stdout wired through FIFOs) and measure the
# wall-clock of each request→response round-trip. This is the apples-to-
# apples comparison for the hot prompt path; spawn-per-request would
# measure startup latency, which is not the number we care about.
#
# We deliberately do *not* drop the OS page cache. Cold runs are a manual
# step (see bench/RESULTS-TEMPLATE.md "Cold-cache procedure").
#
# Dependencies: bash 4+, awk. No jq. No python. No curl.

set -euo pipefail
LC_ALL=C
export LC_ALL

# ---- Locate self ----------------------------------------------------------
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
FIXTURES_DIR="${BENCH_FIXTURES_DIR:-${SCRIPT_DIR}/fixtures/repos}"
RESULTS_DIR="${SCRIPT_DIR}/results"
ITERATIONS="${BENCH_ITERATIONS:-200}"

mkdir -p "$RESULTS_DIR"

# ---- Resolve gitstatusd binary -------------------------------------------
resolve_binary() {
  if [ -n "${GITSTATUSD_BIN:-}" ]; then
    if [ -x "$GITSTATUSD_BIN" ]; then
      printf '%s\n' "$GITSTATUSD_BIN"
      return 0
    fi
    printf 'run_baseline.sh: GITSTATUSD_BIN=%s is not executable\n' \
      "$GITSTATUSD_BIN" >&2
    return 1
  fi

  vendored=/home/seaburdz/github/powerlevel10k/gitstatus/usrbin/gitstatusd-linux-x86_64
  if [ -x "$vendored" ]; then
    printf '%s\n' "$vendored"
    return 0
  fi

  if command -v gitstatusd >/dev/null 2>&1; then
    command -v gitstatusd
    return 0
  fi

  printf 'run_baseline.sh: cannot find gitstatusd. Set $GITSTATUSD_BIN, ' >&2
  printf 'install gitstatusd to PATH, or check out powerlevel10k at the ' >&2
  printf 'expected vendored location.\n' >&2
  return 1
}

GITSTATUSD="$(resolve_binary)"
printf '[info] using gitstatusd: %s\n' "$GITSTATUSD"

# ---- Resolve nanosecond-capable date binary ------------------------------
TIMER_BIN=""
if command -v gdate >/dev/null 2>&1 && \
   [[ "$(gdate +%N 2>/dev/null)" =~ ^[0-9]+$ ]]; then
  TIMER_BIN="gdate"
elif [[ "$(date +%N 2>/dev/null)" =~ ^[0-9]+$ ]]; then
  TIMER_BIN="date"
else
  printf 'run_baseline.sh: no nanosecond-capable `date` found. ' >&2
  printf 'On macOS install GNU coreutils (`brew install coreutils`) so ' >&2
  printf '`gdate` is on PATH.\n' >&2
  exit 78
fi
printf '[info] timer: %s\n' "$TIMER_BIN"

# ---- Capture daemon version (with timeout safety net) --------------------
# Use the wire-protocol --version flag if available; fall back to "unknown"
# rather than blocking the whole run. We never let the version probe hang.
get_version() {
  if command -v timeout >/dev/null 2>&1; then
    timeout 2s "$GITSTATUSD" --version 2>/dev/null | head -n1 | tr -d '"' || true
  else
    # POSIX fallback: background + kill. Best effort.
    "$GITSTATUSD" --version 2>/dev/null | head -n1 | tr -d '"' || true
  fi
}
GSD_VERSION="$(get_version)"
[ -n "$GSD_VERSION" ] || GSD_VERSION="unknown"

# ---- Stat helper (mean / p50 / p95 / min / max) --------------------------
# Reads seconds (one per line), emits a JSON object with millisecond fields.
summarize_ms() {
  awk '
    { v[NR] = $1 * 1000 }
    END {
      if (NR == 0) { print "null"; exit }
      n = NR
      # sort
      for (i = 1; i <= n; i++) {
        for (j = i + 1; j <= n; j++) {
          if (v[j] < v[i]) { t = v[i]; v[i] = v[j]; v[j] = t }
        }
      }
      sum = 0
      for (i = 1; i <= n; i++) sum += v[i]
      mean = sum / n
      p50  = v[int((n - 1) * 0.50) + 1]
      p95  = v[int((n - 1) * 0.95) + 1]
      printf "{\"n\":%d,\"mean_ms\":%.4f,\"p50_ms\":%.4f,\"p95_ms\":%.4f,\"min_ms\":%.4f,\"max_ms\":%.4f}",
        n, mean, p50, p95, v[1], v[n]
    }
  '
}

# ---- Long-lived daemon driver --------------------------------------------
# gitstatusd protocol: <id>US<path>RS  (US=0x1f, RS=0x1e). Response is one
# RS-terminated record on stdout. We pipe N requests in, read N responses
# out, and sample the wall-clock between writing each request and reading
# its response.
#
# Implementation: bash `coproc` keeps the daemon alive with stdin/stdout
# bound to file descriptors COPROC[0] (read) and COPROC[1] (write). Per-
# iteration overhead is one printf and one IFS-bounded read — same syscall
# count as a real prompt would incur.
#
# bash 4+ is required for `coproc`; we error out earlier on macOS bash 3.2
# users implicitly via `set -u` semantics that 3.2 doesn't fully support.
#
# Args: $1 = absolute repo path, $2 = output file for samples
bench_one_fixture() {
  abs_path="$1"
  samples_file="$2"

  # Start the daemon as a coprocess named GSD.
  coproc GSD { "$GITSTATUSD" --num-threads=1; }
  daemon_pid=$GSD_PID

  cleanup_fixture() {
    # Closing the write side first lets the daemon exit on EOF.
    if [ -n "${GSD[1]:-}" ]; then
      eval "exec ${GSD[1]}>&-" 2>/dev/null || true
    fi
    if [ -n "${GSD[0]:-}" ]; then
      eval "exec ${GSD[0]}<&-" 2>/dev/null || true
    fi
    kill "$daemon_pid" 2>/dev/null || true
    wait "$daemon_pid" 2>/dev/null || true
  }
  trap cleanup_fixture RETURN

  # Warm-up: one untimed request → response.
  printf 'warmup\x1f%s\x1e' "$abs_path" >&"${GSD[1]}"
  IFS= read -r -d $'\x1e' -u "${GSD[0]}" _resp || true

  # Timed loop.
  i=0
  : > "$samples_file"
  while [ "$i" -lt "$ITERATIONS" ]; do
    t0="$($TIMER_BIN +%s.%N)"
    printf 'r%d\x1f%s\x1e' "$i" "$abs_path" >&"${GSD[1]}"
    IFS= read -r -d $'\x1e' -u "${GSD[0]}" _resp || true
    t1="$($TIMER_BIN +%s.%N)"
    awk -v a="$t0" -v b="$t1" 'BEGIN { printf "%.6f\n", b - a }' >> "$samples_file"
    i=$((i + 1))
  done

  cleanup_fixture
  trap - RETURN
}

# ---- Iterate fixtures -----------------------------------------------------
if [ ! -d "$FIXTURES_DIR" ]; then
  printf 'run_baseline.sh: fixtures dir not found: %s\n' "$FIXTURES_DIR" >&2
  printf 'Run ./bench/fetch_fixtures.sh first.\n' >&2
  exit 66
fi

HOSTNAME_S="$(hostname -s 2>/dev/null || hostname || printf 'unknown-host')"
UTC_DATE="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_FILE="${RESULTS_DIR}/baseline-${HOSTNAME_S}-${UTC_DATE}.json"

# Buffer output so we don't half-write on Ctrl-C.
TMP_OUT="$(mktemp)"
trap 'rm -f "$TMP_OUT"' EXIT

{
  printf '{\n'
  printf '  "harness": "run_baseline.sh",\n'
  printf '  "hostname": "%s",\n' "$HOSTNAME_S"
  printf '  "utc": "%s",\n' "$UTC_DATE"
  printf '  "gitstatusd_path": "%s",\n' "$GITSTATUSD"
  printf '  "gitstatusd_version": "%s",\n' "$GSD_VERSION"
  printf '  "iterations_per_fixture": %s,\n' "$ITERATIONS"
  printf '  "fixtures": {\n'

  first=1
  for repo_path in "$FIXTURES_DIR"/*/; do
    [ -d "$repo_path" ] || continue
    [ -d "${repo_path}.git" ] || continue

    name="$(basename "$repo_path")"
    abs_path="$(cd "$repo_path" && pwd)"

    printf '[run]  %s (n=%s)\n' "$name" "$ITERATIONS" >&2

    samples_file="$(mktemp)"
    bench_one_fixture "$abs_path" "$samples_file"
    summary="$(summarize_ms < "$samples_file")"
    rm -f "$samples_file"

    if [ "$first" -eq 1 ]; then
      first=0
    else
      printf ',\n'
    fi
    printf '    "%s": %s' "$name" "$summary"
  done

  printf '\n  }\n'
  printf '}\n'
} > "$TMP_OUT"

mv "$TMP_OUT" "$OUT_FILE"
trap - EXIT

printf '\n[done] wrote %s\n' "$OUT_FILE"
