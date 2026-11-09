# Fleet Lua API

`ui/fleet.lua` exports one entry point:

- `M.render(snapshot) -> nil` — paints the fleet surface through `fungos.ui`.

The view has no WIT imports, no producer, no filesystem, no clock and no
network. The snapshot is produced by `spore-fleet`'s Rust library; the shape
below is the contract between them, and `examples/fleet_snapshot.lua` is a
complete fixture.

## Host inputs

| Input | Used for |
|---|---|
| `module.viewport` or `module.inputs` (`width`, `height`) | the surface size |
| `module.theme` | the type scale and radius tokens |
| `module.inputs.graph_surface = true` | the host can place a `canvas.host_resource`; with it, `graph.resource` becomes a GPU surface instead of marks |

The view calls `ui.viewport(w, h)` once with the resolved size because that is
how this library hands the one-pass layout its surface.

## Snapshot

```lua
{
  schema = 1,
  title = "FLEET",
  subtitle = "mycelium · fungOS",
  status = "live" | "degraded" | "stale" | "unknown",
  captured = "2026-11-09 14:05Z",

  counts = {                      -- exactly the four stat cards, in order
    { label = string, value = number, unit = string?, delta = string?,
      status = "live" | "ok" | "warn" | "crit" },
  },

  graph = {
    status = string,              -- "8 live"
    caption = string?,            -- "23 peers · 9 sites"
    resource = string?,           -- a host resource alias, when granted
    legend = { { ink = ink, label = string } },
    nodes = {
      { id = string, label = string, x = 0..1, y = 0..1,
        ink = ink, state = "live" | "stale" | "unknown",
        label_position = "left" | "right" | "below"?,
        rail = boolean?,          -- a hairline across the plot on this row
        halo = boolean? },        -- the soft disc behind a live dot
    },
    edges = {
      { source = string, target = string, ink = ink,
        state = "live" | "stale" | "unknown" },
    },
  },

  sites = { columns = { { key, label, width, align?, kind? } }, rows = { ... } },
  attention = { columns = ..., rows = ... },

  health = {
    title = string, value = number, max = number,
    status = string, thresholds = { { value, ink } },
  },

  facts = { columns = ..., rows = ... },
  footer = string,
}
```

`x`/`y` are normalized positions, y up. The producer owns them; a node without
a position makes `ui.graph` fall back to a ring, which is a place to start and
not a second layout engine.

## Actions

The view registers host-owned hit regions and never receives a callback:

| Action | Body | Where |
|---|---|---|
| `fleet-inspect-count` | `{ metric }` | a stat card |
| `fleet-inspect-peer` | node `{ id, label, state }` or table row | a graph node or attention row |
| `fleet-filter-site` | the site row | a sites table row |
| `fleet-inspect-release` | — | the release gauge |
| `fleet-inspect-mesh` | the fact row | a mesh row |

An action is a name the host routes to the Spore that owns it; the view has no
idea what opening a peer means.

## What the view does not do

- It does not count live peers. `spore-fleet` does, once, with a stated window.
- It does not reorder sites. The producer's order is stable across polls.
- It does not resolve an ink to a colour. `fungos.ui` and the host theme do.
- It does not size the surface. `module.viewport` does; the fallback is 1200×900.
