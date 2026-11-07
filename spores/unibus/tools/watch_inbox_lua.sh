#!/bin/sh
# Re-compose the Inbox Lua view, reload only that module, and capture it.
set -eu
TOOLS=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
UNIBUS=$(CDPATH= cd -- "$TOOLS/.." && pwd)
REPO=$(CDPATH= cd -- "$UNIBUS/../.." && pwd)
MODULES=${MODULES:-"$HOME/Library/Application Support/Dock/canvas/modules"}
SHOT=${SHOT:-/tmp/inbox-hot.png}
STYLE=${UI_STYLE:-"$HOME/.config/fungos/ui.lua"}
CAPTURE=${CAPTURE:-1}
export MODULES
cd "$UNIBUS"

reload() {
  lua tools/compose_inbox.lua \
    "$MODULES" \
    "$UNIBUS/examples/inbox_live.lua" \
    "$UNIBUS/examples/contacts_live.lua" >/dev/null
  printf 'workspace switch spore-dev\n' | canvasctl >/dev/null
  python3 - <<'PY' | canvasctl >/dev/null
import json
print(json.dumps({
    "schema_version": 1,
    "request_id": "hot-reload-inbox",
    "command": {"command": "reload_module", "surface_id": "md"},
}))
PY
  if [ "$CAPTURE" = 1 ]; then
    SURFACE=md "$TOOLS/capture_contacts_surface.sh" "$SHOT" >/dev/null
    printf 'reloaded inbox · %s at %s\n' "$SHOT" "$(date +%H:%M:%S)"
  else
    printf 'reloaded inbox at %s\n' "$(date +%H:%M:%S)"
  fi
}

reload
echo "watching $REPO/spores/unibus/ui/inbox.lua"
fswatch -o -l 0.05 \
  "$UNIBUS/ui/inbox.lua" \
  "$UNIBUS/ui/lib/ui.lua" \
  "$UNIBUS/examples/inbox_live.lua" \
  "$STYLE" |
  while read -r _; do reload; done
