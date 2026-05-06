#!/usr/bin/env bash
# aggregate.sh — fold the latest gitstatusd baseline JSON together with the
# latest criterion JSON and emit a Markdown verdict table.
#
# Inputs (auto-discovered):
#   - latest bench/results/baseline-*.json
#   - latest target/criterion/<bench>/new/estimates.json (per fixture)
#
# Output:
#   bench/results/SPIKE-VERDICT-<utc-date>.md
#
# Convention: criterion benchmark group names match fixture directory names,
# with a `_gix` or `_hybrid` suffix:
#   - <fixture>_gix_hot      — gix-only implementation, hot path
#   - <fixture>_hybrid_hot   — gix + rustix walker, hot path
#
# Cells that miss the MVP-SPEC § 0 thresholds are prefixed **(MISS)** in bold.
#
# Dependencies: bash, awk. No jq.

set -euo pipefail
LC_ALL=C
export LC_ALL

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
RESULTS_DIR="${SCRIPT_DIR}/results"
CRITERION_DIR="${WORKSPACE_ROOT}/target/criterion"

# ---- MVP-SPEC § 0 thresholds (ms) -----------------------------------------
# scenario              gix-only allowable    hybrid target
THRESH_chromium_hot_gix=100
THRESH_chromium_hot_hybrid=60
THRESH_chromium_cold_gix=600
THRESH_chromium_cold_hybrid=400
THRESH_linux_hot_gix=80
THRESH_linux_hot_hybrid=50
# ripgrep is a sanity check; no published threshold. Treat as informational.

# ---- Locate latest baseline JSON ------------------------------------------
latest_baseline() {
  # Prints the most-recently-modified baseline JSON, or empty.
  # Don't let `ls` failure (no glob match) trip pipefail / errexit.
  ls -1t "$RESULTS_DIR"/baseline-*.json 2>/dev/null | head -n 1 || true
}

BASELINE_JSON="$(latest_baseline || true)"
if [ -z "$BASELINE_JSON" ]; then
  printf 'aggregate.sh: no baseline-*.json found in %s\n' "$RESULTS_DIR" >&2
  printf 'Run ./bench/run_baseline.sh first.\n' >&2
  exit 66
fi
printf '[info] baseline: %s\n' "$BASELINE_JSON"

