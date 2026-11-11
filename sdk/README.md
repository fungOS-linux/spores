# SDK: `fungos.ui`

The UI library Spores draw with: declarative trees, one-pass layout, and every
value taken from the host's theme tokens.

This directory is the package source, not a copy. A Spore consumes it the same way
it consumes any other artifact — composition writes the resolved `ui.lua` into the
module it ships, and `spore.lock` covers the result.

## Why it is a package

Every Spore used to carry its own `ui/lib/ui.lua`. Two agents editing two copies is
how one shell ends up with three definitions of "small gap". This is the versioned
package new views compose; `markdown` and `unibus` still carry older copies as
migration debt, and should not be used as the source of truth for new work.

## What lives here

- `ui.lua` — the library: `column`, `row`, `card`, `stack`, `radial`, `text`,
  `rule`, `code`, `diagram`, `bullets`, `quote`, `bar`, `fill`, `button`, `kbd`,
  `icon`, `icon_button`, plus the dashboard nodes `stat`, `gauge`, `table`,
  `chart`, and `graph`, and `host_surface` for a host-owned GPU/scene layer. The
  layout pass (`lay`) and token helpers stay in the same file.
- `ui.lttb(points, threshold)` and `ui.chart_window(points, count)` — dense-series
  decimation and streaming window helpers. `ui.chart` uses LTTB automatically
  before it emits marks; set `decimate = false` to keep uniform sampling.
- `package.toml` — name, version, entry.

Dashboard nodes are ordinary tree nodes. They consume the same ink, space, radius,
and style tokens as the rest of the library; they do not add a second colour or
layout vocabulary. `ui.graph` takes positions as data and draws edges as rotated
rects, so the same snapshot renders on the current canvas ABI; it also accepts a
host `resource`, which places a GPU surface instead of marks. `stat`, `gauge`, `table` rows, chart legends, and the chart plot
can register host-owned hit regions with `action`, `row_action`, `legend_action`,
and `plot_action`. Actions are optional, so a headless preview or a non-Canvas host
still renders the same tree.

`spores/gallery` is the fixture-driven catalog and screenshot for these nodes,
including the five-panel ChartGPU fixture. `ui.host_surface` is the Lua-side
placement point for a native GPU lease; the host must grant the resource and
provide `canvas.host_resource`.

## Rules the library enforces

1. A view is a pure function of its snapshot; it holds no state.
2. Interaction is an action (`ui.action("save")`), never a callback into the host.
3. Layout is single-pass; nothing measures twice, so nothing can disagree with
   itself.
4. No view names a colour or a pixel: `ink`, `style`, `space`, `radius` come from
   the theme, so retheming moves the whole interface.

## Still to land here

- `tokens.lua`: `elevation`, `stroke`, `glass`, `density`, `motion`, so chrome and
  apps share one visual system.
- `components.lua`: `pill`, `tile`, `list-row`, `section`, `glass-panel`.
- A host-layout adapter. The one-pass Lua layout is intentionally independent of
  Canvas's `canvas.layout` (the taffy/`soil-layout` path) so the same tree can be
  previewed and tested without a compositor. A Spore that wants measured flex
  layout can use `canvas.layout` directly, as `unibus` does.
- Chrome regions migrated onto this library (`control-center` first) so the shell
  and the apps stop looking like two products.
