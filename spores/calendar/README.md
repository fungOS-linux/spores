# Calendar Spore

Read-only calendar projection over `soil:records` and the
`fungos.calendar/event` schema.

The host owns the records book, iCal bodies, time zone strings and sync. This
Spore parses stored event fields, normalizes dates/times, detects same-day
overlaps and emits one Canvas snapshot. The Lua view owns week, agenda and month
layout, local date navigation, filtering and selection.

```sh
cargo build -p spore-calendar --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/spore_calendar.wasm spores/calendar/module.wasm
cargo run -p calendar-host -- spores/calendar/module.wasm
```

Environment:

- `RECORDS_ROOT` — default `~/.local/share/fungos/records`
- `RECORDS_CALENDAR_BOOK` — default `personal/avery/calendar`
- `CALENDAR_SPORE` — override the component path
- `CALENDAR_QUERY` — substring filter over summary/location/notes/calendar

For a live preview:

```sh
python3 spores/calendar/tools/spore_snapshot.py /tmp/calendar.lua
cd ../fungos-sdk
./bin/tend examples/calendar/calendar.lua /tmp/calendar.lua \
  --app dev.fungos.calendar --surface calendar --name spore-dev-calendar
```

## Live refresh

```sh
RECORDS_CALENDAR_BOOK=personal/avery/calendar \
CALENDAR_TZ=America/Los_Angeles \
spores/calendar/tools/watch_calendar.sh
```

The watcher polls the records book, regenerates the snapshot only when the bytes
change, and hot-reloads the authored Calendar module. This keeps refresh outside
the Spore and off the Canvas frame path.

## Import mail invitations

```sh
spores/calendar/tools/import_mail_ics.py invite.eml attachment.ics
```

A `.eml` is scanned for `text/calendar` parts; each part is imported through
`records import-ics` into the configured calendar book. The Spore then reads the
resulting `fungos.calendar/event` records like any other event.

## Time zones and recurrence

The Spore resolves `TZID` values with the embedded IANA database, converts display
times into the host's local zone (`CALENDAR_TZ` or the system zone), and expands
bounded recurrence windows with `rrule`. The current window is seven days back
and one year forward, capped at 512 events. Raw `RRULE`, organizer, attendees and
notes remain on the record; this projection does not rewrite the source event.
