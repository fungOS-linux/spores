# SFX — Lua API

`ui/board.lua` is the soundboard surface. It is a pure projection: the host
supplies the pad, rule, room, and status snapshot, and this view draws it with
`fungos.ui`. The view never opens an audio device, reads a sample, or resolves a
room.

## Entrypoint

`ui/board.lua` exports `M.render(snapshot)`. The library is `fungos.ui` from the
`fungos-sdk` package, resolved by name at composition time. It is a build-time
import, not a runtime one: `tend` inlines the SDK and rewrites `require` to its
local shim before the view runs.

The same file renders through `tend --scene` (portable `Present::Scene` plus hit
targets), `soil-lua` (Canvas scene ABI), or a host that calls the Spore's wasm
`snapshot` and passes the result to Lua.

## Snapshot

```lua
{
  title = "SFX",
  counts = { pads, rules, enabled, live_rooms, issues },
  mapping = {
    version = 1, scope = "synced" | "local" | nil, generation = integer?,
    gain = number, room = string?, error = string?,
  },
  status = {
    output = "48 kHz stereo",
    opened_room = string?,      -- the room the host actually opened
    current_room = string?,     -- the room the principal is in, if known
    last_pad = string?, last_kind = string?,
    last_error = string?,       -- a refused trigger, named
    synth_error = string?,      -- a malformed sfx/synth.lua, named
    bind = string?,             -- an armed event kind, if any
    selected = string?, audio_error = string?,
  },
  rooms = {
    { name, label, kind = "local" | "remote", live, principal, current },
  },
  pads = {
    { id, label, category, source = "builtin" | "sample", available, reason,
      duration_ms = integer?, selected, room, key,
      path = string?, content_id = string? },   -- sample pads only
  },
  rules = {
    { kind, pad, label, enabled, available, gain, room = string? },
  },
  issues = { { level = "error" | "warning", subject, message } },
}
```

A sample pad with an invalid path, a missing `blake3` id, or a colliding id is
`available = false` and carries a `reason`. The board paints it muted with the
reason in the detail line; it is never silently omitted, because "I mapped this
and nothing happened" is the failure the reason exists to prevent.

## Actions

The view emits hit targets, not callbacks. The host maps a hit id to the Spore's
`act` JSON. Every action below is a flat object accepted by `act`:

- `sfx.trigger` — a pad was pressed. Payload `{ action: "sfx.trigger", pad, room? }`.
  The hit id is `sfx-pad-<id>`. If a rule binding is armed, the host may bind the
  arm instead of playing, then play the pad as confirmation.
- `sfx.bind` — arm a rule's kind for binding. Hit `sfx-rule-<kind>`; payload
  `{ action: "sfx.bind", kind }`.
- `sfx.test` — play the rule's pad now; payload `{ action: "sfx.test", kind }`.
- `sfx.enabled` — mute or unmute a rule; payload
  `{ action: "sfx.enabled", kind, enabled }`.
- `sfx.room` — set the mapping's default room; payload
  `{ action: "sfx.room", room }`.
- `sfx.rule_room` — set or clear one rule's room; payload
  `{ action: "sfx.rule_room", kind, room }`.
- `sfx.gain` — set the mapping gain; payload `{ action: "sfx.gain", gain }`.
- `sfx.stop` — close the sink and stop the current sound; payload
  `{ action: "sfx.stop" }`.

The current board draws only pad and rule hits; the remaining actions are part of
the Spore's `act` contract and are used by the CLI and board-specific host
controls.

## Layout

The board is a four-column, extent-driven surface. It reads `module.viewport` and
falls back to `1200x900`. Rules occupy a compact table above the pads; pad tiles
flow into as many rows as the snapshot needs. Every tile is a `surface:hit`
rectangle over the whole tile, not only its text, and a hit is refused when its
geometry falls outside the granted surface.

## Stable ids

`header`, `title`, `subtitle`, `room`, `output`, `counts`, `rules-label`,
`rules-bg`, `pads-label`, `last-pad`, `last-error`.

Rules are positional: `rule-kind-<n>`, `rule-pad-<n>`, `rule-status-<n>`,
`rule-status-text-<n>`; their hit id is `sfx-rule-<kind>`. Pads are
`pad-<id>`, `pad-mark-<id>`, `pad-name-<id>`, `pad-detail-<id>`,
`pad-room-<id>`, `pad-key-<id>`, `pad-key-text-<id>`; their hit id is
`sfx-pad-<id>`.

## Slots

None. A soundboard has one shape: header, event rules, pads. A host that wants a
second panel composes it around this surface rather than making the view grow an
extension point no host resolves.

## Components

None beyond `fungos.ui`. The pad, rule row, and status line are deliberately
local: they are one screen's vocabulary, not a component library. Reusable
widgets belong in the SDK when a second surface needs the same thing.

## Theme/config

The view names semantic inks only: `background`, `surface`, `text`, `muted`,
`primary`, `success`, `warning`, and `danger`. The host resolves them; a personal
theme moves the board without editing the view. Text sizes are logical pixels and
the host grants the extent through `module.viewport`.

No app-specific config keys are read. The mapping is the Spore's appdata
document and arrives through the snapshot already validated.

## Authority

No network, no filesystem, no clock, no process, no raw device. `spore.toml`
links `soil:appdata`, `soil:vfs`, and `soil:audio` for the Spore's logic; it
denies `soil:unibus` because the host owns the bus. The Lua view receives only
the snapshot and emits hit targets; audio and persistence stay in the wasm guest.
