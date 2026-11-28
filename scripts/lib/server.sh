# Helpers for scripts that run bizkit on a fresh temp data dir and free
# ports. Source it; it expects `workdir` to be set before start_bizkit.

log() { printf '==> %s\n' "$1"; }

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

random_secrets_key() {
  node -e 'process.stdout.write(require("node:crypto").randomBytes(32).toString("base64"))'
}
