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
#
# Which packages to look for comes from the record, not from a glob: each
# recorded row names a file that declares the package it vendors, so every
# package this record covers is enforced for every copy in the tree. A package
# nobody has recorded stays invisible, which is stated plainly in the record
# rather than left to be discovered.
cut -d' ' -f1 "$work/rows" | sort >"$work/listed"

package_of() {
  awk '/^package[[:space:]]+soil:/ { p = $2; sub(/@.*/, "", p); print p; exit }' "$1"
}

: >"$work/packages"
while read -r path _; do
  [ -f "$path" ] && package_of "$path" >>"$work/packages"
done <"$work/rows"
sort -u "$work/packages" >"$work/packages.sorted"

: >"$work/found"
find spores -path '*/wit/deps/*' -name '*.wit' -type f | sort >"$work/all"
while read -r path; do
  pkg=$(package_of "$path")
  [ -n "$pkg" ] || continue
  grep -qxF "$pkg" "$work/packages.sorted" && echo "$path" >>"$work/found"
done <"$work/all"

while read -r path; do
  if ! grep -qxF "$path" "$work/listed"; then
    echo "unrecorded: $path has no row in $record" >&2
    status=1
  fi
done <"$work/found"

exit $status
