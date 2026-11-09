#!/bin/sh
# Pull Marbles' lifecycle events into a worklog — the reconciliation leg.
#
#   tools/pull-marbles.sh <worklog-binary> [log] [snapshot]
#
# With no snapshot this takes one through `mb backup`, which is Marbles' own
# atomic VACUUM INTO: a published command, not a read of the live WAL store. A
# live read-only reader cannot even open a WAL database ("attempt to write a
# readonly database"), so the snapshot is the honest input.
#
# `outbound_event.seq` is the rowid of the `history` row the same transaction
# wrote, so the join recovers the actor and the detail text. That is the
# difference between `closed spores-42` attributed to nobody and
# `avery closed spores-42 — ack: superseded by scan recognition`. Marbles does
# not put either on its webhook envelope; the outbox is where they still are.
set -eu

BIN=${1:?usage: pull-marbles.sh <worklog-binary> [log] [snapshot]}
LOG=${2:-"$HOME/.local/share/fungos/worklog.jsonl"}
SNAPSHOT=${3:-}

cleanup=
if [ -z "$SNAPSHOT" ]; then
  SNAPSHOT=$(mktemp -t marbles-snapshot)
  rm -f "$SNAPSHOT"
  cleanup=$SNAPSHOT
  mb backup "$SNAPSHOT" >/dev/null
fi

trap '[ -n "$cleanup" ] && rm -f "$cleanup"' EXIT

mkdir -p "$(dirname "$LOG")"

# `store_id` is empty in a local database (nothing ever set `store_meta`), so the
# locator falls back to `local` rather than producing `marbles:////project`.
query="
select json_object(
  'schema', 'marbles.event/1',
  'id', coalesce(nullif(o.store_id, ''), 'local') || ':' || o.seq,
  'store_id', coalesce(nullif(o.store_id, ''), 'local'),
  'company', coalesce(nullif(o.store_id, ''), 'local'),
  'project', o.project,
  'issue_id', o.issue_id,
  'event', o.event,
  'occurred_at', o.occurred_at,
  'actor', nullif(trim(coalesce(h.actor, '')), ''),
  'detail', nullif(trim(coalesce(h.detail, '')), '')
) as envelope
from outbound_event o
left join history h on h.rowid = o.seq
order by o.seq;"

sqlite3 -json "$SNAPSHOT" "$query" | jq -r '.[].envelope' | "$BIN" ingest-batch --source marbles --log "$LOG" --allow-unsigned

echo "worklog: $LOG" >&2
