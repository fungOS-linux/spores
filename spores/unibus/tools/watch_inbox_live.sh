#!/bin/sh
# Keep the live inbox snapshot fresh. Canvas hot-reloads the module artifact.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
INTERVAL=${INBOX_REFRESH_SECONDS:-60}
while :; do
  "$ROOT/tools/refresh_inbox_live.sh" || true
  sleep "$INTERVAL"
done
