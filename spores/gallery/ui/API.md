# Gallery — Lua API

The ChartGPU visual language as ordinary `fungos.ui` nodes. The native ChartGPU
view draws polylines from resident GPU buffers; Canvas has rect/text nodes, so
this fixture keeps the same five-panel language with decimated column marks.
LTTB runs before the marks are emitted, preserving endpoints and dominant spikes
while keeping the scene small.

## Entrypoint

| file | role |
|---|---|
| `ui/chartgpu.lua` | `M.render(snapshot)` — the dashboard fixture |
| `ui/gallery.lua` | the gallery of scene-level primitives |

`chartgpu.lua` resolves `_G.ui` (composed by `tend`) or falls back to
`require("fungos.ui")` for a direct run.

## Snapshot

```lua
{
  chart = { series = { { points = { { x, y } } } } }?,
  stats = { { label, value, delta? } }?,
  gauge = { value, max, label? }?,
  table = { columns = { string }, rows = { { string } } }?,
  latency = { points = { number } }?,
  throughput = { points = { number } }?,
  resources = { cpu?, memory?, disk? }?,
  connections = number?,
  errors = number?,
}
```

Every panel is optional; a missing panel is omitted rather than drawn empty.

## Actions

None. The gallery is a fixture for scene primitives and has no interaction.

## Slots

None.

## Components

It uses the SDK dashboard primitives (`panel`, `stat`, `gauge`, `table`, `chart`,
`status`) from `fungos.dashboard`; it does not define new ones.

## Theme/config

A `DEFAULT_THEME` table (`body`, `title`, `large`, `micro`, `radius`) is used when
`module.theme` is absent; the host theme wins when present. Panel inks are
semantic names, never colour values.

## Stable ids

Panels are `chartgpu-<panel>`; chart series are `chartgpu-series-<index>`; table
rows are `chartgpu-row-<index>`. The ids survive data changes so a diff only
carries the rows that moved.

## Authority

None. The fixture reads its snapshot and nothing else; it is safe to run under
`tend check` with a fake host.
