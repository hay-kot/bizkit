#!/usr/bin/env bash
# Runs every analytics ingest benchmark scenario (see the plan in
# .hive/plans/2026-11-23-analytics-ingest-bench.md) into one results dir,
# then prints the markdown tables. About 10 minutes at the default 10 s per
# run; set BENCH_DURATION for longer runs.
#
#   scripts/bench-ingest-matrix.sh [out-dir] [scenario ...]
#
# Scenarios: steady sync shared burst mixed kill (default: all), and windows:
# buffered and hybrid at 100 and 250 ms flush through steady, burst, mixed
# and kill.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$root/target/bench-ingest/$(date +%Y%m%d-%H%M%S)}"
shift || true
scenarios=("$@")
[[ ${#scenarios[@]} -eq 0 ]] && scenarios=(steady sync shared burst mixed kill)
duration=${BENCH_DURATION:-10}
half=$((duration / 2))
quarter=$((duration / 4 > 0 ? duration / 4 : 1))
built=""

# case <scenario> <design> <rate> <projects> [extra serve flags] [extra load flags] [kill-at] [layout] [sync] [flush-ms]
case_run() {
  local scenario=$1 design=$2 rate=$3 projects=$4 serve_extra=${5:-} load_extra=${6:-} kill_at=${7:-}
  local layout=${8:-per-project} sync=${9:-normal} flush=${10:-250}
  local name="$scenario-$design-$layout-$sync-f$flush-r$rate-p$projects"
  local serve="--design $design --layout $layout --sync $sync --flush-ms $flush $serve_extra"
  local load="--rate $rate --projects $projects --duration $duration --scenario $scenario \
--label-design $design --label-layout $layout --label-sync $sync --label-flush-ms $flush $load_extra"
  BENCH_SKIP_BUILD=$built "$root/scripts/bench-ingest.sh" "$out" "$name" "$serve" "$load" "$kill_at"
  built=1
}

designs=(write-through batched buffered)
for scenario in "${scenarios[@]}"; do
  case $scenario in
    steady)
      # The load curve at 10 projects, plus project count at the top rate.
      for rate in 500 2000 5000 10000; do
        for design in "${designs[@]}"; do case_run steady "$design" "$rate" 10; done
      done
      for projects in 1 100; do
        for design in "${designs[@]}"; do case_run steady "$design" 5000 "$projects"; done
      done ;;
    sync)
      for design in batched buffered; do
        case_run sync "$design" 5000 10 "" "" "" per-project full
      done ;;
    shared)
      for design in "${designs[@]}"; do case_run shared "$design" 5000 10 "" "" "" shared; done ;;
    burst)
      for design in "${designs[@]}"; do
        case_run burst "$design" 500 10 "" "--burst-x 10 --burst-at $quarter --burst-for $half"
      done ;;
    mixed)
      for design in "${designs[@]}"; do
        case_run mixed "$design" 2000 10 "" "--queries-per-s 10 --rollup-every $half"
      done ;;
    kill)
      for design in "${designs[@]}"; do case_run kill "$design" 2000 10 "" "" "$half"; done
      for flush in 100 500; do case_run kill buffered 2000 10 "" "" "$half" per-project normal "$flush"; done ;;
    windows)
      for flush in 100 250; do
        for design in buffered hybrid; do
          w=(per-project normal "$flush")
          for rate in 500 2000 5000 10000; do case_run steady "$design" "$rate" 10 "" "" "" "${w[@]}"; done
          for projects in 1 100; do case_run steady "$design" 5000 "$projects" "" "" "" "${w[@]}"; done
          case_run burst "$design" 500 10 "" "--burst-x 10 --burst-at $quarter --burst-for $half" "" "${w[@]}"
          case_run mixed "$design" 2000 10 "" "--queries-per-s 10 --rollup-every $half" "" "${w[@]}"
          case_run kill "$design" 2000 10 "" "" "$half" "${w[@]}"
        done
      done ;;
    *) echo "unknown scenario: $scenario" >&2; exit 2 ;;
  esac
done

"$root/target/release/bizkit-bench" report "$out"
echo "results: $out" >&2
