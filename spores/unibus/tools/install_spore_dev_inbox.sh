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
# Contacts takes the second half of the frame rather than replacing the inbox:
# both are the running dev surface, and neither is hidden behind a tab.
document["surfaces"] = [
    surface for surface in document.get("surfaces", [])
    if surface.get("id") in ("md", "contacts")
]
if not any(surface.get("id") == "contacts" for surface in document["surfaces"]):
    document["surfaces"].append({
        "id": "contacts",
        "title": "Contacts",
        "content": {
            "kind": "widget",
            "spec": {
                "schema_version": 2,
                "id": "contacts",
                "kind": "module",
                "module": "spore-dev-contacts",
                "inputs": {},
                "theme": None,
                "refresh_ms": None,
                "chrome": "frameless",
                "data": data,
            },
        },
        "inputs": [],
    })
# Construct the placements rather than filtering existing ones: a document that
# has no placed surface (or a layout of kind "empty") would otherwise produce
# zero placements, which a canvas layout refuses.
document.setdefault("layout", {})["placements"] = [
    {
        "surface_id": "md",
        "bounds": {"x": 0.03, "y": 0.04, "width": 0.58, "height": 0.92},
        "layer": 0,
    },
    {
        "surface_id": "contacts",
        "bounds": {"x": 0.63, "y": 0.04, "width": 0.34, "height": 0.92},
        "layer": 0,
    },
]
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

# The workspace is named for its project: this document's project_id is "dock",
# so saving as "spore-dev" is rejected. The document is already applied live, so
# a save that cannot be named is not worth failing the install over.
printf 'workspace save dock\n' | canvasctl || echo "workspace not saved (name must match project_id)" >&2
echo "installed $MODULES/spore-dev-inbox.{json,lua} and spore-dev-approval2.{json,lua}"
