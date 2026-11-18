# Sourced by the tools that reach the hosted database outside the app.
# The settings go last so they win over any sslmode already in the address.
verified_database_url() {
  local database_url="$1"
  local certificate_path
  certificate_path="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/supabase-root-ca.pem"
  local query_separator='?'
  case "$database_url" in
    *\?*) query_separator='&' ;;
  esac
  printf '%s%ssslmode=verify-full&sslrootcert=%s' \
    "$database_url" "$query_separator" "$certificate_path"
}
