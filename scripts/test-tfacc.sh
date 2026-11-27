#!/usr/bin/env bash
#
# Runs the Terraform provider's acceptance tests with OpenTofu.
#
# Under `svx exec` it uses the running dev stack, so the tests dogfood the
# stack you are looking at. Otherwise it starts a fresh server on a temp
# data dir, as test-e2e.sh does. Either way it seeds a new `team` persona,
# so the tests work in a workspace of their own and never touch the demo.
#
# Usage: [svx exec --] scripts/test-tfacc.sh [go test args...]
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
workdir="$(mktemp -d)"
server_pid=""

# shellcheck source=lib/server.sh
source "$root/scripts/lib/server.sh"

cleanup() {
  [[ -n "$server_pid" ]] && kill "$server_pid" 2>/dev/null || true
  wait 2>/dev/null || true
  rm -rf "$workdir"
}
trap cleanup EXIT

bin="$root/target/debug/bizkit"

if [[ -n "${SVX_URL_API:-}" ]]; then
  log "using the svx stack at $SVX_URL_API"
  # The api service's `cargo run` builds this binary with test hooks.
  endpoint="$SVX_URL_API"
  seed=("$bin" --config "$root/dev/bizkit.yaml" dev --data-dir "$SVX_STACK_DIR/data")
else
  log "building the web app"
  (cd "$root" && mise run web:build)
  log "building bizkit with test hooks"
  (cd "$root" && cargo build --locked -p bizkit --features test-hooks)

  api_port="$(free_port)"
  endpoint="http://127.0.0.1:$api_port"
  cat >"$workdir/bizkit.yaml" <<YAML
log_level: info
server:
  listen: 127.0.0.1:$api_port
  base_url: $endpoint
  data_dir: $workdir/data
  secrets_key_env: BIZKIT_SECRETS_KEY
YAML
  BIZKIT_SECRETS_KEY="$(random_secrets_key)"
  export BIZKIT_SECRETS_KEY
  log "starting bizkit on $api_port"
  "$bin" --no-color --config "$workdir/bizkit.yaml" serve >"$workdir/bizkit.log" 2>&1 &
  server_pid=$!
  wait_for bizkit "$endpoint/healthz" "$server_pid"
  seed=("$bin" --config "$workdir/bizkit.yaml" dev)
fi

namespace="tfacc$(date +%s)"
log "seeding workspace team-$namespace"
"${seed[@]}" seed-persona --persona team --namespace "$namespace" --out "$workdir/manifest.json"

BIZKIT_ENDPOINT="$endpoint"
BIZKIT_WORKSPACE_ID="$(node -e 'process.stdout.write(require(process.argv[1]).workspaces.main.id)' "$workdir/manifest.json")"
BIZKIT_API_KEY="$(node -e 'process.stdout.write(require(process.argv[1]).api_keys.full.token)' "$workdir/manifest.json")"
TF_ACC=1
TF_ACC_TERRAFORM_PATH="$(mise which tofu)"
TF_ACC_PROVIDER_HOST=registry.opentofu.org
TF_ACC_PROVIDER_NAMESPACE=hay-kot
export BIZKIT_ENDPOINT BIZKIT_WORKSPACE_ID BIZKIT_API_KEY TF_ACC TF_ACC_TERRAFORM_PATH TF_ACC_PROVIDER_HOST TF_ACC_PROVIDER_NAMESPACE

log "running acceptance tests"
cd "$root/terraform-provider-bizkit"
status=0
go test ./... -run '^TestAcc' -count=1 -timeout 20m "$@" || status=$?
if [[ $status -ne 0 && -n "$server_pid" ]]; then
  echo "server log:" >&2
  tail -50 "$workdir/bizkit.log" >&2 || true
fi
exit "$status"
