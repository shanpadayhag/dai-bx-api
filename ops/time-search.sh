#!/usr/bin/env bash
set -euo pipefail

: "${SEARCH_API_URL:?set SEARCH_API_URL to the API address, for example http://localhost:9000}"
: "${SEARCH_ACCESS_TOKEN:?set SEARCH_ACCESS_TOKEN to the access token the seed was given}"

search_parameters=(
  "q=plan" "q=review" "q=p" "q=re" "q=ch" "q=chinese" "q=vocab" "q=gym" "q=run" "q=meeting"
  "q=plan%20review" "q=review%20plan" "q=write%20report%20draft" "q=chinese%20tones" "q=gym%20morning"
  "q=zzz" "q=qwerty" "q=database%20schema%20migration" "q=birthday%20gift" "q=weekly%20standup"
  "q=plan&kind=tasks&limit=50" "q=re&kind=cards&limit=50" "q=gym&kind=events&limit=50"
  "q=review&kind=groups&limit=50" "q=a&kind=cards&limit=50" "q=b" "q=c" "q=d" "q=te" "q=se"
)

search_curl() {
  if [[ -n "${SEARCH_ORIGIN_SECRET:-}" ]]; then
    curl -H "x-daibx-origin-secret: ${SEARCH_ORIGIN_SECRET}" "$@"
  else
    curl "$@"
  fi
}

request_times_file="$(mktemp)"
trap 'rm -f "$request_times_file"' EXIT

search_curl -s -o /dev/null \
  -H "Cookie: __Host-daibx_access=${SEARCH_ACCESS_TOKEN}" \
  "${SEARCH_API_URL}/search?q=warm"

time_search() {
  search_curl -s -o /dev/null -w '%{http_code} %{time_total}' \
    -H "Cookie: __Host-daibx_access=${SEARCH_ACCESS_TOKEN}" \
    "${SEARCH_API_URL}/search?$1"
}

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
for round_number in 1 2 3; do
  for search_parameter in "${search_parameters[@]}"; do
    response_line="$(time_search "$search_parameter")"
    response_status="${response_line%% *}"
    if [[ "$response_status" != "200" ]]; then
      echo "round ${round_number}: ${search_parameter} answered ${response_status}" >&2
      exit 1
    fi
    echo "${response_line#* }" >> "$request_times_file"
  done
done
finished_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

sort -n "$request_times_file" | awk -v started_at="$started_at" -v finished_at="$finished_at" '
  { request_seconds[NR] = $1 }
  END {
    p50_index = int(NR * 0.50 + 0.5); p95_index = int(NR * 0.95 + 0.5)
    printf "requests: %d (from %s to %s)\n", NR, started_at, finished_at
    printf "round trip p50: %.0f ms, p95: %.0f ms, max: %.0f ms\n",
      request_seconds[p50_index] * 1000, request_seconds[p95_index] * 1000, request_seconds[NR] * 1000
  }'
