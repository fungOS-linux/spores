#!/bin/sh
# Re-read the records calendar and hot-reload the Canvas module when it changes.
#
# This is the dev loop, not a Spore capability. The Spore never reads files or a
# clock; the host polls the records book and recomposes the authored view.
set -eu
TOOLS=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
SPORE=$(CDPATH= cd -- "$TOOLS/.." && pwd)
REPO=$(CDPATH= cd -- "$SPORE/../.." && pwd)
SDK=${FUNGOS_SDK:-"$REPO/../fungos-sdk"}
if [ ! -x "$SDK/bin/tend" ]; then
  SDK=${FUNGOS_SDK:-"$HOME/src/fungOS/fungos-sdk"}
fi
BOOK=${RECORDS_CALENDAR_BOOK:-personal/avery/calendar}
ZONE=${CALENDAR_TZ:-America/Los_Angeles}
SURFACE=${SURFACE:-calendar}
INTERVAL=${INTERVAL:-5}
SNAPSHOT=${SNAPSHOT:-/tmp/fungos-calendar-spore.lua}
export RECORDS_CALENDAR_BOOK="$BOOK" CALENDAR_TZ="$ZONE"

last=
while :; do
  if python3 "$TOOLS/spore_snapshot.py" "$SNAPSHOT" >/dev/null 2>&1; then
    sum=$(shasum -a 256 "$SNAPSHOT" | awk '{print $1}')
    if [ "$sum" != "$last" ]; then
      (cd "$SDK" && ./bin/tend examples/calendar/calendar.lua "$SNAPSHOT" \
        --app dev.fungos.calendar --surface "$SURFACE" --name spore-dev-calendar >/dev/null)
      last=$sum
      printf 'calendar updated · %s · %s\n' "$BOOK" "$(date +%H:%M:%S)"
    fi
  fi
  sleep "$INTERVAL"
done
