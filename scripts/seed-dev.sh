#!/usr/bin/env bash
#
# Seeds the svx dev stack: the demo account and an API key, then the demo
# workspace's configuration with OpenTofu (dev/tofu), then sample data with
# `bizkit dev seed-demo`. Safe to run again. svx runs it as the `seed`
# service; run it by hand with `svx exec -- scripts/seed-dev.sh`.
#
# A stack seeded before the OpenTofu step already has the demo rows. On the
# first apply (no state yet) this adopts them with generated import blocks
# instead of making duplicates.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
: "${SVX_STACK_DIR:?run under svx (svx exec -- scripts/seed-dev.sh)}"
: "${SVX_URL_API:?run under svx}"

state_dir="$SVX_STACK_DIR/tofu"
config_dir="$root/dev/tofu"
adopt="$config_dir/adopt_generated.tf"
bin="$root/target/debug/bizkit"
dev=("$bin" --config "$root/dev/bizkit.yaml" dev --data-dir "$SVX_STACK_DIR/data")

log() { printf '==> %s\n' "$1"; }

mkdir -p "$state_dir/plugins"
log "building the provider"
(cd "$root/terraform-provider-bizkit" && mise exec -- go build -o "$state_dir/plugins/terraform-provider-bizkit" .)

"${dev[@]}" demo-key --out "$state_dir/demo.env" --endpoint "$SVX_URL_API"
set -a
# shellcheck source=/dev/null
source "$state_dir/demo.env"
set +a

# The provider comes from the build above, never a registry.
cat >"$state_dir/tofurc" <<HCL
provider_installation {
  dev_overrides {
    "hay-kot/bizkit" = "$state_dir/plugins"
  }
  direct {}
}
HCL
export TF_CLI_CONFIG_FILE="$state_dir/tofurc"
export TF_DATA_DIR="$state_dir/work"
export TF_IN_AUTOMATION=1
tofu() { mise exec -- tofu -chdir="$config_dir" "$@"; }

trap 'rm -f "$adopt"' EXIT
if [[ ! -s "$state_dir/terraform.tfstate" ]]; then
  log "looking for demo rows to adopt"
  node "$root/scripts/tofu-adopt.mjs" >"$adopt"
  [[ -s "$adopt" ]] || rm -f "$adopt"
fi

log "applying dev/tofu"
# dev_overrides needs no `tofu init`. The state lives with the stack, so
# each checkout's stack has its own.
tofu apply -input=false -auto-approve -state="$state_dir/terraform.tfstate"
rm -f "$adopt"

log "seeding sample data"
"${dev[@]}" seed-demo
