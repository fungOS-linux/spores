#!/bin/sh
# Capture the native Contacts surface for the Lua hot loop.
set -eu
OUT=${1:-/tmp/contacts-hot.png}
SURFACE=${SURFACE:-contacts}
OUT="$OUT" SURFACE="$SURFACE" python3 - <<'PY'
import json, os, shutil, subprocess, uuid

out = os.environ["OUT"]
surface = os.environ["SURFACE"]
request = {
    "schema_version": 1,
    "request_id": "hot-shot-" + uuid.uuid4().hex[:8],
    "command": {"command": "screenshot", "surface_id": surface},
}
reply = json.loads(subprocess.check_output(["canvasctl"], input=json.dumps(request), text=True))
receipt = reply["outcome"]["receipt"]
path = receipt["path"]
shutil.copyfile(path, out)
os.unlink(path)
print(f"{out} {receipt['width']}x{receipt['height']}")
PY
