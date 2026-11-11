# Fleet Lua API

`ui/fleet.lua` is one surface: the mycelium wall — masthead, four stat cards, the
site-to-peer topology graph, the attention rail, the site table, the release
drift gauge and the mesh counts.

## Entrypoint

`ui/fleet.lua` exports one function:

- `M.render(snapshot) -> nil` — builds the tree and presents it with `ui.present`,
  so every coordinate comes from the layout in `fungos.ui` and none from here.

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

The view calls `ui.viewport(w, h)` once with its authoring size as the fallback.
The library resolves the host's window (or the fallback) and hands back the size
on the 4px layout grid: a browser window of 937px is not a legal layout height,
and a view that rounded its own numbers would be inventing a second grid.

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

## Slots

None. The wall is a complete view, not a component contributed into another
window: there is no named region a host can replace. A host that wants a piece
of it renders that piece's producer instead.

## Components

`fungos.ui` only: `ui.column`, `ui.row`, `ui.card`, `ui.text`, `ui.stat`,
`ui.table`, `ui.graph`, `ui.gauge` and `ui.present`. The view places nothing by
hand and draws no rectangle of its own, so a theme or spacing change lands here
without an edit.

`ui.graph` draws positions the producer chose; its edges are rotated rectangles,
because that is the line primitive the scene ABI has. When the host declares
`module.inputs.graph_surface`, the same node is emitted as
`canvas.host_resource` and the host paints the GPU surface instead.

## Theme/config

The view names ink roles, never colours: `text`, `muted`, `primary`, `success`,
`warning`, `danger`, `surface`, `surface_sunken` and `background`. Type sizes and
radii come from the host's `module.theme` in pixels (`micro` / `small` / `body` /
`large` / `title`, plus `radius`); `DEFAULT_THEME` is the authoring fallback, not
a second scale.

Every height the view computes is snapped to the 4px layout grid through
`ui.viewport`, because a browser window is whatever size the reader dragged it
to. Spacing is a token (`"sm"`, `"md"`) or a multiple of that base.

## Stable ids

The cards and tables are addressable by id: `fleet-stat-<1..4>` for the stat
cards, `fleet-graph`, `fleet-attention`, `fleet-sites`, `fleet-release` and
`fleet-mesh`. Graph peers are `fleet-graph:<peer-id>`.

Table rows take the library's shape `<table>-row-<index>-<key>`, where the key
is the snapshot's `host`, `site` or `fact`: `fleet-attention-row-3-home-pi`,
`fleet-sites-row-1-auto`, `fleet-mesh-row-2-dhcp-leases`. A host that hit-tests
or addresses a row uses that id, and the index is the row's position in the
snapshot — which is why the producer's order is load-bearing.

## Authority

No ambient authority: the view cannot read a file, call a clock, open a socket,
or load code. It reads only `module.viewport` / `module.inputs`, `module.theme`
and the snapshot it is handed.

The interactions it declares are host-owned hit regions (`widget.button`). A
host that cannot place them still renders every pixel of the view; it just has
nothing to route.

## What the view does not do

- It does not count live peers. `spore-fleet` does, once, with a stated window.
- It does not reorder sites. The producer's order is stable across polls.
- It does not resolve an ink to a colour. `fungos.ui` and the host theme do.
- It does not size the surface. `module.viewport` does; the fallback is 1200×900.
