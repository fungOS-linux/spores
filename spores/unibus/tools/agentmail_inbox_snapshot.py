#!/usr/bin/env python3
"""Emit the current inbox.lua snapshot from one live AgentMail inbox.

Metadata only: subjects, senders, timestamps, labels and previews. Bodies and
attachments are never written to the snapshot. Usage:
  agentmail_inbox_snapshot.py [OUT.lua]
Key: AGENTMAIL_API_KEY or AGENTMAIL_API_KEY_FILE (~/agentmail_api_key_fungOS.txt)
"""
from __future__ import annotations

import datetime as dt
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
        os.environ.get("AGENTMAIL_API_KEY_FILE", "~/agentmail_api_key_fungOS.txt")
    ).expanduser()
    return path.read_text(encoding="utf-8").strip()


def get(path: str, token: str) -> dict:
    request = urllib.request.Request(
        f"{BASE}{path}",
        headers={"Authorization": f"Bearer {token}", "Accept": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def quote(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", " ").replace("\r", " ") + '"'


def lua(value) -> str:
    if value is None:
        return "nil"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, (list, tuple)):
        return "{ " + ", ".join(lua(item) for item in value) + " }"
    return quote(str(value))


def obj(items: dict) -> str:
    return "{ " + ", ".join(f"{k} = {lua(v)}" for k, v in items.items()) + " }"


def sender_name(raw: str) -> str:
    raw = (raw or "").strip()
    if "<" in raw:
        name = raw.split("<", 1)[0].strip().strip('"')
        if name:
            return name
    if "@" in raw:
        return raw.split("@", 1)[0]
    return raw or "Unknown"


def when(raw: str) -> tuple[str, str]:
    try:
        stamp = dt.datetime.fromisoformat(raw.replace("Z", "+00:00")).astimezone()
    except (TypeError, ValueError):
        return "", ""
    today = dt.datetime.now().astimezone().date()
    return (stamp.strftime("%H:%M") if stamp.date() == today else stamp.strftime("%b %d"), stamp.isoformat())


def clean(value: str, limit: int = 180) -> str:
    return " ".join((value or "").replace("\r", " ").replace("\n", " ").split())[:limit]


def labels_for(message: dict, subject: str, sender: str) -> list[str]:
    labels = []
    raw = message.get("labels") or []
    if isinstance(raw, str):
        raw = [raw]
    for label in raw:
        label = str(label).strip().lower()
        if label and label not in {"received", "sent", "unread", "read", "inbox"} and label not in labels:
            labels.append(label)
    if "github.com" in sender.lower() or "github" in subject.lower():
        labels.insert(0, "github")
    if any(word in subject.lower() for word in ("failed", "failure", "ci ", "workflow")):
        labels.insert(0, "ci")
    return list(dict.fromkeys(labels))[:4]


def main() -> int:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/agentmail_inbox.lua")
    token = key()
    inboxes = get("/inboxes", token).get("inboxes", [])
    if not inboxes:
        print("no AgentMail inbox", file=sys.stderr)
        return 1
    inbox = inboxes[0]
    inbox_id = inbox.get("inbox_id") or inbox.get("email") or ""
    quoted = urllib.parse.quote(inbox_id, safe="")
    messages = get(f"/inboxes/{quoted}/messages?limit=50", token).get("messages", [])
    messages.sort(key=lambda item: item.get("timestamp", ""), reverse=True)

    grouped: dict[str, list[dict]] = {}
    for message in messages:
        grouped.setdefault(message.get("thread_id") or message.get("message_id") or "", []).append(message)

    threads = []
    unread = urgent = 0
    for thread_id, thread_messages in grouped.items():
        thread_messages.sort(key=lambda item: item.get("timestamp", ""), reverse=True)
        latest = thread_messages[0]
        subject = clean(latest.get("subject") or "(no subject)", 140)
        sender = latest.get("from") or ""
        # The list clips to 62 chars and the detail pane wraps one brief, so a
        # long preview is bytes the module cannot use. The composed module is
        # capped at 32 KiB by Canvas and the snapshot is inlined into it.
        preview = clean(latest.get("preview") or "", 100)
        is_unread = any("unread" in (item.get("labels") or []) for item in thread_messages)
        is_urgent = any(
            word in subject.lower()
            for word in ("failed", "failure", "urgent", "deadline", "action required", "security")
        )
        unread += int(is_unread)
        urgent += int(is_urgent)
        stamp, iso = when(latest.get("timestamp", ""))
        messages_out = []
        for item in thread_messages[:8]:
            msg_stamp, msg_iso = when(item.get("timestamp", ""))
            # No per-message preview: nothing renders it. With 8 messages per
            # thread at 320 chars this was roughly a third of the snapshot, and
            # the snapshot is inlined into the composed Canvas module, which is
            # capped at 32 KiB. Thread previews (below) are the only ones the
            # view draws.
            messages_out.append(
                {
                    "from": item.get("from") or "",
                    "time": msg_stamp,
                }
            )
        threads.append(
            {
                "id": f"agentmail/{inbox_id}/[REDACTED]",
                "title": subject,
                "sender": sender_name(sender),
                "preview": preview,
                "time": stamp,
                "unread": is_unread,
                "urgent": is_urgent,
                "labels": labels_for(latest, subject, sender),
                "messages": messages_out,
            }
        )


    threads.sort(key=lambda item: item.get("observed_at") or "", reverse=True)
    selected = next((thread for thread in threads if thread["unread"]), threads[0] if threads else None)
    if selected:
        selected["selected"] = True

    github = sum(1 for thread in threads if "github" in thread["labels"])
    support = sum(1 for thread in threads if "support" in thread["labels"])
    tag_counts: dict[str, int] = {}
    for thread in threads:
        for label in thread["labels"]:
            tag_counts[label] = tag_counts.get(label, 0) + 1
    tags = [
        {"id": name, "label": name, "count": count}
        for name, count in sorted(tag_counts.items(), key=lambda item: (-item[1], item[0]))
    ]
    now = dt.datetime.now().astimezone().strftime("%H:%M")

    lines = [
        "return {",
        f"  revision = {int(dt.datetime.now().timestamp())},",
        '  title = "Inbox",',
        '  search = "",',
        '  sort = "Newest first",',
        f'  account = {{ email = {lua(inbox_id)}, provider = "AgentMail", sync = {lua(now)} }},',
        "  accounts = {",
        f'    {{ id = "agentmail", label = "AgentMail", email = {lua(inbox_id)}, provider = "AgentMail", sync = {lua(now)}, unread = {unread}, selected = true }},',
        "  },",
        "  inboxes = {",
        f'    {{ id = "inbox", label = "Inbox", unread = {unread}, selected = true }},',
        '    { id = "archive", label = "Archive", unread = 0 },',
        '    { id = "snoozed", label = "Snoozed", unread = 0 },',
        "  },",
        f"  counts = {{ unread = {unread}, urgent = {urgent}, support = {support}, drafts = 0, archived = 0 }},",
        '  state = { mode = "ready" },',
        "  tags = {",
    ]
    for tag in tags:
        lines.append("    " + obj(tag) + ",")
    lines += [
        "  },",
    ]
    lines += [
        f'  selected_id = {lua(selected["id"] if selected else "")},',
        "  layout = {",
        "    list_offset = 0, list_limit = 50,",
        f'    cursor_id = {lua(selected["id"] if selected else "")},',
        '    focus = "list", width_class = "wide", rail_collapsed = false, detail_collapsed = false,',
        "  },",
        "  filters = {",
        f'    {{ id = "all", label = "All mail", count = {len(threads)} }},',
        f'    {{ id = "unread", label = "Unread", count = {unread}, selected = true }},',
        f'    {{ id = "attention", label = "Needs attention", count = {urgent} }},',
        f'    {{ id = "github", label = "GitHub", count = {github} }},',
        f'    {{ id = "support", label = "Support triage", count = {support} }},',
        "  },",
        "  threads = {",
    ]
    for thread in threads:
        fields = {k: v for k, v in thread.items() if k not in {"messages"}}
        lines.append("    " + obj(fields)[:-2] + ", messages = {")
        for message in thread["messages"]:
            lines.append("      " + obj(message) + ",")
        lines.append("    } },")
    lines += ["  },", "}", ""]
    out.write_text("\n".join(lines), encoding="utf-8")
    print(f"{out}: {len(threads)} threads, {unread} unread")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
