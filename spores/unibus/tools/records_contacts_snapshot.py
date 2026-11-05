#!/usr/bin/env python3
"""Emit a contacts snapshot from the local records store for Canvas."""
from __future__ import annotations

import datetime as dt
import json
import os
import subprocess
import sys
from pathlib import Path

RECORDS = os.environ.get("RECORDS_BIN", os.path.expanduser("~/.local/bin/records"))
ROOT = os.environ.get("RECORDS_ROOT", os.path.expanduser("~/.local/share/fungos/records"))
BOOK = os.environ.get("RECORDS_CONTACTS_BOOK", "personal/avery/contacts")


def q(value: object) -> str:
    if value is None:
        return "nil"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, (list, tuple)):
        return "{ " + ", ".join(q(v) for v in value) + " }"
    return '"' + str(value).replace("\\", "\\\\").replace('"', '\\"').replace("\n", " ").replace("\r", " ") + '"'


def obj(items: dict) -> str:
    return "{ " + ", ".join(f"{k} = {q(v)}" for k, v in items.items()) + " }"


def clean(value: object, limit: int = 240) -> str:
    text = "".join(ch for ch in str(value or "") if ch.isprintable() or ch in "\r\n\t")
    return " ".join(text.replace("\r", " ").replace("\n", " ").split()).rstrip("; ")[:limit]


def main() -> int:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/contacts.lua")
    proc = subprocess.run(
        [RECORDS, "--root", ROOT, "list", "--book", BOOK, "--limit", "120", "--json"],
        check=True,
        capture_output=True,
        text=True,
    )
    records = json.loads(proc.stdout)
    contacts = []
    organizations = set()
    for record in records:
        fields = record.get("fields") or {}
        name = clean(fields.get("display_name"))
        if not name:
            given = clean(fields.get("given_name"))
            family = clean(fields.get("family_name"))
            name = " ".join(part for part in (given, family) if part).strip()
        if not name:
            continue
        org = clean(fields.get("organization"))
        if org:
            organizations.add(org)
        contacts.append({
            "id": record.get("entity_id") or name,
            "name": name,
            "org": org,
            "title": clean(fields.get("title")),
            "emails": [clean(item.get("value")) for item in (fields.get("emails") or []) if item.get("value")],
            "phones": [clean(item.get("value")) for item in (fields.get("phones") or []) if item.get("value")],
            "addresses": [clean(item.get("value")) for item in (fields.get("addresses") or []) if item.get("value")],
            "note": clean(fields.get("note"), 500),
            "birthday": clean(fields.get("birthday")),
        })
    contacts.sort(key=lambda contact: (contact["name"].casefold(), contact["org"].casefold()))
    selected = contacts[0]["id"] if contacts else ""
    now = dt.datetime.now().astimezone().strftime("%H:%M")
    lines = [
        "return {",
        f"  revision = {int(dt.datetime.now().timestamp())},",
        '  title = "Contacts",',
        '  search = "",',
        f'  account = {{ label = "Records · contacts", sync = {q(now)} }},',
        f"  total = {len(contacts)},",
        f"  organizations = {len(organizations)},",
        f'  selected_id = {q(selected)},',
        "  filters = {",
        f'    {{ id = "all", label = "All people", count = {len(contacts)}, selected = true }},',
        f'    {{ id = "org", label = "Organizations", count = {len(organizations)} }},',
        "  },",
        "  contacts = {",
    ]
    for contact in contacts:
        lines.append("    " + obj(contact) + ",")
    lines += ["  },", "}", ""]
    out.write_text("\n".join(lines), encoding="utf-8")
    print(f"{out}: {len(contacts)} contacts, {len(organizations)} organizations")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
