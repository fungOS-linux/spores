# Rooms

Shared audio/video rooms, rendered for Canvas. Each room also has a Holodeck
twin: the same `space` id, the same members and the same media, with a VR
presenter instead of this flat one. This Spore owns composition, not room
state, media or transport.

## What it renders

- **Room widget** — a compact card with the room name, member/speaking/VR
  counts, bus counts, now playing, a screen summary and open/fullscreen
  controls. It places no participant grid, conversation or host surface.
- **Stats strip** — in-room members, speaking now, speech publishers and buffer,
  program publishers and lossless state. The numbers are computed from the
  member and bus lists, not read from a fixture `stats` table.
- **Room list** — voice and text rooms, live marker, member count, unread count
  and how many members are in the Holodeck twin (`2 VR`).
- **Participant tiles** — humans, agents and bots with distinct state:
  `agent · addressed only` is how an agent says it is not always listening;
  `bot · program · buffered` is a music bot on the high-quality tier.
- **Audio buses** — the two classes a room carries: `speech` (interactive,
  40 ms) and `program` (buffered, 250 ms, lossless on LAN), plus now playing.
- **Holodeck handoff** — `Open in Holodeck` carries the shared `space` id, so
  the VR presenter joins the same room instead of starting another one.
- **Screen grid** — one `ui.host_surface` per `room.screens[]` entry. The
  snapshot carries a host alias (`share:current`), never a URL, texture or
  frame. Each tile has a Fullscreen intent.
- **Fullscreen** — a focused screen fills the view with a back control and the
  room name. Focus is a snapshot state (`view.focus`); the host decides whether
  the screen is still live and what the surface alias resolves to.
- **Conversation and composer** — the room thread plus `Ask JARVIS` and `Share`
  intents.

## What it does not do

There is no Rust module yet, so there is no live data, no grants beyond
`soil:canvas`, and no actions routed back to a Spore. The service skeleton is
[`crates/unibus-room`](https://github.com/fungOS-linux/unibus/tree/main/crates/unibus-room)
([runbook](https://github.com/fungOS-linux/unibus/blob/main/docs/room-service.md));
the `soil:room` host capability and the media path are still design work at
[`soil/docs/rooms.md`](https://github.com/fungOS-linux/soil/blob/main/docs/rooms.md).

This is a fixture view, not yet an installable package: `soil-spore check`
requires a module or a chrome region, and `spores/gallery` has the same gap.
The module lands with the room semantics, not before.

The grants it will need are commented in `spore.toml`: `soil:room` for the
directory, presence and buses; `soil:room.media` for audio through the host;
`soil:surface-import` for the shared screens; `soil:records` for the thread.

A Spore cannot own a WebRTC peer connection, a microphone, an encoder or an
SSRC. Canvas owns pixels, Soil owns grants, and the host owns transport, devices
and media handles. That boundary is why the view is a snapshot and not a call
client. The host room adapter uses `soil:mic`, `soil:camera` and
`soil:screencast` internally; this Spore receives their state and surface
aliases, not capture authority.

## Run

```sh
cd spores/rooms
lua tests/rooms_fixture.lua
```

The fixture stubs `canvas.rect`, `canvas.text`, `canvas.host_resource` and
`widget.button`, enables `ui.debug` so every node must draw inside the box it
claimed, and asserts the room, the widget, fullscreen focus, the empty room, the
lobby, the Holodeck handoff, both audio classes, the music bot and the screens.

## Snapshot

```lua
{
  revision = 42,
  view = { mode = "room", focus = nil },
  workspace = { name = "FPL Rooms", presence = "connected" },
  self = { member = "person:avery", name = "Avery", room = "studio", mic = "on" },
  rooms = { { id = "studio", name = "Studio", kind = "voice", members = 4, vr = 2 } },
  room = {
    id = "studio", name = "Studio", topic = "Transmog + Quest", joined = true,
    space = "studio",
    buses = {
      { class = "speech", buffer_ms = 40, publishers = 3 },
      { class = "program", buffer_ms = 250, publishers = 1, lossless = true,
        now_playing = { title = "Voyager", artist = "Daft Punk" } },
    },
    members = {
      { id = "person:james", name = "James", kind = "human", speaking = true,
        mic = "on", presenter = "holodeck" },
      { id = "agent:jarvis", name = "JARVIS", kind = "agent", agent_state = "addressed" },
      { id = "bot:hifi", name = "Hi-Fi", kind = "bot", audio = "program",
        buffered = true, state = "playing" },
    },
    screens = {
      { id = "share-current", owner = "James", title = "Transmog / fixie",
        kind = "window", surface = "share:current" },
    },
    messages = { { id = "m1", from = "James", text = "Rotation is live.", mine = false } },
  },
}
```

`surface` is a host alias, not a URL. `space` is the shared room identity:
Canvas and Holodeck present the same space. `agent_state = "addressed"` is how
an agent states that it is not always listening. `view.focus` names one screen
id; the host sets it for fullscreen and clears it when the user goes back.

## Boundary

`ui/rooms.lua` is a pure view: no state, no clock, no I/O, no transport. A live
module swaps the snapshot without changing any geometry. Actions are intents;
Canvas owns the hit regions and the Spore decides what each intent means. The
fullscreen action changes `view.focus`, not a media device.
