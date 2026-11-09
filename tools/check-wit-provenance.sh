#!/bin/sh
# Fail when a vendored copy of soil's WIT stops matching the blob it was taken
# from, or when a copy appears that the record does not name.
#
# The record (`wit-provenance.txt`) carries the blob, the repository carries the
# copy, so the check needs no network and no soil checkout. What it catches is
# the drift that actually happens here: someone edits a copy to test something
# and commits it, or bumps the copy without bumping the record.
set -eu

cd "$(dirname "$0")/.."
record=wit-provenance.txt

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Rows are "<path> <blob>"; comments and blank lines are not rows.
awk '!/^[[:space:]]*#/ && NF == 2 { print $1, $2 }' "$record" >"$work/rows"

status=0
while read -r path blob; do
  if [ ! -f "$path" ]; then
    echo "missing: $path is in $record but not in the tree" >&2
    status=1
    continue
  fi
  actual=$(git hash-object "$path")
  if [ "$actual" != "$blob" ]; then
    echo "drift: $path is $actual, $record records $blob" >&2
    status=1
  fi
done <"$work/rows"

# A copy nobody recorded is the other half of the same failure: it is exactly
# how the copies this record replaced went stale unnoticed.
cut -d' ' -f1 "$work/rows" | sort >"$work/listed"
find spores -path '*/wit/deps/*' -name '*soil-records*.wit' -type f | sort >"$work/found"
while read -r path; do
  if ! grep -qxF "$path" "$work/listed"; then
    echo "unrecorded: $path has no row in $record" >&2
    status=1
  fi
done <"$work/found"

exit $status
