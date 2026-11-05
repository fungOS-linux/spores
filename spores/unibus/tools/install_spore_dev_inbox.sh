#!/bin/sh
# Compose the inbox into the current Canvas runtime and point the spore-dev
# workspace surface at it. The markdown composer still owns spore-dev.lua.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
MODULES=${1:-"$HOME/Library/Application Support/Dock/canvas/modules"}
export MODULES

cd "$ROOT"
lua tools/compose_inbox.lua "$MODULES"

python3 - <<'PY'
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
found = False
for surface in document.get("surfaces", []):
    if surface.get("id") == "md":
        surface["title"] = "Inbox"
        spec = surface["content"]["spec"]
        spec["module"] = "spore-dev-inbox"
        spec["id"] = "inbox"
        spec["data"] = {
            "state": "ready",
            "source": None,
            "observed_at": None,
            "window": None,
            "detail": None,
            "provenance": "live",
        }
        found = True
if not found:
    raise SystemExit("spore-dev workspace has no md surface")

with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as file:
    json.dump(document, file)
    path = file.name
try:
    subprocess.run(["canvasctl"], input=f"doc {path}\n", text=True, check=True)
finally:
    os.unlink(path)
PY

printf 'workspace save spore-dev\n' | canvasctl
echo "installed $MODULES/spore-dev-inbox.{json,lua}"
