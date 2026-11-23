#!/usr/bin/env bash
# Runs one analytics ingest benchmark scenario: the bizkit-bench server in a
# container capped at 2 CPUs and 1 GB, the load generator on the host.
#
#   scripts/bench-ingest.sh <out-dir> <run-name> "<serve flags>" "<load flags>" [kill-at-seconds]
#
# serve flags go to `bizkit-bench serve` (--design, --layout, --sync, --flush-ms ...).
# load flags go to `bizkit-bench load` (--rate, --projects, --duration, labels ...).
# With kill-at, the container gets SIGKILL that many seconds into the load,
# and `finish` counts acked events no file holds.
# BENCH_SKIP_BUILD=1 reuses the built binaries and image.
set -euo pipefail

out_dir=$1 run=$2 serve_args=$3 load_args=$4 kill_at=${5:-}
root="$(cd "$(dirname "$0")/.." && pwd)"
image=bizkit-bench
container=bizkit-bench-run
volume="bizkit-bench-$run"
port=8600
target=aarch64-unknown-linux-musl
[[ "$(docker info --format '{{.Architecture}}')" == "x86_64" ]] && target=x86_64-unknown-linux-musl

log() { printf '\n==> %s\n' "$*" >&2; }

mkdir -p "$out_dir"
out_dir="$(cd "$out_dir" && pwd)"

if [[ -z "${BENCH_SKIP_BUILD:-}" ]]; then
  log "building bizkit-bench for $target and the host"
  (cd "$root" && cargo zigbuild --release --locked -p bizkit-bench --target "$target")
  (cd "$root" && cargo build --release --locked -p bizkit-bench)
  docker build -q -t "$image" -f "$root/docker/bench.Dockerfile" "$root/target/$target/release" >/dev/null
fi
bench="$root/target/release/bizkit-bench"

docker rm -f "$container" >/dev/null 2>&1 || true
docker volume rm -f "$volume" >/dev/null 2>&1 || true
docker volume create "$volume" >/dev/null

sampler=""
cleanup() {
  [[ -n "$sampler" ]] && kill "$sampler" 2>/dev/null || true
  docker rm -f "$container" >/dev/null 2>&1 || true
  docker volume rm -f "$volume" >/dev/null 2>&1 || true
}
trap cleanup EXIT

log "$run: serve $serve_args"
# shellcheck disable=SC2086 # flags are word-split on purpose
docker run -d --name "$container" --cpus=2 --memory=1g --memory-swap=1g \
  -p "127.0.0.1:$port:$port" -v "$volume:/data" "$image" \
  serve --data /data --listen "0.0.0.0:$port" $serve_args >/dev/null
for _ in $(seq 1 100); do
  curl -fsS "http://127.0.0.1:$port/healthz" >/dev/null 2>&1 && break
  sleep 0.1
done

# Warmup: same flags, seq IDs far from the measured run's.
# shellcheck disable=SC2086
"$bench" load --url "http://127.0.0.1:$port" $load_args --duration 5 --app-per-s 0 \
  --seq-start 1000000000 --out "$out_dir/.warmup.json" >/dev/null
rm -f "$out_dir/.warmup.json" "$out_dir/.warmup.acked"

: >"$out_dir/$run.stats"
(while docker stats --no-stream --format '{{.CPUPerc}},{{.MemUsage}}' "$container" >>"$out_dir/$run.stats" 2>/dev/null; do
  sleep 0.5
done) &
sampler=$!

log "$run: load $load_args"
# shellcheck disable=SC2086
"$bench" load --url "http://127.0.0.1:$port" $load_args --out "$out_dir/$run.json" &
loader=$!
if [[ -n "$kill_at" ]]; then
  sleep "$kill_at"
  log "$run: SIGKILL at ${kill_at}s"
  docker kill -s KILL "$container" >/dev/null
fi
wait "$loader"
kill "$sampler" 2>/dev/null || true
sampler=""
docker rm -f "$container" >/dev/null 2>&1 || true

finish_args=(finish --run "/out/$run.json" --docker-stats "/out/$run.stats")
[[ -n "$kill_at" ]] && finish_args+=(--data /data)
docker run --rm -v "$volume:/data" -v "$out_dir:/out" "$image" "${finish_args[@]}"
