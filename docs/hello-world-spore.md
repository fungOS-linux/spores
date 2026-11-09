# Build a hello-world Spore

This is the guide I wish existed when the first Spore was written. It starts
from a working package, [`spores/sfx`](../spores/sfx), and works outward: what
each file owns, which decisions belong to the Spore and which belong to Soil,
and how a single meaningful outcome travels from an event to a speaker.

The SFX Spore is deliberately a small application with a real edge. It is not a
toy that proves WebAssembly can print: it receives desktop events, resolves a
private mapping, verifies a sample by content id, decodes audio in the guest,
and asks the host to open the room the principal is actually listening in. That
makes it a useful reference for every layer of a Spore.

## What a Spore is

A Spore is four things shipped together:

1. a **manifest** (`spore.toml`) that names the package and requests
   capabilities;
2. a **WebAssembly component** (`module.wasm`) built for `wasm32-wasip2`;
3. a **Lua UI** (`ui/*.lua`) that turns a snapshot into a Canvas scene;
4. the **contract** between them: WIT imports/exports, a JSON snapshot, and
   named actions.

There is no filesystem, network, clock, process, or device access beyond the
manifest grants. The host owns identity, storage, devices, and transport. The
Spore owns its document and its semantics.

The split is not cosmetic:

| Party | Owns | Never owns |
|---|---|---|
| Spore module | document semantics, validation, state, decoding, DSP | layout, colour, host policy, raw device access |
| Spore UI (Lua) | structure, hierarchy, spacing, roles, copy | document truth, capability grants, I/O |
| Canvas | layout, rendering, input normalization, assets | Spore semantics |
| Soil | grants, module lifecycle, identity, storage, rooms, transport | view content |

The longer statement of that contract is in
[`docs/host-contract.md`](host-contract.md).

## The shape of the reference Spore

```text
spores/sfx/
  Cargo.toml            its own Cargo workspace; the artifact is wasm, not a host build
  spore.toml            identity, ABI, launch surfaces, grants, denial reasons
  spore.lock            resolved package lock (generated; commit it)
  module.wasm           built component (generated; shipped in the .spore)
  wit/world.wit         what the guest imports and exports
  wit/deps/*.wit        pinned copies of the host interfaces it was built against
  src/lib.rs            module boundary
  src/guest.rs          soil:appdata, soil:vfs, soil:audio wiring and actions
  src/mapping.rs        the appdata document: schema, defaults, validation
  src/snapshot.rs       the flat JSON view model
  src/decode.rs         MP3/WAV decode, resample, fade, interleave
  src/synth.rs          twelve built-in pads, no assets required
  src/actions.rs        action envelope parsing, host-testable
  ui/board.lua          the soundboard view; self-painting
  ui/API.md             the UI contract: snapshot, actions, ids, authority
  ui/board.example.lua  a snapshot for previews and fixtures
  tests/ui_fixture.lua  renders the view without a host and asserts what it drew
  tests/ui_contract.rs  the Lua artifact gate
  tools/compose_sfx.lua turns the view into one Canvas module
  README.md             the package's own operator notes
```

Two rules are visible in that tree. The guest is split by **responsibility**
(contract, document, view model, media, presentation), not by technical layer.
And every generated artifact has a source next to it.

## 1. Start with one observable outcome

Write the sentence before the code:

> When a desktop event arrives, the selected room plays the sound mapped to that
event; if the room has no listener, the board says so instead of playing
somewhere else.

That sentence determines the hard parts. It needs an event source, a document
(the mapping), a sample store, a decoder, a room-aware sink, and a view. It also
determines what is *not* in scope: no mixer, no streaming export, no network
client, no room discovery inside the guest. A Spore that starts with a protocol
often ends with a capability list instead of an application.

For a first Spore, choose an outcome that is:

- observable without reading logs (a sound, a rendered row, a saved document);
- impossible to fake locally (the host must resolve something the guest cannot);
- small enough that every failure can be named.

## 2. Write the manifest as a permission application

[`spores/sfx/spore.toml`](../spores/sfx/spore.toml) is the model. The important
fields are not the display name; they are the engine, ABI, and grants.

