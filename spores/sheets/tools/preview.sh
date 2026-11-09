#!/usr/bin/env bash
# Render the Spore's Lua view for every example snapshot, in both themes.
#   bash spores/sheets/tools/preview.sh [snapshot-dir] [out-dir]
set -euo pipefail
cd "$(dirname "$0")/../../.."
snapshots="${1:-spores/sheets/examples}"
outdir="${2:-spores/sheets/preview}"
mkdir -p "$outdir"
tree="$(mktemp -t sheets-tree).json"
trap 'rm -f "$tree"' EXIT

render() { # snapshot, name
  lua spores/sheets/tools/ui-dump.lua "$1" > "$tree"
  python3 spores/sheets/tools/preview.py "$tree" "$outdir/$2.png" light
  python3 spores/sheets/tools/preview.py "$tree" "$outdir/$2-dark.png" dark
}

render "$snapshots/fpl-snapshot.lua" sheet
render "$snapshots/pricing-strategy-snapshot.lua" pricing-strategy
