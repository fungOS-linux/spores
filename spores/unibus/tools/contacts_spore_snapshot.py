#!/usr/bin/env python3
"""Call the contacts Spore host and emit the Canvas Lua snapshot."""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPO = ROOT.parent.parent
HOST = os.environ.get("CONTACTS_HOST", str(REPO / "target" / "debug" / "contacts-host"))
MODULE = os.environ.get("CONTACTS_SPORE", str(REPO / "spores" / "contacts" / "module.wasm"))
QUERY = os.environ.get("CONTACTS_QUERY", "")


def q(value: object) -> str:
    if value is None:
        return "nil"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, list):
        return "{ " + ", ".join(q(v) for v in value) + " }"
    if isinstance(value, dict):
        return "{ " + ", ".join(f'["{k}"] = {q(v)}' for k, v in value.items()) + " }"
    return '"' + str(value).replace("\\", "\\\\").replace('"', '\\"').replace("\n", " ").replace("\r", " ") + '"'


def main() -> int:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/contacts_spore.lua")
    proc = subprocess.run([HOST, MODULE, QUERY], check=True, capture_output=True, text=True)
    data = json.loads(proc.stdout)
    out.write_text("return " + q(data) + "\n", encoding="utf-8")
    print(f"{out}: {data.get('total', 0)} contacts via {Path(HOST).name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