```toml
schema = 1

[spore]
id = "dev.fungos.sfx"
name = "sfx"
version = "0.1.0"
license = "MIT"
summary = "Event-driven soundboard: unibus events to the room you are listening in"

[engine]
soil = ">=0.1"
target = "wasm32-wasip2"
abi = { canvas = "1.0.0", appdata = "1.0.0", vfs = "1.0.0", audio = "0.1.0" }

[module]
path = "module.wasm"

[ui]
entry = "ui/board.lua"
jit = true
hot_reload = true

[launch]
default = "canvas"
surfaces = ["canvas", "native"]
```

The grants are the security review:

```toml
[grants]
"soil:appdata" = "read and write the event->sound mapping"
"soil:vfs" = "read sample objects by workspace path, verified by content id"
"soil:audio" = "open the room the principal is listening in and write PCM"
"soil:canvas" = "present the soundboard surface"

[denied]
"soil:unibus" = ["a window is not a bus subscriber; the host delivers matched events"]
"wasi:audio" = ["all playback goes through soil:audio so the room is resolved once"]
transport = ["socket", "quic", "iroh"]
host = ["process", "secrets", "environment", "raw-filesystem", "clock"]
```

Three habits make this manifest useful rather than decorative:

- **Deny the thing that looks convenient.** The SFX Spore could import
  `soil:unibus` and subscribe itself, but then every soundboard would own a bus
  session and the host could not reason about the window. The host reads the
  rules and delivers an action; the Spore stays a document.
- **Name the layer you actually need.** `wasi:audio` is the portable PCM sink.
  `soil:audio` is the room-aware layer above it. Import the higher layer when
  the feature is routing; import the lower one when the host's default device
  is the whole requirement.
- **Write the denial reason in the manifest.** A future contributor who wants to
  add a socket must first argue with the sentence that says why it is absent.

The ABI table is also the compatibility promise. A host that does not provide
`audio = 0.1.0` should refuse to install the package, not fail on the first
press.

## 3. Write the WIT world before the implementation

The world is the smallest useful description of the application boundary.
[`spores/sfx/wit/world.wit`](../spores/sfx/wit/world.wit):

```wit
package fungos:sfx@0.1.0;

world sfx {
  import soil:appdata/appdata@1.0.0;
  import soil:vfs/vfs@1.0.0;
  import soil:audio/audio@0.1.0;

  /// `play <pad>`, `event <kind>`, `snapshot`, `stop`, `selftest`.
  export run: func(args: list<string>) -> result<_, string>;

  /// The board snapshot as JSON bytes. Canvas passes it to ui/board.lua.
  export snapshot: func() -> result<list<u8>, string>;

  /// Apply one UI action (JSON) and return a fresh snapshot.
  export act: func(action: string) -> result<list<u8>, string>;
}
```

Three exports cover the whole host surface:

- `snapshot` is the read side. It is called when the board opens, after an
  action, and when the host wants fresh room presence.
- `act` is the write side. It takes one JSON intent and returns the new
  snapshot. The host never reaches into the mapping; it only forwards the
  person's intent.
- `run` is the command-shaped entrypoint for hosts and probes that invoke a
  component once per event. The desktop host can use `act` instead; `run`
  keeps the module testable without a Canvas surface.

### Choosing `soil:audio` over `wasi:audio`

This is the decision that most affects the application:

| Need | Import |
|---|---|
| Play into the host's one configured sink | `wasi:audio` |
| Choose a named room (`current`, `home-pi`, `lab`) | `soil:audio` |
| Follow the principal as they move between rooms | `soil:audio` with `current` |
| Route a rule to a specific receiver | `soil:audio` and a per-rule room |

