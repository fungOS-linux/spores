# SFX

A soundboard for the events a desktop already produces. Unibus kinds arrive as
`sfx.event` intents, the mapping decides which pad they mean, and the PCM goes
out through **`soil:audio`** to the room the principal is actually in — home-pi,
the lab stereo, or the IEMs on the S22.

```text
unibus frame (chat.message.v1)
        │
        ▼
   host: owns the bus, reads `rules[].kind` from the snapshot
        │  act {"action":"sfx.event","kind":"chat.message.v1"}
        ▼
   sfx spore ── mapping (soil:appdata) ──► pad
        │                                  │
        │ built-in synth                   │ sample: soil:vfs (path + blake3 cid)
        │                                  │         decode in-guest (MP3/WAV)
        ▼                                  ▼
                 soil:audio (room = current | home-pi | lab)
                                │
                                ▼
                    local device  or  Isochrone RTP
```

## Why the mapping is appdata

The mapping is a **document**, not host configuration: which event sounds like
what is a judgement about someone's attention, and it belongs to the app that
makes it. It is stored under one key, `sfx.mapping@1`, in the Spore's appdata —
synced when the principal has an identity, local otherwise. The host never reads
the mapping; it only needs the kinds, which the snapshot exposes.

## Why the sample is a path **and** a CID

`soil:vfs` gives a workspace object a `blake3:<hex>` content id. A sample pad
stores both the path (lookup) and the content id (identity). Before a sample
plays, the Spore checks the id the host served against the id in the mapping,
including on every chunk of a long read. If they differ, the pad is refused and
the reason is visible in the board.

This is the point of a sound: it tells you what happened. A file that can be
swapped under the same name turns an alert into a lie. The path is convenient;
the CID is the promise.

MP3 and WAV are decoded in the guest (`symphonia`), because `wasi:audio` and
`soil:audio` are PCM sinks — the guest owns sources, decoding and DSP.

## The room, not the device

`soil:audio` opens a **room**, resolved by the host. A room is a name such as
`home-pi` or `lab`; it is never an address, port, SSRC, or device path. The
host knows which room the principal is in and whether a receiver is listening.
`current` follows the principal; a named room is an override.

If a room exists but nothing is listening, `open` fails with `no-listener` and
the board says so. That is deliberate: playing into an empty room is the tree
falling in the forest. The host is the only place that can know, so the Spore
asks rather than guesses.

`wasi:audio` remains the portable PCM sink underneath; a Spore that only needs
the host's default device can keep importing it. This Spore imports
`soil:audio` because room selection is the feature.

## Defaults

A fresh install has twelve synthesized pads and six rules:

```text
email.delta.v1     → chime      chat.message.v1   → pop
notify.publish.v1  → ping       call.incoming.v1  → alarm
ci.failed.v1       → thud       timer.elapsed.v1  → rise
```

Built-ins are not a fallback for a missing asset; they are the guarantee that
the alert path works on an empty workspace. Sample pads are opt-in. They are
appended to the board in mapping order and get the next key in the row.

### Editing from the CLI

The mapping is appdata, and the `sfx` command edits the same document the board
uses:

```sh
sfx list                         # pads, rules, effective rooms
sfx mapping                      # the stored JSON
sfx mapping > /tmp/sfx.json
sfx load < /tmp/sfx.json          # replace the document (strict parse)
sfx render intro > intro.wav      # pre-render one pad, no audio device
sfx bind ci.failed.v1 thud       # add or replace a rule
sfx unbind build.done.v1         # remove a rule
sfx rule-room build.done.v1 home-pi
sfx rule-gain build.done.v1 0.8
sfx enable ci.failed.v1
sfx room current                 # default room for every unmatched rule
sfx gain 0.85
sfx sample add intro sfx/intro.mp3 --label 'Intro sting'
sfx sample remove intro
sfx validate                     # parse, ids, paths, CIDs, room names
sfx reset                        # restore the compiled-in defaults
```

`sample add` reads the object from `soil:vfs`, records its `blake3:` content id,
and refuses a file that is not a file or is over the sample limit. The mapping
is saved under `sfx.mapping@1`; there is no second config file.

Mapping JSON (`sfx.mapping@1`):

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

Unknown fields are rejected rather than dropped: a typo that silently removes
the rule for a failed build is worse than a mapping that refuses to load.

## Host contract

- Read `snapshot()`. It is JSON; `pads`, `rules`, `rooms`, and `status` are the
  board. Each rule's `kind` is exactly the Unibus kind to observe.
- Own the `soil:unibus` subscription. When a frame matches, call
  `act({"action":"sfx.event","kind":"…"})`. The Spore checks the rule, the
  room, and the pad; the host never needs to understand any of them.
- Present the Lua view. Pads emit `sfx.trigger`, rules emit `sfx.bind`,
  `sfx.test`, `sfx.enabled`; the side panel emits `sfx.room` and `sfx.stop`.
- Link `soil:audio` with a room table. The Spore opens with `current` by
  default; a rule can override it.

A host that only has `wasi:audio` can still run the module with the host's
single configured sink, but it cannot honour a room override. That is the one
capability gap; `docs/soil-audio.md` in Soil records the contract.

## Pad definitions live in Lua

The twelve built-ins are defined in [`ui/synth.lua`](ui/synth.lua). The guest
reads the same document from `sfx/synth.lua` in `soil:vfs` at load time, so a
sound designer can change a frequency, envelope, or duration by writing a VFS
object and pressing again. No Rust rebuild is needed. The file is a data-only
Lua subset (`return { ... }` with tables, strings, numbers and booleans); the
guest does not execute Lua code. If the VFS copy is missing or malformed, the
embedded copy remains in force and `status.synth_error` says why.

