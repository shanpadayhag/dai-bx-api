#!/usr/bin/env bash
# Fails when a workflow uses a third-party action without a full commit SHA.
set -euo pipefail

unpinned_actions="$(
  grep -rEn '^[[:space:]]*(-[[:space:]]+)?uses:' .github/workflows \
    | grep -Ev 'uses:[[:space:]]*\./' \
    | grep -Ev 'uses:[[:space:]]*[^[:space:]@]+@[0-9a-f]{40}([[:space:]]|$)' \
    || true
)"

if [ -n "$unpinned_actions" ]; then
  echo "These actions are not pinned to a commit SHA:" >&2
  echo "$unpinned_actions" >&2
  exit 1
fi
