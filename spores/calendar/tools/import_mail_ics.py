#!/usr/bin/env python3
"""Import an .ics file or the text/calendar parts of an .eml into records.

This is a host-side bootstrap. Mail clients attach invitations as text/calendar;
the Calendar Spore only ever reads the resulting records.
"""
from __future__ import annotations

import argparse
import email
import os
import subprocess
import tempfile
from pathlib import Path

RECORDS = os.environ.get("RECORDS_BIN", os.path.expanduser("~/.local/bin/records"))
ROOT = os.environ.get("RECORDS_ROOT", os.path.expanduser("~/.local/share/fungos/records"))
BOOK = os.environ.get("RECORDS_CALENDAR_BOOK", "personal/avery/calendar")


def calendars(path: Path) -> list[bytes]:
    data = path.read_bytes()
    if path.suffix.lower() == ".ics":
        return [data]
    message = email.message_from_bytes(data)
    found: list[bytes] = []
    for part in message.walk():
        content_type = part.get_content_type()
        filename = (part.get_filename() or "").lower()
        if content_type == "text/calendar" or filename.endswith(".ics"):
            payload = part.get_payload(decode=True)
            if payload:
                found.append(payload)
    return found


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("paths", nargs="+", type=Path)
    parser.add_argument("--book", default=BOOK)
    args = parser.parse_args()
    imported = 0
    for path in args.paths:
        for index, payload in enumerate(calendars(path)):
            with tempfile.NamedTemporaryFile(suffix=".ics", delete=False) as handle:
                handle.write(payload)
                temp = Path(handle.name)
            try:
                subprocess.run(
                    [RECORDS, "--root", ROOT, "import-ics", "--book", args.book, str(temp)],
                    check=True,
                )
                imported += 1
                print(f"imported {path} part {index + 1} into {args.book}")
            finally:
                temp.unlink(missing_ok=True)
    if imported == 0:
        raise SystemExit("no calendar parts found")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
