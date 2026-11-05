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
