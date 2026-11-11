# Lights — Lua API

`ui/lights.lua` is a room-first lighting surface. It projects one host-owned
snapshot into a room rail, a room summary, scene chips, and light tiles. It never
probes the bridge, reads Geode, or chooses a credential.

## Entrypoint

- `M.render(view)` — builds a `fungos.ui` tree and presents it.
- `M.name = "dev.fungos.lights"`, `M.version = "0.1.0"`.

## Snapshot

```lua
{
  selected = string,              -- group id to open
  bridge = { name, model, online, lights },
  rooms = {
    { id = string, name = string, on = integer, total = integer,
      bri_pct = integer, lights = { string } },
  },
  scenes = {
    { id = string, name = string, group_id = string, group = string },
  },
  lights = {
    { id = string, name = string, on = boolean, bri = integer,
      temperature_k = integer? },
  },
}
```

Ids and names are bridge-owned. `rooms[].lights` carries membership; a light can
belong to more than one group, so the view never infers membership from a name.
`bri` is Hue's 0..254 scale; `bri_pct` is the display scale 0..100. Counts and
percentages come from the host, not from a second policy in the view.

A room with more than four lights switches to a denser three-column grid; four
or fewer use two columns. That is a layout decision, not a second data model.

## Actions

All actions are intents. The host validates the target and performs the write.

| action | payload | meaning |
|---|---|---|
| `lights.room` | `{ id }` | open a room/group |
| `lights.room_power` | `{ group_id, on }` | turn a group on or off |
| `lights.power` | `{ light_id, on }` | turn one light on or off |
| `lights.brightness` | `{ light_id, level }` | set 25, 50, or 100 percent and turn it on |
| `lights.scene` | `{ scene_id, group_id }` | recall a bridge scene |
| `lights.all_off` | `{}` | turn off every light in the home group |

`lights.brightness` uses a percentage level so the view does not have to know
the bridge's 0..254 range. The host converts it at the Hue boundary.

## Stable ids

`lights-root`, `lights-header`, `lights-body`, `lights-rooms`,
`lights-bridge`, `lights-room-panel`, `lights-scenes`, `lights-list`.
Room rows are `lights-rooms-row-<index>-<id>`. Light cards are
`lights-light-<id>`; their controls are composed by the UI kit.

## Slots

None. This is a complete surface, not a contribution to another region.

## Components

Built from the `fungos.ui` package: `column`, `row`, `card`, `text`, `table`,
`gauge`, `button`, `spacer`, and `action`. Room rows use the table's native
`row_action`/`row_key`; light cards use `ui.button` so every press is a host-owned
hit region rather than a view-local callback.

## Theme/config

Semantic inks only: `background`, `surface`, `surface_raised`, `border`, `text`,
`primary`, `muted`, `accent`, `info`, `success`, `warning`, `danger`. Lit state
uses `warning`; bridge health uses `success`; everything else is neutral. Text
styles and spacing come from `fungos.ui`; the view names no pixel size or colour.

## Authority

No grants. The view has no WIT imports, no filesystem, no socket, no clock, and
no bridge address. The host resolves `geode://home/hue/bridge` and enforces the
origin allowlist. A missing bridge or credential is an empty/offline snapshot,
never a fabricated on-state.

## Host

[`spores/lights-host`](../../lights-host) is the loopback browser host: it polls
the bridge through the existing `hue` Spore, supplies this snapshot, renders the
view, and routes the actions above. The fixture remains the contract a host can
assert against without a bridge.
