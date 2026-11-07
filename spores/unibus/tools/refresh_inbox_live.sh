#!/bin/sh
# Refresh the live inbox module without rewriting the workspace document.
# Canvas polls the module artifact once a second and hot-swaps a valid scene.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
MODULES=${1:-"$HOME/Library/Application Support/Dock/canvas/modules"}
cd "$ROOT"
python3 tools/agentmail_inbox_snapshot.py examples/inbox_live.lua
lua tools/compose_inbox.lua "$MODULES" "$ROOT/examples/inbox_live.lua"
