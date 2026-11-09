# Rooms Lua API

`ui/rooms.lua` is the flat presenter for a shared room: a pure projection from
one room snapshot to one Canvas scene. The same file renders three
presentations of that snapshot: the full room, a compact room widget, and one
shared screen in fullscreen. A live host supplies the snapshot through Canvas
module text inputs; the reviewed fixture is only the headless fallback. The
same snapshot's `space` id is what the Holodeck presenter opens.

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

- `live` — `true` when a host supplied this snapshot; the fixture leaves it
  absent.
- `revision` — monotonic snapshot revision.
- `view` — `mode` (`room`, `widget`), `focus` (a screen id or `nil`) and
  `selected` (the host's echo of the last scene selection).
- `call` — `state` (`idle`, `starting`, `connected`, `muted`, `failed`),
  `route`, `format`, `frame_ms`, `pid`, `frame_count`, `received_samples`,
  `mic_rms`, `muted`, `error`. A live host owns these; the fixture may use the
  local `module.select` sample instead.
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

## Layout

The composed `fungos.ui` library lays out in **logical pixels** and maps them
onto Canvas's 8×8 scene. Canvas stamps the real surface size as
`module.viewport`; the library reads it through `ui.viewport_size()`. The
fixture supplies the same numbers through `module.inputs` so headless previews
and older hosts keep working. A view that assumes a portrait page would put
controls outside a wide room surface, which Canvas rejects as a whole module.

Controls and host surfaces are asserted to stay inside the scene. Panels with a
fixed column height use that height to pin the composer to the bottom, the way
a chat client keeps its message field at the foot of the window.

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
| `message-room` | — | focus the host's text field; `rooms.myc` sends the typed text back as `send-message` |
| `share-surface` | — | ask the host to share one chosen surface |
| `fullscreen-screen` | `{ id = <screen> }` | put that screen in fullscreen |
| `exit-fullscreen` | — | leave fullscreen |
| `join-call` | — | ask the host bridge to start the local Mac call |
| `leave-call` | — | ask the host bridge to stop it |
| `toggle-mic` | — | ask the host bridge to mute or unmute |
| `request-agent` | — | host-visible marker for an addressed turn |
| `call-request-*` | — | host-visible marker for start/stop/mute/unmute |
| `request-holodeck` | — | host-visible marker for the VR handoff |

`fullscreen-screen` and `exit-fullscreen` are view intents, not media calls. The
host decides whether the screen is still live, updates `view.focus`, and owns the
surface alias. The view never opens, captures, decodes or scales a surface.

With `soil:surface-import` granted, a screen is a host surface. Without it, the
view draws a compact placeholder in the room and a centered stage in fullscreen;
neither branch invents pixels or hides the missing capability behind a fake
preview.

All controls are **local** Canvas buttons: they carry no declared action route
and do not publish to Unibus. A click re-renders the module with
`module.event.target_id`; the view records its choice in `module.select`.

A live click is a **bounded request marker**, not an optimistic final state.
The marker is a disabled control (`call-request-start`, `call-request-stop`,
`call-request-mute`, `call-request-unmute`, `request-holodeck`,
`request-agent`) that `room-host` reads from the retained scene. The host acts,
then publishes a snapshot whose `view.selected` omits the marker. The call
panel therefore shows the real `starting`, `connected`, `muted` or `failed`
state from the child process, not a painted guess.

The fixture has no host, so it keeps the earlier optimistic `module.select`
sample for call state. A live snapshot (`live = true`) ignores that remembered
call choice and uses `snapshot.call`.

## Slots

None. The view composes `sdk/ui` nodes directly and does not expose named
regions to other components.

## Stable ids

The scene promises a host-surface id derived from each screen id
(`share-current`, `quest-camera`) and the fixed `focus-screen` id for the
fullscreen placement. Repeated controls carry explicit ids
(`join-<room>`, `fullscreen-<screen>`); singleton controls use the action name.
The host depends on the fixed marker ids `call-request-*`, `request-holodeck`
and `request-agent`, and on the `join-<room>` label `Here` for room selection.
The persistent controls `join-call`, `toggle-mic` and `leave-call` are panel
affordances; their mere presence is not a click. Room, member and message ids
are data, not scene ids.

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
