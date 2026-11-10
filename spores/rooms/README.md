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
- **Participant rail** — humans, agents and bots with distinct state:
  `AGENT · addressed only` is how an agent says it is not always listening;
  `BOT · playing · buffered` is a music bot on the high-quality tier. The rail
  carries presence, speaking and Holodeck badges; the room's main column stays
  focused on screens and conversation.
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
- **Conversation and composer** — the room thread plus a host-owned text field,
  `Ask JARVIS` and `Share` intents. `rooms.myc` overlays a real input on the
  field; Canvas still presents it as a placeholder until a Canvas text input
  lands.
- **Local call panel** — `THIS MAC` shows the local call state. In Canvas,
  `Join call` asks the host bridge to start the loopback mic + TTS
  tester; in `rooms.myc`, the page starts the browser microphone and WebRTC
  peers. The connected state offers mute/unmute and leave. The view never opens
  a device.

## Live host

The installed module receives a live snapshot through one Canvas stream:
`room-host` publishes `canvas.widget.snapshot.v1` on `soil/rooms`, the
`rooms-v1` binder turns it into declared module text inputs, and this view
renders it. The host owns Unibus membership, local actions, the call child
processes and the optional transcript file. The view still owns no socket,
device, token or peer connection.

The reviewed fixture remains in `ui/rooms.lua` for headless tests and for a
host with no snapshot yet. `tests/live_snapshot.lua` exercises the record stream
the binder sends.

The call panel is host-owned and truthful: `idle`, `starting`, `connected`,
`muted` or `failed`, with the real PID, frame count, microphone RMS and error
text. The default child is `soil-test-call`, a local mic + TTS harness; it is a
device/lifecycle proof, not a room relay. `soil-opus-call` is the separate
direct Opus speech proof.

## Browser presenter (`rooms.myc`)

`rooms-serve` in `unibus` serves this same view at `rooms.myc`. It runs
`ui/rooms.lua` through `tools/render_rooms.lua` for every snapshot, so the web
page and Canvas cannot drift into two layouts. The page is a host, like Canvas:
it draws the scene, owns the microphone, and runs one browser `RTCPeerConnection`
per other tab. The room service receives membership and presence only; the
server relays SDP and ICE and never sees a media frame.

```sh
cargo build -p unibus-room --bin rooms-serve
rooms-serve --config CLIENT.json --room studio --name Avery \
  --spore-root ../spores/spores/rooms \
  --ui ../spores/sdk/ui/ui.lua \
  --bind 127.0.0.1:8793
```

Then add a `rooms.myc` route to the Mycelium gateway pointing at
`http://127.0.0.1:8793`. The gateway supplies TLS; `getUserMedia` requires a
secure context, and `rooms.myc` gets it from the wildcard gateway certificate.

The composer publishes `room.say.v1` through the room service, so the message
is broadcast to every member of the room as `room.message.v1`, not just to the
browser tabs served by this process. The service keeps a bounded, ephemeral
history and includes it in `room.welcome.v1` for late joiners. `Ask JARVIS`
uses the typed text for the addressed turn instead.

Each tab is a distinct LiveKit participant, while this server process is one
Unibus member (the fixed participant identity in `CLIENT.json`). That keeps the
call usable for two tabs without pretending they are two independent room
members; a second person should run their own `rooms-serve` with their own
client identity. `rooms-serve` mints the LiveKit token, so the API secret stays
in the host process. JARVIS is not yet a browser call participant.

The call uses LiveKit for camera, microphone and screen share. Configure it
with `LIVEKIT_URL`, `LIVEKIT_API_KEY` and `LIVEKIT_API_SECRET` when starting
`rooms-serve`; the page loads the pinned `livekit-client` browser bundle from
jsDelivr. If LiveKit is not configured, the room and text composer still work,
but Join call reports that the call service is unavailable.

Screen share is currently a LiveKit track, not `soil:surface-export`; tying it
to the native surface picker is a later adapter.

The module is not yet an installable Spore package: `soil-spore check` requires
a module or a chrome region, and `spores/gallery` has the same gap.

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
lua tools/compose_rooms.lua "$HOME/Library/Application Support/Dock/canvas/modules"
cp tools/rooms.widget.json "$HOME/.canvas/widgets/rooms.json"
```

Then open the `rooms` widget in Canvas and run the reference host from
`unibus`:

```sh
cargo build -p unibus-room --bin room-host --bin room-agent
room-host CLIENT.json --room studio --name Avery --call-bin soil-test-call
```

The host needs the `rooms` surface to be open. It installs the snapshot binding,
polls the retained scene, and publishes a snapshot on membership, presence,
call and selection changes. `Join call` starts the local harness; `Mute` writes
to the call process; `Leave` stops both children. `--transcript PATH` enables
addressed-turn transcript mode; `--agent jarvis` chooses the addressed target.

For a headless addressed participant:

```sh
room-agent JARVIS.json --room studio --name JARVIS \
  --peer 127.0.0.1:48000 --bind 127.0.0.1:48010 \
  --tts-bin soil-opus-call
```

The agent waits for `room.addressed.v1`; it does not listen to the room bus.
`soil-opus-call` proves the direct UDP/Opus speech path between two hosts. Use
ports two apart because ffmpeg allocates RTCP on the next port.

The fixture stubs `canvas.rect`, `canvas.text`, `canvas.host_resource` and
`widget.button`, enables `ui.debug` so every node must draw inside the box it
claimed, and asserts the room, the widget, fullscreen focus, the empty room, the
lobby, the Holodeck handoff, both audio classes, the music bot, the screens, and
the local call states. It also checks that every control and host surface stays
inside the 8×8 scene, which is what Canvas requires to accept the module.

The composed view reads the real surface size from `module.viewport` (through
`ui.viewport_size()`), so a wide room is laid out as a wide room rather than a
portrait page stretched across the wall. The fixture supplies the same numbers
through `module.inputs` for headless runs.

## Snapshot

```lua
{
  live = true,
  revision = 42,
  view = { mode = "room", focus = nil, selected = "room:studio" },
  call = {
    state = "connected", route = "loopback-udp-pcm", format = "48 kHz mono",
    frame_ms = 20, pid = 46107, frame_count = 30, mic_rms = 0.008, muted = false,
  },
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
`call.state` is host-owned when `live = true`; the fixture may still use the
local `module.select` sample. A live click writes a bounded disabled request
marker (`call-request-start`, `call-request-stop`, `call-request-mute`, …), the
host acts on it, and the next snapshot clears it. `view.selected` is the host's
echo of the last scene selection so a live input refresh does not close a
fullscreen screen or move the selected room back. `call.route` is a label, not
a path choice.

## Boundary

`ui/rooms.lua` is a pure view: no state, no clock, no I/O, no transport. A live
module swaps the snapshot without changing any geometry. Local controls are
view events: Canvas re-renders the module with an activation, and the view
records its choice in `module.select`. `room-host` reads the retained scene; it
is the only part that starts a process or touches a device. The fullscreen
action changes `view.focus`, not a media device.

The composed module opts into the `layout` UI-library profile, which removes
the dashboard-node implementations it never uses; the Canvas source cap is
64 KiB and the full kit would leave no room for a live room view.
