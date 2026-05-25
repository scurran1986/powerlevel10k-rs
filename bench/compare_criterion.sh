#!/usr/bin/env bash
# compare_criterion.sh — compare two criterion run trees and emit a verdict.
#
# Inputs:
#   $1 = path to BASELINE criterion dir (e.g. baseline/target/criterion)
#   $2 = path to CURRENT  criterion dir (e.g. target/criterion)
#
# Env (thresholds, all percentages):
#   PERF_FAIL_PCT  — regression at or above this fails the script (default 25)
#   PERF_WARN_PCT  — regression at or above this emits a warning   (default 10)
#
# Output:
#   - Markdown table on stdout (suitable for $GITHUB_STEP_SUMMARY).
#   - GitHub workflow annotations (::warning::, ::error::) on stderr.
#   - Exit code:
#       0 = no benchmark regressed at/above PERF_FAIL_PCT (warnings ok).
#       1 = at least one benchmark regressed at/above PERF_FAIL_PCT.
#       2 = usage / IO error.
#
# Methodology:
#   For every `<group>/<bench>/new/estimates.json` in the CURRENT tree,
#   look up the matching baseline file. Parse criterion's `mean.point_estimate`
#   (nanoseconds) out of both files. Compute pct_change = (cur - base) / base
#   * 100. Positive = slower, negative = faster.
#
#   Missing baseline rows are reported as "new" — never fail on them.
#   Missing current rows are reported as "removed" — never fail on them.
#
# Dependencies: bash 4+, awk. No jq. (Matches bench/README.md non-goals.)

set -euo pipefail
LC_ALL=C
export LC_ALL

if [ "$#" -ne 2 ]; then
  printf 'usage: %s <baseline_criterion_dir> <current_criterion_dir>\n' "$0" >&2
  exit 2
fi

BASE_DIR="$1"
CUR_DIR="$2"
PERF_FAIL_PCT="${PERF_FAIL_PCT:-25}"
PERF_WARN_PCT="${PERF_WARN_PCT:-10}"

if [ ! -d "$CUR_DIR" ]; then
  printf 'compare_criterion.sh: current criterion dir missing: %s\n' "$CUR_DIR" >&2
  exit 2
fi

# Pull `mean.point_estimate` (ns) from a criterion estimates.json. Lifted
# directly from bench/aggregate.sh so the parser stays in one shape across
# tools — see that script for the line-by-line commentary.
criterion_mean_ns() {
  local est="$1"
  [ -f "$est" ] || return 0
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
        printf "%.6f\n", s + 0
      }
    }
  ' "$est"
}

# Enumerate every `<group>/<bench>/new/estimates.json` under a criterion
# tree, printing one "group/bench" key per line. Criterion writes a
# top-level `report/` dir we want to skip.
list_benches() {
  local root="$1"
  [ -d "$root" ] || return 0
  # Criterion writes one estimates.json per snapshot at:
  #   <root>/<group>/<bench>/new/estimates.json   (depth 4 below <root>)
  # Stable sort so the table order is deterministic across runs.
  find "$root" -mindepth 4 -maxdepth 4 -type f -name estimates.json \
    -path '*/new/estimates.json' 2>/dev/null \
    | awk -F/ '{
        n = NF
        # path tail looks like .../<group>/<bench>/new/estimates.json
        print $(n-3) "/" $(n-2)
      }' \
    | sort -u
}

# Emit a single GitHub Actions workflow command. The `::` prefix is
# the magic that turns a stdout line into an annotation; we route to
# stderr so the human-readable Markdown stays on stdout uncluttered.
gha_annotation() {
  local level="$1"   # warning | error | notice
  local msg="$2"
  printf '::%s::%s\n' "$level" "$msg" >&2
}

# Sole side channel: $regression_count gets bumped from inside the loop
# via a temp file. Subshell + here-string handling in bash makes a plain
# variable awkward across the while-read.
state_dir="$(mktemp -d)"
trap 'rm -rf "$state_dir"' EXIT
echo 0 > "$state_dir/fail"
echo 0 > "$state_dir/warn"
echo 0 > "$state_dir/new"
echo 0 > "$state_dir/removed"
echo 0 > "$state_dir/ok"

printf '## Bench regression report\n\n'
printf 'Thresholds: **fail ≥ %s%%**, warn ≥ %s%% (positive = slower).\n\n' \
  "$PERF_FAIL_PCT" "$PERF_WARN_PCT"
