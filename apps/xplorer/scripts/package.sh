#!/bin/sh
# Build the shipped module and package it, the way `soil-spore` wants it.
#
# A package is the manifest, the module and the lock — `src/`, `wit/` and
# `Cargo.*` are build inputs and never ship (soil docs/spore-package.md). The
# lock covers the bytes that ship, so it is regenerated here rather than by hand:
# a stale lock is a package whose identity no longer describes it.
#
#   ./scripts/package.sh              # module.wasm + spore.lock
#   ./scripts/package.sh --tar        # ... and pack a .spore next to the app
#
# Deliberately no `--features demo`: the harness build renders a fixed listing
# and imports no `soil:vfs`, so it is not this Spore.
set -eu

cd "$(dirname "$0")/.."

SOIL_SPORE="${SOIL_SPORE:-soil-spore}"

cargo build --release --target wasm32-wasip2
cp -f target/wasm32-wasip2/release/xplorer.wasm module.wasm

"$SOIL_SPORE" lock .
"$SOIL_SPORE" check .

if [ "${1:-}" = "--tar" ]; then
    out="${2:-$PWD/xplorer.spore}"
    "$SOIL_SPORE" pack . -o "$out"
    "$SOIL_SPORE" verify "$out"
fi
