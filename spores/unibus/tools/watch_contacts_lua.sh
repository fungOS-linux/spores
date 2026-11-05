#!/bin/sh
# Re-compose the Contacts Lua view and ask Canvas to reload the workspace.
#
# This is the fast loop: edit spores/contacts/ui/contacts.lua, save, and the
# running Canvas surface redraws. No Rust, WASM, records query, or workspace
# import is involved.
set -eu
TOOLS=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
UNIBUS=$(CDPATH= cd -- "$TOOLS/.." && pwd)
REPO=$(CDPATH= cd -- "$UNIBUS/../.." && pwd)
MODULES=${MODULES:-"$HOME/Library/Application Support/Dock/canvas/modules"}
WORKSPACE=${WORKSPACE:-contacts-dev}
export MODULES
cd "$UNIBUS"

compose() {
  lua tools/compose_inbox.lua \
    "$MODULES" \
    "$UNIBUS/examples/inbox_live.lua" \
    "$UNIBUS/examples/contacts_live.lua" >/dev/null
  printf 'workspace switch %s\n' "$WORKSPACE" | canvasctl >/dev/null
  printf 'reloaded %s at %s\n' "$WORKSPACE" "$(date +%H:%M:%S)"
}

compose
echo "watching $REPO/spores/contacts/ui/contacts.lua"
fswatch -o -l 0.15 \
  "$REPO/spores/contacts/ui/contacts.lua" \
  "$UNIBUS/ui/lib/ui.lua" \
  "$UNIBUS/examples/contacts_live.lua" |
  while read -r _; do compose; done
