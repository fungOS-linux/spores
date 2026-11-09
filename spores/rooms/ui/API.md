# Rooms Lua API

`ui/rooms.lua` is the flat presenter for a shared room: a pure projection from
one room snapshot to one Canvas scene. The same file renders three
presentations of that snapshot: the full room, a compact room widget, and one
shared screen in fullscreen. It is fixture-driven because the room module and
`soil:room` do not exist yet. The same snapshot's `space` id is what the
Holodeck presenter opens.

## Entrypoint

```lua
local view = assert(loadfile("ui/rooms.lua"))()
view.render(view.fixture())
```

`M.render(snapshot)` builds the tree, calls the composed `ui.render(tree, theme)`
so the scene reaches Canvas, and returns the tree for tests. With no argument it
renders `M.fixture()`.

The presentation comes from `snapshot.view`:

- `mode = "room"` (default) with no `focus`: the room list, stats, members, buses,
  screens, conversation and composer.
- `mode = "widget"`: a compact card with stats, now playing, a screen summary and
  open/fullscreen controls.
- `focus = <screen id>` in any mode: that one screen fullscreen with a back
  control. Fullscreen is a snapshot state, not a third mode; the host sets
  `view.focus` in response to `fullscreen-screen` and clears it on
  `exit-fullscreen`.

## Snapshot

One flat, string-keyed table:

- `revision` — monotonic snapshot revision.
- `view` — `mode` (`room`, `widget`) and `focus` (a screen id or `nil`).
- `workspace` — `name`, `presence` (`connected`, `reconnecting`, `offline`).
- `self` — `member`, `name`, `room`, `mic`.
- `rooms[]` — `id`, `name`, `kind` (`voice`, `text`), `members`, `unread`, `vr`.
- `room` — `id`, `name`, `topic`, `joined`, `space`, `buses[]`, `members[]`,
  `screens[]`, `messages[]`. Absent while the user is only browsing; the view
  then renders a lobby. `name` is the display name; `topic` heads the room panel.
- `room.space` — the shared space id; the presenters open the same space.
- `room.buses[]` — `class` (`speech`, `program`), `buffer_ms`, `publishers`,
  optional `lossless`, optional `now_playing` (`title`, `artist`).
- `room.members[]` — `id`, `name`, `kind` (`human`, `agent`, `bot`, `bridge`),
  `presence`, `speaking`, `mic` (`on`, `off`), `agent_state` (`idle`,
  `addressed`), `audio` (`speech`, `program`), `buffered`, `state` (`playing`),
  `presenter` (`canvas`, `holodeck`). The widget counts `presenter = "holodeck"`
  as the VR count.
- `room.screens[]` — `id`, `owner`, `title`, `kind` (`window`, `camera`),
  `surface`; `surface` is a host alias. The room renders one tile per screen and
  a Fullscreen control per tile; `view.focus` matches one screen id.
- `room.messages[]` — `id`, `from`, `text`, `mine`.

Ids are opaque host-issued strings. The view never sees an address, SSRC, SDP,
token or key.

## Actions

Intent strings, with the payload in `body`:

| Action | Body | Meaning |
|---|---|---|
| `join-room` | `{ id = <room> }` | join that room |
| `new-room` | — | create a room |
| `open-room` | `{ id = <room> }` | open the room from the widget |
| `open-rooms` | — | open the rooms workspace from an empty widget |
| `enter-holodeck` | `{ space = <id> }` | open the room in the VR presenter |
| `address-agent` | `{ member = <id> }` | address the agent for the next turn |
| `share-surface` | — | ask the host to share one chosen surface |
| `fullscreen-screen` | `{ id = <screen> }` | put that screen in fullscreen |
| `exit-fullscreen` | — | leave fullscreen |

`fullscreen-screen` and `exit-fullscreen` are view intents, not media calls. The
host decides whether the screen is still live, updates `view.focus`, and owns the
surface alias. The view never opens, captures, decodes or scales a surface.

No action is routed yet: there is no module to receive it. The view never
performs an action itself.

## Slots

None. The view composes `sdk/ui` nodes directly and does not expose named
regions to other components.

## Stable ids

The scene promises a host-surface id derived from each screen id
(`share-current`, `quest-camera`) and the fixed `focus-screen` id for the
fullscreen placement. Repeated controls carry explicit ids
(`join-<room>`, `fullscreen-<screen>`); singleton controls use the action name.
Room, member and message ids are data, not scene ids.

## Components

None yet. The view calls `sdk/ui` directly, like `spores/gallery`. It becomes a
consumer of `fungos/chrome`-style components when the host resolves locked
component sets.

## Theme/config

Uses `module.theme` and falls back to the shared local default when rendered
headlessly. It reads no app-specific configuration and no ambient state.

## Authority

None in Lua. Room membership, presence, audio, screen capture, surface focus and
the Holodeck handoff are host or service concerns. The view names a space, a
screen id and a surface alias and emits an intent; it cannot open a socket, read
a device, resolve a texture or reach an address.
