#!/usr/bin/env bash
# Writes daibx.dump, rows.tsv and started_at.txt to BACKUP_OUTPUT_DIR.
# The row counts and the dump read one exported snapshot, so they always agree.
set -euo pipefail
umask 077

: "${BACKUP_DATABASE_URL:?BACKUP_DATABASE_URL is required}"
: "${BACKUP_OUTPUT_DIR:?BACKUP_OUTPUT_DIR is required}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/verified-database-url.sh"
verified_backup_database_url="$(verified_database_url "$BACKUP_DATABASE_URL")"
control_dir="$(mktemp -d)"
trap 'rm -rf "$control_dir"' EXIT
mkfifo "$control_dir/commands" "$control_dir/results"

psql --no-psqlrc --quiet --tuples-only --no-align --field-separator=$'\t' \
  --set=ON_ERROR_STOP=1 --dbname="$verified_backup_database_url" \
  <"$control_dir/commands" >"$control_dir/results" &
snapshot_session_pid=$!
exec 3>"$control_dir/commands" 4<"$control_dir/results"

cat >&3 <<'SQL'
BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SELECT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'), pg_export_snapshot();
SQL
IFS=$'\t' read -r -u 4 started_at snapshot_id
printf '%s\n' "$started_at" >"$BACKUP_OUTPUT_DIR/started_at.txt"

printf '\\i %s\n\\echo rows-counted\n' "$script_dir/count-rows.sql" >&3
rows_counted=false
: >"$BACKUP_OUTPUT_DIR/rows.tsv"
while IFS= read -r -u 4 result_line; do
  if [ "$result_line" = "rows-counted" ]; then
    rows_counted=true
    break
  fi
  printf '%s\n' "$result_line" >>"$BACKUP_OUTPUT_DIR/rows.tsv"
done
if [ "$rows_counted" != true ]; then
  echo "The row count did not finish" >&2
  exit 1
fi

pg_dump --dbname="$verified_backup_database_url" --snapshot="$snapshot_id" --schema=public \
  --format=custom --no-owner --no-privileges --file="$BACKUP_OUTPUT_DIR/daibx.dump"

printf 'COMMIT;\n\\q\n' >&3
exec 3>&- 4<&-
wait "$snapshot_session_pid"

table_count="$(wc -l <"$BACKUP_OUTPUT_DIR/rows.tsv" | tr -d ' ')"
dumped_table_count="$(pg_restore --list "$BACKUP_OUTPUT_DIR/daibx.dump" | grep -c ' TABLE DATA public ')"
if [ "$table_count" -eq 0 ] || [ "$table_count" -ne "$dumped_table_count" ]; then
  echo "The dump holds $dumped_table_count tables but the database has $table_count" >&2
  exit 1
fi

echo "Dumped $table_count tables"
