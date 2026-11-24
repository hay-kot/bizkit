#!/usr/bin/env bash
# Builds a scratch database from every crate's migrations, then refreshes
# (`prepare`) or checks (`check`) the committed sqlx query data in .sqlx/.
#
# Core migrates first; each migrator ignores versions it doesn't own, the
# same way `Db::migrate` runs them, because all crates share one table.
set -euo pipefail

mode="${1:?usage: scripts/sqlx-db.sh prepare|check}"
root="$(cd "$(dirname "$0")/.." && pwd)"
db="$root/target/sqlx/dev.db"

mkdir -p "$(dirname "$db")"
rm -f "$db" "$db-shm" "$db-wal"
export DATABASE_URL="sqlite:$db"
sqlx database create

dirs=("$root/crates/bizkit-core/migrations")
for dir in "$root"/crates/*/migrations; do
  [[ "$dir" == "$root/crates/bizkit-core/migrations" ]] || dirs+=("$dir")
done
# Per-project file schemas (analytics' migrations-project) go last: the app
# migrations drop the tables they recreate.
for dir in "$root"/crates/*/migrations-*; do
  [[ -d "$dir" ]] && dirs+=("$dir")
done
for dir in "${dirs[@]}"; do
  sqlx migrate run --source "$dir" --ignore-missing
done

# The macros must talk to the scratch database, not the committed data.
export SQLX_OFFLINE=false
cd "$root"
case "$mode" in
  prepare) cargo sqlx prepare --workspace -- --all-targets --all-features ;;
  check) cargo sqlx prepare --workspace --check -- --all-targets --all-features ;;
  *) echo "unknown mode $mode" >&2; exit 2 ;;
esac