# ---- Pull fixture -> mean_ms from baseline JSON --------------------------
# Format we wrote ourselves in run_baseline.sh — shallow JSON, awk-parseable.
# The "fixtures" object maps name -> {n, mean_ms, p50_ms, p95_ms, min_ms, max_ms}.
baseline_metric() {
  fixture="$1"
  metric="$2"   # e.g. mean_ms
  awk -v fixture="$fixture" -v metric="$metric" '
    BEGIN { in_fix = 0 }
    /"fixtures": *\{/ { in_fix = 1; next }
    in_fix && $0 ~ "\"" fixture "\"" {
      # line looks like:    "linux": {"n":200,"mean_ms":24.7,...}
      if (match($0, "\"" metric "\":[ ]*[-0-9.]+")) {
        s = substr($0, RSTART, RLENGTH)
        sub("\"" metric "\":[ ]*", "", s)
        print s
        exit
      }
    }
  ' "$BASELINE_JSON"
}

# ---- Pull point estimate from a criterion estimates.json -----------------
# Criterion writes a JSON object that contains, among other things,
# "mean": { "point_estimate": <nanoseconds>, ... }. We want point_estimate
# in milliseconds.
criterion_mean_ms() {
  bench_name="$1"
  est="${CRITERION_DIR}/${bench_name}/new/estimates.json"
  [ -f "$est" ] || return 0

  # criterion's estimates.json is one-line minified JSON of the shape:
  #   {"mean":{...,"point_estimate":<ns>,...},"median":{...},...}
  # We want the point_estimate that lives inside "mean":{...}. Scan the
  # whole file as one buffer, isolate the substring after `"mean":{`,
  # capture the first "point_estimate":<num> inside it, convert ns to ms.
  awk '
    BEGIN { RS = "\0" }
    {
      buf = $0
      i = index(buf, "\"mean\"")
      if (i == 0) exit
      tail = substr(buf, i)
      j = index(tail, "{")
      if (j == 0) exit
      tail = substr(tail, j)
      # Find matching close-brace for the mean object.
      depth = 0
      end = 0
      for (k = 1; k <= length(tail); k++) {
        c = substr(tail, k, 1)
        if (c == "{") depth++
        else if (c == "}") { depth--; if (depth == 0) { end = k; break } }
      }
      if (end == 0) exit
      mean_obj = substr(tail, 1, end)
      if (match(mean_obj, /"point_estimate"[[:space:]]*:[[:space:]]*[-0-9.eE+]+/)) {
        s = substr(mean_obj, RSTART, RLENGTH)
        sub(/"point_estimate"[[:space:]]*:[[:space:]]*/, "", s)
        # criterion stores nanoseconds; convert to milliseconds.
        printf "%.4f\n", s / 1.0e6
      }
    }
  ' "$est"
}

# ---- Format one cell, marking misses --------------------------------------
# Args: value_ms threshold_ms
fmt_cell() {
  v="$1"
  thresh="$2"

  if [ -z "$v" ] || [ "$v" = "null" ]; then
    printf 'n/a'
    return 0
  fi

  if [ -z "$thresh" ]; then
    printf '%s ms' "$v"
    return 0
  fi

  miss="$(awk -v v="$v" -v t="$thresh" 'BEGIN { print (v + 0 > t + 0) ? "1" : "0" }')"
  if [ "$miss" = "1" ]; then
    printf '**(MISS)** %s ms (>%s ms)' "$v" "$thresh"
  else
    printf '%s ms (≤%s ms)' "$v" "$thresh"
  fi
}

# ---- Build the verdict file -----------------------------------------------
UTC_DATE="$(date -u +%Y%m%dT%H%M%SZ)"
OUT="${RESULTS_DIR}/SPIKE-VERDICT-${UTC_DATE}.md"

{
  printf '# Spike Verdict — %s (UTC)\n\n' "$UTC_DATE"
  printf 'Generated by `bench/aggregate.sh`. Joins:\n\n'
  printf '%s\n' "- baseline: \`${BASELINE_JSON#"$WORKSPACE_ROOT/"}\`"
  printf '%s\n\n' "- criterion: \`target/criterion/<bench>/new/estimates.json\`"
  printf 'Thresholds come from `MVP-SPEC.md` § 0 "Day-1 Spike". '
  printf 'Cells in **(MISS)** prefix exceed the threshold and require pivot consideration.\n\n'

  printf '## Hot-path comparison\n\n'
  printf '| Fixture | gitstatusd (mean) | gix-only (mean) | hybrid (mean) |\n'
  printf '|---|---|---|---|\n'

  # Build rows for every fixture present in the baseline JSON.
  awk '
    /"fixtures": *\{/ { in_fix = 1; next }
    in_fix && match($0, /"[a-zA-Z0-9_-]+": *\{/) {
      name = substr($0, RSTART + 1, RLENGTH - 5)
      sub(/": *\{$/, "", name)
      print name
    }
  ' "$BASELINE_JSON" | while read -r fixture; do
    [ -n "$fixture" ] || continue

    base_ms="$(baseline_metric "$fixture" mean_ms || printf '')"
    gix_ms="$(criterion_mean_ms "${fixture}_gix_hot")"
    hyb_ms="$(criterion_mean_ms "${fixture}_hybrid_hot")"

    # Pick the right thresholds for this fixture's hot row.
    thresh_gix=""
    thresh_hyb=""
    case "$fixture" in
      chromium) thresh_gix="$THRESH_chromium_hot_gix";  thresh_hyb="$THRESH_chromium_hot_hybrid" ;;
      linux)    thresh_gix="$THRESH_linux_hot_gix";     thresh_hyb="$THRESH_linux_hot_hybrid" ;;
      ripgrep)  thresh_gix=""; thresh_hyb="" ;;
      *)        thresh_gix=""; thresh_hyb="" ;;
    esac

    base_cell="$(fmt_cell "$base_ms" "")"
    gix_cell="$(fmt_cell "$gix_ms" "$thresh_gix")"
    hyb_cell="$(fmt_cell "$hyb_ms" "$thresh_hyb")"

    printf '| `%s` | %s | %s | %s |\n' "$fixture" "$base_cell" "$gix_cell" "$hyb_cell"
  done

  printf '\n## Cold-path comparison (manual entry)\n\n'
  printf 'Cold numbers are not collected automatically. See '
  printf '`bench/RESULTS-TEMPLATE.md` § "Cold-cache procedure" and paste the\n'
  printf 'measured values here:\n\n'
  printf '| Fixture | gitstatusd cold | gix-only cold | hybrid cold |\n'
  printf '|---|---|---|---|\n'
  printf '| `chromium` | _fill in_ | _fill in_ (≤%s ms) | _fill in_ (≤%s ms) |\n' \
    "$THRESH_chromium_cold_gix" "$THRESH_chromium_cold_hybrid"

  printf '\n## Decision tree (from `MVP-SPEC.md` § 0)\n\n'
  printf '%s\n' '- Hybrid hits all three targets → **GO** (proceed with `p10k-rs-git`).'
  printf '%s\n' '- gix-only hits all three targets → **GO** (drop the rustix walker, simpler arch).'
  printf '%s\n' '- Hybrid misses kernel-hot by < 2× → **HYBRID** (proceed; document gap).'
  printf '%s\n' '- Hybrid misses chromium-hot by > 3× → **PIVOT** (talk to gitoxide; consider FFI to C++ daemon).'

  printf '\n_Final signed verdict goes in `bench/RESULTS-TEMPLATE.md`._\n'
} > "$OUT"

printf '[done] wrote %s\n' "$OUT"