printf '| Bench | Baseline (ms) | Current (ms) | Δ%% | Verdict |\n'
printf '|---|---:|---:|---:|---|\n'

current_benches="$(list_benches "$CUR_DIR")"
baseline_benches="$(list_benches "$BASE_DIR" 2>/dev/null || true)"

# Union of bench keys, sorted, deduped — so "removed" rows surface too.
all_benches="$(printf '%s\n%s\n' "$current_benches" "$baseline_benches" \
  | sort -u | sed '/^$/d')"

while IFS= read -r bench; do
  [ -n "$bench" ] || continue
  cur_path="$CUR_DIR/$bench/new/estimates.json"
  base_path="$BASE_DIR/$bench/new/estimates.json"

  cur_ns="$(criterion_mean_ns "$cur_path" 2>/dev/null || true)"
  base_ns="$(criterion_mean_ns "$base_path" 2>/dev/null || true)"

  # ns → ms for display.
  ns_to_ms() {
    awk -v ns="$1" 'BEGIN { if (ns == "") print ""; else printf "%.4f", ns / 1.0e6 }'
  }
  cur_ms="$(ns_to_ms "$cur_ns")"
  base_ms="$(ns_to_ms "$base_ns")"

  if [ -z "$cur_ns" ] && [ -n "$base_ns" ]; then
    printf '| `%s` | %s | (missing) | – | removed |\n' "$bench" "$base_ms"
    echo $(( $(cat "$state_dir/removed") + 1 )) > "$state_dir/removed"
    continue
  fi
  if [ -n "$cur_ns" ] && [ -z "$base_ns" ]; then
    printf '| `%s` | (missing) | %s | – | new (baseline-seed) |\n' "$bench" "$cur_ms"
    echo $(( $(cat "$state_dir/new") + 1 )) > "$state_dir/new"
    continue
  fi
  if [ -z "$cur_ns" ] && [ -z "$base_ns" ]; then
    # Should be unreachable given the union, but guard anyway.
    continue
  fi

  # Both sides present — compute the percent delta.
  pct="$(awk -v c="$cur_ns" -v b="$base_ns" 'BEGIN {
    if (b + 0 == 0) { print "0.00"; exit }
    printf "%.2f", (c - b) / b * 100.0
  }')"

  verdict_kind="$(awk -v p="$pct" -v warn="$PERF_WARN_PCT" -v fail="$PERF_FAIL_PCT" 'BEGIN {
    if (p + 0 >= fail + 0)      print "fail"
    else if (p + 0 >= warn + 0) print "warn"
    else                        print "ok"
  }')"

  case "$verdict_kind" in
    fail)
      printf '| `%s` | %s | %s | %s%% | **FAIL** |\n' \
        "$bench" "$base_ms" "$cur_ms" "$pct"
      gha_annotation error \
        "perf regression: $bench is ${pct}% slower than baseline (${base_ms} ms → ${cur_ms} ms, threshold ${PERF_FAIL_PCT}%)"
      echo $(( $(cat "$state_dir/fail") + 1 )) > "$state_dir/fail"
      ;;
    warn)
      printf '| `%s` | %s | %s | %s%% | warn |\n' \
        "$bench" "$base_ms" "$cur_ms" "$pct"
      gha_annotation warning \
        "perf regression: $bench is ${pct}% slower than baseline (${base_ms} ms → ${cur_ms} ms, threshold ${PERF_WARN_PCT}%)"
      echo $(( $(cat "$state_dir/warn") + 1 )) > "$state_dir/warn"
      ;;
    ok)
      printf '| `%s` | %s | %s | %s%% | ok |\n' \
        "$bench" "$base_ms" "$cur_ms" "$pct"
      echo $(( $(cat "$state_dir/ok") + 1 )) > "$state_dir/ok"
      ;;
  esac
done <<EOF
$all_benches
EOF

fail_count="$(cat "$state_dir/fail")"
warn_count="$(cat "$state_dir/warn")"
new_count="$(cat "$state_dir/new")"
removed_count="$(cat "$state_dir/removed")"
ok_count="$(cat "$state_dir/ok")"

printf '\n**Summary:** %s ok, %s warn, %s fail, %s new, %s removed.\n' \
  "$ok_count" "$warn_count" "$fail_count" "$new_count" "$removed_count"

if [ "$fail_count" -gt 0 ]; then
  printf '\nAt least one benchmark exceeded the %s%% regression threshold.\n' \
    "$PERF_FAIL_PCT"
  exit 1
fi
exit 0