`wasi:audio` is a PCM sink. `soil:audio` is a PCM sink plus a room directory and
policy. A room is a **name**, never an address, port, SSRC, or device path. The
host knows which room the principal is in and whether anything is listening;
the guest asks. See [`soil:audio`](https://github.com/fungOS-linux/soil/blob/main/docs/soil-audio.md)
for the host side.

If a room exists but has no listener, `open` fails with `no-listener`. That is
the point: a sound played into an empty room is the tree falling in the forest.
There is no silent fallback to the laptop speaker.

## 4. Implement the guest as a state machine with named failures

The guest is not a request handler with global mutable state hidden everywhere.
It is one state value, loaded once, mutated by actions, and rendered as a pure
snapshot.

```rust
struct State {
    store: Option<Store>,
    mapping: Mapping,
    status: Status,
    output: Option<Output>,
}
```

`State::load()` does four things, in order:

1. opens appdata, preferring the synced scope and falling back to local;
2. parses `sfx.mapping@1`, keeping the compiled-in defaults on a missing or
   invalid document;
3. refreshes the room directory and the current room;
4. records every failure in `status`, so the board can show it.

This is the pattern worth copying: **a bad document does not panic and does not
silently disappear.** The default mapping stays usable, and the parse error is
visible in the snapshot.

### `snapshot`

`snapshot()` returns JSON bytes. The shape is deliberately flat and
string-keyed, because Canvas and the wiki tooling both need to read it without
a binding generator. See
[`src/snapshot.rs`](../spores/sfx/src/snapshot.rs) and
[`ui/API.md`](../spores/sfx/ui/API.md).

A snapshot is a **view model**, not a serialized internal struct. It carries
what the view needs to make a decision, plus enough status to explain a
failure:

- `pads` (id, label, source, availability, room, duration, sample path/CID);
- `rules` (event kind, pad, enabled, gain, room);
- `rooms` (name, label, local/remote, live, principal, current);
- `status` (opened room, current room, last pad/kind, last error, bind state);
- `issues` (validation errors and warnings).

`opened_room` and `current_room` are separate on purpose. If the host opens
`lab` while the principal is in `home-pi`, the board can show the difference.
The UI never has to guess.

### `act`

The action envelope accepts all three shapes a host might send:

```json
{"action": "sfx.trigger", "pad": "pop"}
{"action": "sfx.trigger", "payload": {"pad": "pop"}}
{"action": {"action": "sfx.trigger", "payload": {"pad": "pop"}}}
```

[`src/actions.rs`](../spores/sfx/src/actions.rs) normalizes them before the
dispatch. This is a boundary concern; it is tested on the host, not only in
wasm.

`act` catches application errors and puts them in `status.last_error`, then
returns a fresh snapshot. A refused trigger is data the board can paint, not a
trap that kills the module. The host still gets a typed `result` for failures
that prevent a snapshot from being produced at all.

### `run`

`run` exists for command-shaped hosts and probes. The SFX implementation
recognizes `play`, `event`, `stop`, and `selftest`. Keep this entrypoint small;
it is a second path into the same state, not a second implementation.

## 5. Make the document strict

[`src/mapping.rs`](../spores/sfx/src/mapping.rs) is the heart of the Spore. It
defines:

```json
{
  "version": 1,
  "gain": 0.85,
  "room": "current",
  "rules": [
    { "kind": "chat.message.v1", "pad": "pop", "enabled": true, "gain": 1.0, "room": null }
  ],
  "samples": [
    {
      "pad": "intro",
      "label": "Intro sting",
      "path": "sfx/intro.mp3",
      "content_id": "blake3:<64 hex>",
      "category": "sample",
      "gain": 1.0
    }
  ]
}
```

Rules that make this document trustworthy:

- **Unknown fields are rejected.** A typo in `enabeld` must fail loudly, not
  silently unmute an alert.
- **Ids and paths are validated.** Event kinds, pad ids, room names, workspace
  paths, and `blake3:` content ids all have explicit predicates.
- **Defaults are a real product surface.** Twelve synth pads and six event
  rules mean a fresh install works with no assets. Built-ins are not a
  fallback for a missing file; they are the guaranteed alert path.
- **Precedence is one rule.** A pad chosen from the board can carry a room; a
  rule can carry a room; otherwise the Spore default applies; otherwise the
  host's `current` room. The exact resolution is
  `room_for(rule) -> rule.room || mapping.room`, and `None` means "let the host
  choose the current room". There is no fourth path.
- **Validation is reported, not hidden.** `validate()` returns issues; sample
  pads that cannot be loaded are `available = false` with a reason. The board
  paints them muted. "I mapped this and nothing happened" is the failure the
  reason exists to prevent.

### Paths are lookup; content ids are truth

A sample has both a workspace path and a `blake3:<hex>` content id. The path is
how the object is found. The content id is what the sound **is**.

Before playback, the guest calls `vfs::stat`, checks that the entry is a file
and within the sample size limit, and compares `entry.content_id` to the id in
the mapping. During a chunked read it checks every chunk's `content_id` again.
If a file is swapped under the same name, the pad is refused with a visible
reason.

This is not paranoia for its own sake. An alert sound tells a person what
happened. A file that can be replaced without changing its name turns the
alert into a lie. The path is convenience; the CID is the promise.

## 6. Decode in the guest; write PCM to the host

`soil:audio` and `wasi:audio` are sinks, not players. They accept interleaved
PCM frames at a declared sample rate and channel count. The guest owns sources,
decoding, resampling, gain, and fades:

```text
sample bytes (soil:vfs)
  -> symphonia decode (MP3/WAV/PCM)
  -> mono f32
  -> resample to 48 kHz
  -> 3 ms edge fade
  -> interleave stereo with gain
  -> soil:audio Output::write
```

[`src/decode.rs`](../spores/sfx/src/decode.rs) implements the middle steps;
[`src/guest.rs`](../spores/sfx/src/guest.rs) handles the sink and backpressure.
The built-in pads are synthesized in [`src/synth.rs`](../spores/sfx/src/synth.rs),
so the alert path has no decoder dependency and no asset dependency.

A write can accept fewer frames than offered. `Output::play` loops, and when a
write accepts zero it calls `drain(50)` before retrying. Do not assume one
write drains a whole sample.

## 7. Write the Lua UI as a pure function

[`ui/board.lua`](../spores/sfx/ui/board.lua) exports one function:

```lua
function M.render(snapshot)
  -- build a fungos.ui tree and call ui.render
end
```

It holds no state, opens no file, resolves no room, and verifies no CID. It
receives the snapshot and returns a scene. Every interaction is an intent:

- `sfx.trigger` - a pad was pressed;
- `sfx.bind` / `sfx.cancel_bind` - arm a rule for rebinding;
- `sfx.test` - play a rule's pad now;
- `sfx.enabled` - mute or unmute a rule;
- `sfx.room` / `sfx.rule_room` - choose the default or per-rule room;
- `sfx.gain` - change the mapping gain;
- `sfx.select` - select a pad for the detail panel;
- `sfx.stop` - close the sink and stop.

The view uses the SDK package, not a private copy:

```lua
local ui = require("fungos.ui")
```

[`spores/sdk/ui`](../sdk/ui) is resolved at composition time. A pad that only
reacts where its text is drawn is not a pad; `ui.tile` registers a host-owned
hit region over the whole tile. The board's stable ids and layout rules are
documented in [`ui/API.md`](../spores/sfx/ui/API.md), because the Lua API is part
of the package ABI.

Three presentation rules are worth carrying to a new Spore:

1. **Paint state, not hope.** Muted, unavailable, and errored pads are visible
   and named. Do not omit a broken mapping; explain it.
2. **Keep ids stable.** `sfx-pad-<id>` and `sfx-rule-<n>` are what fixtures and
   host tests assert. A test that finds text by index is brittle.
3. **Responsive is a layout decision, not a second view.** The side panel drops
   below 760 px; the pad grid flows. The scene is still one render function.

## 8. Test each layer where it can fail

The SFX tests are a template:

| Test | What it protects |
|---|---|
| `src/mapping.rs` unit tests | strict parse, defaults, binding, room precedence, validation |
| `src/actions.rs` unit tests | all three action envelopes, named errors |
| `src/synth.rs` / `src/decode.rs` | pads are audible, decoded samples are finite, resampling is sane |
| `src/snapshot.rs` | view model shape, availability reasons, room reporting |
| `tests/ui_contract.rs` | the Lua UI still composes and renders |
| `tests/ui_fixture.lua` | the real view paints the sections, pads, and live room |
| `cargo clippy --all-targets -- -D warnings` | the host-testable code stays clean |
| `wasm-tools validate` | the shipped component is a valid component |

The fixture is not a screenshot test and does not need Canvas. It stubs
`canvas.text`, `canvas.rect`, and `widget.button`, loads the real SDK and the
real view, and asserts on what was drawn. It catches the class of bug where the
host is fine but the person sees an empty board.

For a new Spore, write the fixture before the polish. If the view cannot render
from
[`ui/board.example.lua`](../spores/sfx/ui/board.example.lua), it will not render
from a live snapshot either.

## 9. Build, package, and run

The full command reference is in
[`docs/build-and-test.md`](build-and-test.md). The shortest path for SFX is:

```sh
cd spores/sfx
cargo test --lib
cargo clippy --all-targets -- -D warnings
cargo build --release --target wasm32-wasip2
cp target/wasm32-wasip2/release/spore_sfx.wasm module.wasm
wasm-tools validate module.wasm

cd ../..
soil-spore lock spores/sfx
soil-spore pack spores/sfx -o dist/sfx-0.1.0.spore
soil-spore verify dist/sfx-0.1.0.spore
```

A `.spore` is a package, not a directory of loose files. `pack` includes the
manifest, module, UI, docs, and lock; `verify` checks the package digest and
layout. Commit `spore.lock`; treat `module.wasm` and `dist/` as build outputs
except when a host needs the prebuilt artifact checked into the repository for
a demo.

## 10. Follow one event all the way to a speaker

This is the whole architecture in one path:

```text
Unibus frame: chat.message.v1
        |
        v
host: owns the bus session, reads rules[].kind from snapshot()
        |  act {"action":"sfx.event","kind":"chat.message.v1"}
        v
SFX guest
  rule lookup -> pad "pop" -> room "current"
        |
        +-- built-in synth: render("pop")
        |
        +-- sample: vfs stat + blake3 check + chunked read + decode
        v
48 kHz stereo interleaved PCM
        |
        v
soil:audio open(Format, room)
  host resolves room -> local device or Isochrone RTP
        |
        v
speaker in the room the principal is actually in
```

The host is not a dumb pipe and the Spore is not a client. The host owns the
things only the host can know: which events exist, who is listening, which
receiver is live, and how to get bytes onto the network. The Spore owns the
things that are judgements about the application: what a sound means, which pad
it uses, how loud it is, and whether the mapping is valid.

That division is why the same module can run on a laptop, a phone, or a
Canvas surface without a per-host renderer, and why a wrong-room sound is
visible instead of mysterious.

## Checklist for a new Spore

- [ ] One sentence describes the observable outcome.
- [ ] `spore.toml` names every grant and every denial reason.
- [ ] The WIT world imports only what the outcome needs.
- [ ] `snapshot()` is a flat view model, not a dump of internal structs.
- [ ] `act()` accepts one intent and returns a fresh snapshot.
- [ ] Application failures become named `status` fields, not traps.
- [ ] The document schema rejects unknown fields and validates ids.
- [ ] Content-addressed assets are verified before and during reads.
- [ ] The Lua view is a pure `render(snapshot)` and emits named intents.
- [ ] `ui/API.md` documents snapshot, actions, and stable ids.
- [ ] A fixture renders the real view without a host.
- [ ] Unit tests, clippy, wasm validation, package verification pass.
- [ ] The package ships the source and the lock, not only the binary.

## Adjacent work, deliberately not in this Spore

These are follow-ups, not missing pieces of the hello-world path:

- a streaming `render` export for guest-side mixing and overlapping voices;
- host-side Isochrone pacing so a whole sample does not arrive as one burst;
- dynamic room discovery through a route control plane;
- a text field in the board for event kinds not yet in the mapping;
- a signed package and a Spore Store install path.

Keeping them out is part of the design. A first Spore should fit in a person's
head; the next Spore should be able to reuse the parts that worked.
