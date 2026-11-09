# SFX — Lua API

`ui/board.lua` is the soundboard: pads on the right, the event mapping on the
left, and a side panel that says which room the last sound actually reached. It
is self-painting: it builds a `fungos.ui` tree and calls `ui.render`, so what
runs in Canvas is what `tests/ui_fixture.lua` asserts against.

The view is a pure function of the snapshot. It holds no state, opens no file,
and never resolves a room, a sample, or a CID. Those are the Spore's job; the
view only names intents.

## Entrypoint

`ui/board.lua` exports `M.render(snapshot)`. The library is `fungos.ui` from
[`sdk/ui`](../../../sdk/ui/package.toml), resolved by name at composition time
(see `tools/compose_sfx.lua`). It is not the tool's own UI copy.

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

All actions are intents sent back to the Spore; the view executes nothing.

- `sfx.trigger` — a pad was pressed. Payload `{ pad, room? }`. If a rule binding
  is armed, the press binds the arm instead of playing, then plays the pad as
  confirmation.
- `sfx.bind` — arm a rule's kind for binding. Payload `{ kind }`.
- `sfx.cancel_bind` — leave binding mode.
- `sfx.test` — play the rule's pad now. Payload `{ kind }`.
- `sfx.enabled` — mute or unmute a rule. Payload `{ kind, enabled }`.
- `sfx.room` — set the mapping's default room. Payload `{ room }`.
- `sfx.rule_room` — set or clear one rule's room. Payload `{ kind, room }`.
- `sfx.gain` — set the mapping gain. Payload `{ gain }`.
- `sfx.select` — select a pad for the detail panel. Payload `{ pad }`.
- `sfx.stop` — close the sink and stop the current sound.

## Layout

The board is a four-column tile grid. A tile is `ui.tile`, which registers a
host-owned hit region over the whole tile; a pad that only reacts where its text
is drawn is not a pad. At widths below 760 px the side panel drops and the room
moves to the footer. The pad count is not capped by the layout: sample pads
flow into the next row, and the last row keeps the same tile width.

## Stable ids

`sfx-root`, `sfx-head`, `sfx-title`, `sfx-subtitle`, `sfx-counts`, `sfx-rules`,
`sfx-rules-label`, `sfx-rules-empty`, `sfx-pads-label`, `sfx-side`, `sfx-output`,
`sfx-room-now`, `sfx-last`, `sfx-stop`, `sfx-bind-bar`, `sfx-bind-cancel`.

Rules are positional: `sfx-rule-<n>` with `-bind`, `-pad`, `-test`, `-toggle`.
Pads are `sfx-pad-<id>`; rooms are `sfx-room-<n>` (position in the snapshot).

## Authority

No network, no filesystem, no clock, no process, no raw device. `spore.toml`
links `soil:appdata`, `soil:vfs`, `soil:audio`, and `soil:canvas`; it denies
`soil:unibus` because the host owns the bus. A frame reaches this view only as
an `sfx.event` action the host chose to deliver.
