#!/bin/sh
# Stage the fungos-vfs document layer so this crate can depend on it by path.
#
# The doc layer (`crdt`, `doc`, `anchor`, `comments`, `projector`, `session`) is
# developed in its own repository on branch `feat/pi-sessions`, not here. A
# committed copy would be a second source of truth that goes stale silently, so
# the copy this crate compiles against is generated and git-ignored. Nothing in
# it is edited from this repository.
#
#   FUNGOS_VFS_CHECKOUT  where the fungos-vfs checkout is
#                        (default: ../../../../fungos-vfs, i.e. the Delta
#                        worktree next to this one)
#
# Re-run it whenever the doc layer moves. The revision it staged is recorded in
# vendor/fungos-vfs/PROVENANCE.
set -eu

here=$(cd "$(dirname "$0")/.." && pwd)
checkout=${FUNGOS_VFS_CHECKOUT:-}

if [ -z "$checkout" ]; then
    for candidate in \
        "$here/../../../../"*/fungos-vfs \
        "$HOME/Documents/src/fungOS-linux/fungos-vfs"
    do
        if [ -f "$candidate/Cargo.toml" ]; then
            checkout=$candidate
            break
        fi
    done
fi

if [ -z "${checkout:-}" ] || [ ! -f "$checkout/Cargo.toml" ]; then
    echo "vendor-doc-layer: set FUNGOS_VFS_CHECKOUT to a fungos-vfs checkout" >&2
    exit 1
fi

dest="$here/vendor/fungos-vfs"
rm -rf "$dest"
mkdir -p "$dest"
cp -R "$checkout/src" "$dest/src"

# The staged manifest loses the workspace it belongs to and gets an empty one of
# its own. The copy is a package, not a workspace root: left as it was, cargo
# would look for members that are not staged (`crates/fungos-vfs-unibus`, …), and
# without any `[workspace]` table it would look *upwards* into this repository's
# workspace, which does not list it either. Its dependencies, features and
# targets are copied unchanged.
{
    awk '
        /^\[workspace\]/ { skipping = 1; next }
        skipping && /^\[/ { skipping = 0 }
        !skipping
    ' "$checkout/Cargo.toml"
    printf '\n# The staged copy is a package of its own; see PROVENANCE.\n[workspace]\n'
} > "$dest/Cargo.toml"
for extra in LICENSE README.md; do
    [ -f "$checkout/$extra" ] && cp "$checkout/$extra" "$dest/"
done

{
    echo "source: $(cd "$checkout" && pwd)"
    echo "branch: $(git -C "$checkout" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
    echo "commit: $(git -C "$checkout" rev-parse HEAD 2>/dev/null || echo unknown)"
    echo "dirty:  $(git -C "$checkout" status --porcelain 2>/dev/null | wc -l | tr -d ' ') changed path(s)"
    echo "staged: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$dest/PROVENANCE"
cat "$dest/PROVENANCE"
