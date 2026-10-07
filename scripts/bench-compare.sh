#!/usr/bin/env bash
# Benchmark regression gate (MILESTONE_1 task T1, DECISIONS.md D13).
#
#   scripts/bench-compare.sh <base-pax_cli> <base-scenario-dir> <head-pax_cli> <head-scenario-dir>
#
# Runs both binaries on the same machine, alternating base/head RUNS times,
# and compares the median ms/day. Absolute timings vary too much between
# machines to compare against a stored baseline; a same-runner A/B comparison
# cancels that out. Exits 1 if head is more than MAX_REGRESSION_PCT slower.
set -euo pipefail

base_bin=$1 base_dir=$2 head_bin=$3 head_dir=$4
RUNS=${RUNS:-5}
SCALE=${SCALE:-50000}       # 300k POP rows: large enough to measure, small enough for CI
DAYS=${DAYS:-10}
THREADS=${THREADS:-2}       # fixed thread count keeps runs comparable on shared runners
MAX_REGRESSION_PCT=${MAX_REGRESSION_PCT:-20}

measure() { # <bin> <dir> -> ms/day
  "$1" bench "$2" --days "$DAYS" --scale "$SCALE" --threads "$THREADS" | sed -nE 's/.*: ([0-9.]+) ms\/day.*/\1/p'
}
median() { sort -g | awk '{a[NR]=$1} END {print (NR % 2) ? a[(NR+1)/2] : (a[NR/2] + a[NR/2+1]) / 2}'; }

base_times=() head_times=()
for i in $(seq "$RUNS"); do
  # Alternate the order so drift on a noisy runner affects both sides equally.
  if (( i % 2 )); then
    base_times+=("$(measure "$base_bin" "$base_dir")"); head_times+=("$(measure "$head_bin" "$head_dir")")
  else
    head_times+=("$(measure "$head_bin" "$head_dir")"); base_times+=("$(measure "$base_bin" "$base_dir")")
  fi
done

base=$(printf '%s\n' "${base_times[@]}" | median)
head=$(printf '%s\n' "${head_times[@]}" | median)
change=$(awk -v b="$base" -v h="$head" 'BEGIN {printf "%.1f", (h / b - 1) * 100}')

report="| | median ms/day | runs |
|---|---|---|
| base | $base | ${base_times[*]} |
| head | $head | ${head_times[*]} |

Change: **${change}%** (limit +${MAX_REGRESSION_PCT}%; ${SCALE}x scale, ${DAYS} days, ${THREADS} threads)"
echo "$report"
[ -n "${GITHUB_STEP_SUMMARY:-}" ] && printf '### Benchmark\n\n%s\n' "$report" >> "$GITHUB_STEP_SUMMARY"

if awk -v c="$change" -v m="$MAX_REGRESSION_PCT" 'BEGIN {exit !(c > m)}'; then
  echo "::error title=Benchmark regression::head is ${change}% slower than base (limit ${MAX_REGRESSION_PCT}%)"
  exit 1
fi