```sh
# after writing a new sfx/synth.lua through the VFS
sfx list                 # durations update immediately
sfx play blip
```

## A minimal pad surface

[`ui/pads.lua`](ui/pads.lua) is a smaller view: a four-column grid of pad
tiles, the room line, and the last result. It emits the same `sfx.trigger`
actions as the full board. To use it as the package entry, change one line in
`spore.toml`:

```toml
[ui]
entry = "ui/pads.lua"
```

[`ui/board.lua`](ui/board.lua) remains the full soundboard with the event
mapping and binding workflow. Both are pure `render(snapshot)` functions.

## Command-line face

The same Spore can be installed as a Mush command. It imports the same three
capabilities and loads the same mapping and pad definitions:

```sh
cd spores/sfx
cargo build --release --target wasm32-wasip2 \
  --no-default-features --features cli --bin sfx
mkdir -p ~/.local/share/fungos/commands/sfx
cp target/wasm32-wasip2/release/sfx.wasm ~/.local/share/fungos/commands/sfx/module.wasm
cp cli/command.toml ~/.local/share/fungos/commands/sfx/command.toml
```

Then, from Mush:

```text
sfx list
sfx rooms
sfx play blip
sfx event chat.message.v1
sfx bind ci.failed.v1 thud
sfx room home-pi
```

A CLI process has no long-lived sink, so `stop` is not a command. The host
resolves `current` and named rooms exactly as it does for the board.

## Bundled sample pads

`assets/samples/` contains eight short interface sounds from Kenney's
[Interface Sounds](https://kenney.nl/assets/interface-sounds) pack, converted to
48 kHz stereo MP3. The pack is CC0; provenance and the original filenames are
in [`assets/samples/README.md`](assets/samples/README.md).

Install them into a personal VFS and mapping with:

```sh
SOIL_COMMAND=/tmp/sfx-target/debug/soil-command tools/install-samples.sh
```

The script writes each object through `fungos-vfs` and adds a sample pad with
`sfx sample add`, so the mapping records the path and the `blake3:` content id.
It is idempotent: an existing pad is left alone.

## Personal browser sheet

[`tools/sfx-web.py`](../../tools/sfx-web.py) is a small loopback web surface for
the personal instance. It reads the same appdata mapping and calls the same
`sfx` command for playback; it is a browser face, not a second sound engine.

```sh
python3 tools/sfx-web.py \
  --soil-command /tmp/sfx-target/debug/soil-command
```

Then route `sfx.myc` to `http://127.0.0.1:8795` in the Mycelium gateway and
open <https://sfx.myc>. The page lists the pads from appdata and shows the
default room and gain. It pre-renders each pad once through `sfx render`, then
plays overlapping pads with Web Audio in the browser. There is no process spawn
or CoreAudio stream open per press. The older `POST /api/play` endpoint remains
for a host-side, room-routed fallback.

The server binds loopback and has no filesystem grant of its own; the command
host is the authority boundary. Keep it loopback-only unless the gateway has an
auth policy you trust.

## Build

The Spore is its own workspace (like `notify`), because the decoder is a real
dependency and the wasm target is the artifact, not a host build.

```sh
# On the build machine (agora-one):
rustup target add wasm32-wasip2
cargo build --release --target wasm32-wasip2
cp target/wasm32-wasip2/release/spore_sfx.wasm module.wasm

# Locally, with the Soil packer:
soil-spore lock spores/sfx
soil-spore pack spores/sfx -o dist/sfx-0.1.0.spore
soil-spore verify dist/sfx-0.1.0.spore
```

Preview the board without a host:

```sh
cd spores/sfx
lua tests/ui_fixture.lua
lua tools/compose_sfx.lua /tmp/sfx-modules
```

Hear the built-ins without a desktop:

```sh
cargo run --example render_demo -- sfx-demo.wav
```

The packaging step produces `dist/sfx-0.1.0.spore` and
`dist/sfx-demo.wav`; the WAV is every built-in pad in order with a quarter
second of silence between them.

## Tests

```sh
cd spores/sfx
cargo test --lib                          # mapping, synth, decode, snapshot
cargo test --test ui_contract             # the Lua artifact gate (needs Lua)
```

## Known gaps

- **One voice per trigger.** `act` writes one sample and lets the host's sink
  queue it; overlaps are the host's mixer, and today the cpal host opens one
  ring per `soil:audio` output. A streaming `render` export would let the guest
  mix, but it also needs a host that drives it, and this Spore does not pretend
  to have one.
- **Isochrone pacing is the host's job.** `soil:audio`'s Isochrone sink accepts
  PCM as fast as it is written; a guest that writes a whole 720 ms chime in one
  call sends it in one burst. The host should pace or the receiver should
  rebuffer. `docs/wasi-audio.md` records the same limit.
- **Presence is host config in v1.** `current` resolves from the host's room
  table; dynamic discovery (and isochrone's route control plane) is a host
  adapter, not a Spore concern.
- **New event kinds need appdata.** The board can rebind the six defaults and
  test them; adding a kind that is not in the mapping is a host/agent write to
  `sfx.mapping@1`. A text input is a UI feature this slice does not have.
- **MP3 decoding is guest-side.** `symphonia` is MPL-2.0; the Spore stays MIT
  and the decoder is an unmodified dependency.
