#!/usr/bin/env bash
#
# Runs the Playwright suite against a fresh server and Mailpit.
#
# The server runs the binary itself, not a container, so the suite covers the
# single-binary layout releases ship. Each run gets its own temp data dir and
# free ports, so several checkouts can run at once.
#
# Usage: scripts/test-e2e.sh [playwright args...]
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
workdir=""
mailpit_pid=""
server_pid=""

log() { printf '==> %s\n' "$1"; }

cleanup() {
  [[ -n "$server_pid" ]] && kill "$server_pid" 2>/dev/null || true
  [[ -n "$mailpit_pid" ]] && kill "$mailpit_pid" 2>/dev/null || true
  wait 2>/dev/null || true
  [[ -n "$workdir" ]] && rm -rf "$workdir"
}
trap cleanup EXIT

free_port() {
  node -e 'const s = require("node:net").createServer(); s.listen(0, "127.0.0.1", () => { process.stdout.write(String(s.address().port)); s.close(); });'
}

# Waits until `url` answers, failing early if `pid` exits first.
wait_for() {
  local name="$1" url="$2" pid="$3"
  for _ in $(seq 1 120); do
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "$name exited before it was ready; see $workdir/$name.log" >&2
      cat "$workdir/$name.log" >&2 || true
      exit 1
    fi
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
  done
  echo "$name did not answer $url in time" >&2
  exit 1
}

log "building the web app"
(cd "$root" && mise run web:build)

log "building bizkit with test hooks and signing"
(cd "$root" && cargo build --locked -p bizkit --features test-hooks,signing)
bin="$root/target/debug/bizkit"

workdir="$(mktemp -d)"
api_port="$(free_port)"
smtp_port="$(free_port)"
mailpit_port="$(free_port)"
base_url="http://127.0.0.1:$api_port"

log "starting mailpit on $mailpit_port (smtp $smtp_port)"
# Chaos lets the isolated project make SMTP reject mail (mailing resend).
mailpit --listen "127.0.0.1:$mailpit_port" --smtp "127.0.0.1:$smtp_port" --database "$workdir/mailpit.db" --enable-chaos \
  >"$workdir/mailpit.log" 2>&1 &
mailpit_pid=$!
wait_for mailpit "http://127.0.0.1:$mailpit_port/api/v1/info" "$mailpit_pid"

# Only *@signup.e2e.test may sign up; invited emails get in through their
# invitations, never through a policy.
cat >"$workdir/bizkit.yaml" <<YAML
log_level: info
server:
  listen: 127.0.0.1:$api_port
  base_url: $base_url
  data_dir: $workdir/data
  # Seals workspace mail settings; the key is fresh for each run.
  secrets_key_env: BIZKIT_SECRETS_KEY
  mail:
    from: "bizkit e2e <noreply@e2e.test>"
    smtp_url: smtp://127.0.0.1:$smtp_port
  auth:
    policies:
      - name: e2e-signup
        matches: ["*@signup.e2e.test"]
YAML

BIZKIT_SECRETS_KEY="$(node -e 'process.stdout.write(require("node:crypto").randomBytes(32).toString("base64"))')"
export BIZKIT_SECRETS_KEY

log "starting bizkit on $api_port"
"$bin" --no-color --config "$workdir/bizkit.yaml" serve >"$workdir/bizkit.log" 2>&1 &
server_pid=$!
wait_for bizkit "$base_url/healthz" "$server_pid"

export BASE_URL="$base_url"
export MAILPIT_URL="http://127.0.0.1:$mailpit_port"
# Workspace mail settings in the suite point their SMTP host here.
export MAILPIT_SMTP_PORT="$smtp_port"
export E2E_SEED_CMD="$bin --config $workdir/bizkit.yaml dev"

log "running playwright"
status=0
pnpm -C "$root/e2e" exec playwright test "$@" || status=$?
if [[ $status -ne 0 ]]; then
  echo "server log: $workdir/bizkit.log (removed on exit; copied to e2e/test-results/bizkit.log)" >&2
  mkdir -p "$root/e2e/test-results" && cp "$workdir/bizkit.log" "$root/e2e/test-results/bizkit.log" || true
fi
exit "$status"
