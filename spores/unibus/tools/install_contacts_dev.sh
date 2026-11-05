#!/bin/sh
# Install the records contacts view and import a contacts-dev workspace.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
MODULES=${1:-"$HOME/Library/Application Support/Dock/canvas/modules"}
export MODULES
cd "$ROOT"
if [ -x "${CONTACTS_HOST:-../../target/debug/contacts-host}" ]; then
  python3 tools/contacts_spore_snapshot.py "$ROOT/examples/contacts_live.lua"
else
  python3 tools/records_contacts_snapshot.py "$ROOT/examples/contacts_live.lua"
fi
lua tools/compose_inbox.lua "$MODULES" "$ROOT/examples/inbox_live.lua" "$ROOT/examples/contacts_live.lua"

python3 - <<'PY'
import copy, json, os, subprocess, tempfile
request = {"schema_version": 1, "request_id": "install-contacts", "command": {"command": "read_document"}}
reply = json.loads(subprocess.check_output(["canvasctl"], input=json.dumps(request), text=True))
document = copy.deepcopy(reply["outcome"]["receipt"]["document"])
document["id"] = "contacts-dev-workspace"
document["project_id"] = "contacts-dev"
document["revision"] = int(document.get("revision", 0)) + 1
document["surfaces"] = [s for s in document.get("surfaces", []) if s.get("id") == "md"] or document.get("surfaces", [])[:1]
surface = document["surfaces"][0]
surface["id"] = "contacts"
surface["title"] = "Contacts"
surface["content"]["spec"]["id"] = "contacts"
surface["content"]["spec"]["module"] = "spore-dev-contacts"
surface["content"]["spec"]["data"] = {
    "state": "ready", "source": None, "observed_at": None,
    "window": None, "detail": None, "provenance": "live",
}
document["layout"]["placements"] = [{
    "surface_id": "contacts",
    "bounds": {"x": 0.03, "y": 0.035, "width": 0.94, "height": 0.93},
    "layer": 0,
}]
document["focused_surface_id"] = "contacts"
request = {
    "schema_version": 1,
    "request_id": "install-contacts-import",
    "command": {"command": "workspace_import", "document": document, "activate": True},
}
out = subprocess.run(["canvasctl"], input=json.dumps(request), text=True, capture_output=True)
if out.returncode != 0:
    raise SystemExit(out.stderr or out.stdout)
print(out.stdout.strip())
PY

printf 'workspace switch contacts-dev\n' | canvasctl
printf 'workspace save contacts-dev\n' | canvasctl
python3 - <<'PY'
import json, subprocess
request = {
    "schema_version": 1,
    "request_id": "contacts-workspace-name",
    "command": {
        "command": "workspace_sync",
        "projects": [{
            "project_id": "contacts-dev",
            "name": "Contacts",
            "status": "active",
            "theme": {"source": "coherence", "mode": "dark", "accent": "success"},
        }],
    },
}
subprocess.run(["canvasctl"], input=json.dumps(request), text=True, check=True, capture_output=True)
PY
echo "installed contacts-dev"
