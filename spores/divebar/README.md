# Divebar

The system bar: workspaces on the left, what has focus in the middle, the
ambient facts on the right. It is the `top-bar` chrome region.

Divebar is a **Lua-only Spore**: `ui/divebar.lua` is the program. There is no
`[module]`, because a bar is a projection of state the host already holds —
workspaces and focus from the shell, the clock from `soil:time`, the battery
from `soil:device`. Nothing here needs authority, so nothing here can widen it.

```text
spore.toml     identity, the region it claims, what it denies
spore.lock     generated; pins every byte that ships
ui/divebar.lua the program: one function, 1200x44 authored space
ui/snapshot.lua a fixture frame, so the view renders without a compositor
ui/API.md      the Lua ABI: snapshot fields, stable ids, authority
dist/top-bar.lua  the composed region artifact the compositor loads
```

`[chrome] regions = ["top-bar"]` says which slice of the shell this draws. The
region's Lua is `[ui] entry` — one declaration, not two that can drift.

## Loading it

`chrome-host` (in this repo) is the host side of `[chrome]`: it reads the
manifest, resolves the region's Lua, provides `require` for the SDK and a
grant-checked `__soil`, then renders.

```sh
cargo run -p chrome-host -- spores/divebar --scene /tmp/bar.json
# dev.fungos.divebar 0.1.0 · top-bar · 17 nodes
```

`dist/top-bar.lua` is the artifact the compositor loads: every import inlined,
no snapshot baked, so the host calls `render(snapshot)` with live state. Rebuild
it after changing the view, then re-lock:

```sh
cargo run -p chrome-host -- compose spores/divebar --sdk ../fungos-sdk
soil-spore lock spores/divebar
```

Drop the package fixture to see it with live state — the host supplies the wall
clock when the snapshot has none:

```sh
cargo run -p chrome-host -- spores/divebar --no-snapshot
# 4 nodes: ground, edge, divider, clock at the real time
```

## Work on it

The view is checked and composed with `tend` from
[`fungos-sdk`](https://github.com/fungOS-linux/fungos-sdk):

```sh
tend check ui/divebar.lua ui/snapshot.lua
```

`tend check` composes the module, asserts it stays inside Canvas's node and byte
budgets, and requires `ui/API.md` to document the ABI the view actually uses. A
change that breaks either fails before the compositor sees it.

To look at it, compose a scene and rasterise it, or rebuild the preview artifact
into the host's module directory:

```sh
tend --scene /tmp/divebar.json ui/divebar.lua ui/snapshot.lua --width 1200 --height 44
tend build ui/divebar.lua ui/snapshot.lua --name top-bar
```

Nothing composed is committed here: the lock covers what ships, and what ships is
the Lua above.

## Fallback

A chrome Spore that fails to render must never leave a hole in the desktop. The
host keeps its own bar and only hands the region over when loading and
presenting this one succeeds.

## Known gaps

Two things stand between this package and a drawn bar, and neither is divebar's
to fix:

1. **`tend build` bakes the fixture.** The composed artifact ends with
   `view.render(snapshot)`, using the snapshot it was built with, so it draws one
   frozen frame. A region needs the host to supply state per frame, which means
   `tend build` must emit a module that exposes `render(snapshot)` instead of
   calling it. (It also records the builder's absolute paths in its header, so
   two checkouts compose different bytes.)
2. **The compositor still draws its own bar.** `chrome-host` loads and renders
   this package, but Canvas has built-in shell surfaces and does not consult
   `[chrome]` yet, so nothing hands the region over. The fallback rule is
   therefore untested in the one place it matters: a running desktop.
3. **Rendering a live frame is the compositor's job, not the host's.** This
   package renders on demand; nothing calls it every second yet. The clock on
   the bar is correct at load and does not tick until Canvas takes the region.
4. **Lua has no capability bindings.** The Lua sandbox has `os`, `io`,
   `require` and `package` removed and nothing installed in their place, so the
   Lua entrypoint can draw and cannot act. That is why `[grants]` is empty here:
   a grant a Lua view cannot call would be a lie in the manifest. When a
   generated WIT→Lua binding layer lands, grants become meaningful for Lua-only
   Spores and this file should grow the ones it needs.
