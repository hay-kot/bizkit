#!/usr/bin/env bash
#
# Regenerates terraform-provider-bizkit/docs with tfplugindocs. The provider
# schema comes from OpenTofu with the provider built locally, so nothing is
# downloaded from a registry or releases.hashicorp.com.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
provider="$root/terraform-provider-bizkit"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

(cd "$provider" && go build -o "$work/plugins/terraform-provider-bizkit" .)
cat >"$work/tofurc" <<HCL
provider_installation {
  dev_overrides {
    "hay-kot/bizkit" = "$work/plugins"
  }
  direct {}
}
HCL
mkdir "$work/config"
cp "$provider/examples/provider/provider.tf" "$work/config/"
TF_CLI_CONFIG_FILE="$work/tofurc" TF_DATA_DIR="$work/data" \
  tofu -chdir="$work/config" providers schema -json >"$work/tofu-schema.json"
# tfplugindocs looks the provider up by its short name, and OpenTofu keys
# it by the full registry.opentofu.org address.
node -e '
  const fs = require("node:fs")
  const schema = JSON.parse(fs.readFileSync(process.argv[1], "utf8"))
  const [address] = Object.keys(schema.provider_schemas).filter((key) => key.endsWith("/bizkit"))
  schema.provider_schemas = { bizkit: schema.provider_schemas[address] }
  fs.writeFileSync(process.argv[2], JSON.stringify(schema))
' "$work/tofu-schema.json" "$work/schema.json"

cd "$provider"
go tool tfplugindocs generate --provider-name bizkit --providers-schema "$work/schema.json"
