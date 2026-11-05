#!/bin/sh
# Re-compose the Contacts Lua view, reload only that module, and capture it.
#
# This is the fast loop: edit spores/contacts/ui/contacts.lua, save, and the
# running Canvas surface redraws. No Rust, WASM, records query, workspace
# switch, or workspace import is involved.
set -eu
TOOLS=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
UNIBUS=$(CDPATH= cd -- "$TOOLS/.." && pwd)
REPO=$(CDPATH= cd -- "$UNIBUS/../.." && pwd)
MODULES=${MODULES:-"$HOME/Library/Application Support/Dock/canvas/modules"}
SHOT=${SHOT:-/tmp/contacts-hot.png}
CAPTURE=${CAPTURE:-1}
export MODULES
cd "$UNIBUS"

reload() {
  lua tools/compose_inbox.lua \
    "$MODULES" \
    "$UNIBUS/examples/inbox_live.lua" \
    "$UNIBUS/examples/contacts_live.lua" >/dev/null

  python3 - <<'PY' | canvasctl >/dev/null
import json
print(json.dumps({
    "schema_version": 1,
    "request_id": "hot-reload",
    "command": {"command": "reload_module", "surface_id": "contacts"},
}))
PY

  if [ "$CAPTURE" = 1 ]; then
    "$TOOLS/capture_contacts_surface.sh" "$SHOT" >/dev/null
    printf 'reloaded contacts · %s at %s\n' "$SHOT" "$(date +%H:%M:%S)"
  else
    printf 'reloaded contacts at %s\n' "$(date +%H:%M:%S)"
  fi
}

reload
echo "watching $REPO/spores/contacts/ui/contacts.lua"
fswatch -o -l 0.05 \
  "$REPO/spores/contacts/ui/contacts.lua" \
  "$UNIBUS/ui/lib/ui.lua" \
  "$UNIBUS/examples/contacts_live.lua" |
  while read -r _; do reload; done
