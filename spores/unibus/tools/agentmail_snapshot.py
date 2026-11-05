#!/usr/bin/env python3
"""Fetch one AgentMail inbox and emit the Lua snapshot ui/inbox.lua renders.

This is a UI preview adapter, not a Soil host. It never prints the API key.
Usage: agentmail_snapshot.py [OUT.lua]
Key: AGENTMAIL_API_KEY or AGENTMAIL_API_KEY_FILE (default ~/agentmail_api_key_fungOS.txt)
"""
import json
import os
import sys
import urllib.parse
import urllib.request
from pathlib import Path

BASE = "https://api.agentmail.to/v0"


def key() -> str:
    if value := os.environ.get("AGENTMAIL_API_KEY"):
        return value.strip()
    path = Path(
        os.environ.get(
            "AGENTMAIL_API_KEY_FILE",
            "~/agentmail_api_key_fungOS.txt",
        )
    ).expanduser()
    return path.read_text(encoding="utf-8").strip()


def get(path: str, token: str) -> dict:
    request = urllib.request.Request(
        f"{BASE}{path}",
        headers={"Authorization": f"Bearer {token}", "Accept": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def lua(value) -> str:
    if value is None:
        return "nil"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    text = str(value)
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"').replace("\n", " ") + '"'


def field(name: str, value) -> str:
    return f"{name} = {lua(value)}"


def summary(message: dict) -> str:
    text = (message.get("preview") or "").replace("\r", " ").replace("\n", " ").strip()
    return text[:180]


def main() -> int:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/agentmail_snapshot.lua")
    token = key()
    inboxes = get("/inboxes", token).get("inboxes", [])
    if not inboxes:
        print("no AgentMail inbox", file=sys.stderr)
        return 1
    inbox = inboxes[0]
    inbox_id = inbox["inbox_id"]
    quoted = urllib.parse.quote(inbox_id, safe="")
    messages = get(f"/inboxes/{quoted}/messages?limit=50", token).get("messages", [])
    messages.sort(key=lambda message: message.get("timestamp", ""), reverse=True)

    unread = 0
    records = []
    selected = False
    for message in messages:
        labels = message.get("labels") or []
        is_unread = "unread" in labels
        unread += int(is_unread)
        subject = (message.get("subject") or "(no subject)").strip()
        urgent = any(
            word in subject.lower()
            for word in ("failed", "urgent", "deadline", "action required")
        )
        record = {
            "id": f"agentmail/{inbox_id}/{message.get('message_id', '')}",
            "title": subject,
            "summary": summary(message),
            "unread": is_unread,
            "urgent": urgent,
        }
        if is_unread and not selected:
            record["selected"] = True
            selected = True
        records.append(record)

    lines = [
        "return {",
        f"  revision = {len(messages)},",
        '  title = "Inbox",',
        f'  brief = {lua(f"{len(messages)} messages · {unread} unread · {inbox_id}")},',
        "  sections = {",
        "    {",
        '      collection = "email.message",',
        '      label = "Mail",',
        "      records = {",
    ]
    for record in records:
        parts = [
            field("id", record["id"]),
            field("title", record["title"]),
            field("summary", record["summary"]),
            field("unread", record["unread"]),
            field("urgent", record["urgent"]),
        ]
        if record.get("selected"):
            parts.append(field("selected", True))
        lines.append("        { " + ", ".join(parts) + " },")
    lines += ["      },", "    },", "  },", "}", ""]
    out.write_text("\n".join(lines), encoding="utf-8")
    print(f"{out}: {len(messages)} messages, {unread} unread")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
