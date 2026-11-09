#!/bin/sh
# Capture the two mycelium documents the fleet projection reads.
#
#   tools/capture.sh [out-dir]     (default /tmp)
#
# Two reads, not one: `fleet status` is peer identity and health, `topology` is
# what the mesh observed. Merging them in the shell would be a third opinion
# about the schema; `fleet-serve` joins them in Rust where the types live.
set -eu
OUT=${1:-/tmp}
mycelium fleet status --json > "$OUT/fleet-status.json"
mycelium topology --json > "$OUT/fleet-topology.json"
printf 'wrote %s/fleet-status.json and %s/fleet-topology.json\n' "$OUT" "$OUT"
