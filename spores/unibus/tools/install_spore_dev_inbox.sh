#!/bin/sh
# Compose the inbox into the current Canvas runtime and point the spore-dev
# workspace surface at it. The markdown composer still owns spore-dev.lua.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
MODULES=${1:-"$HOME/Library/Application Support/Dock/canvas/modules"}
export MODULES

cd "$ROOT"
SNAPSHOT="$ROOT/examples/inbox_live.lua"
if ! python3 tools/agentmail_inbox_snapshot.py "$SNAPSHOT"; then
  echo "live AgentMail snapshot unavailable; keeping the last snapshot" >&2
  if [ ! -f "$SNAPSHOT" ]; then SNAPSHOT="$ROOT/examples/inbox_mock.lua"; fi
fi
lua tools/compose_inbox.lua "$MODULES" "$SNAPSHOT"

python3 - <<'PY'
import copy
import json
import os
import subprocess
import tempfile

modules = os.environ["MODULES"]
request = {
    "schema_version": 1,
    "request_id": "install-spore-dev-inbox",
    "command": {"command": "read_document"},
}
reply = json.loads(subprocess.check_output(["canvasctl"], input=json.dumps(request), text=True))
document = reply["outcome"]["receipt"]["document"]
data = {
    "state": "ready",
    "source": None,
    "observed_at": None,
    "window": None,
    "detail": None,
    "provenance": "live",
}
found = False
for surface in document.get("surfaces", []):
    if surface.get("id") == "md":
        surface["title"] = "Inbox"
        spec = surface["content"]["spec"]
        spec["module"] = "spore-dev-inbox"
        spec["id"] = "inbox"
        spec["data"] = data
        found = True
if not found:
    # No surface to repoint: create one. The spore-dev workspace may not exist
    # yet (for example when the stored document fails to parse and Canvas starts
    # empty), and an installer that only works against a pre-existing surface is
    # not an installer.
    document.setdefault("surfaces", []).append({
        "id": "md",
        "title": "Inbox",
        "content": {
            "kind": "widget",
            "spec": {
                "schema_version": 2,
                "id": "inbox",
                "kind": "module",
                "module": "spore-dev-inbox",
                "inputs": {},
                "theme": None,
                "refresh_ms": None,
                "chrome": "frameless",
                "data": data,
            },
        },
        "inputs": [],
    })

# The inbox is the live surface. The approval module remains installed for the
# next action-routing milestone, but it is not placed over live mail by default.
document["surfaces"] = [surface for surface in document.get("surfaces", []) if surface.get("id") == "md"]
# Construct the placement rather than filtering existing ones: a document that
# has no placed surface (or a layout of kind "empty") would otherwise produce
# zero placements, which a canvas layout refuses.
document.setdefault("layout", {})["placements"] = [{
    "surface_id": "md",
    "bounds": {"x": 0.03, "y": 0.04, "width": 0.94, "height": 0.92},
    "layer": 0,
}]
# A live document can have layout.kind "empty" (for example when the stored
# workspace failed to parse and Canvas started blank). Placements under that kind
# are ignored, so the surface exists but renders nothing.
document["layout"]["kind"] = "canvas"
document["focused_surface_id"] = "md"
document.setdefault("chrome", {})["title"] = "always"

with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as file:
    json.dump(document, file)
    path = file.name
try:
    subprocess.run(["canvasctl"], input=f"doc {path}\n", text=True, check=True)
finally:
    os.unlink(path)
PY

printf 'workspace save spore-dev\n' | canvasctl
echo "installed $MODULES/spore-dev-inbox.{json,lua} and spore-dev-approval2.{json,lua}"
