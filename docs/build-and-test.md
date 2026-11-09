# Build, test, package, and probe a Spore

This is the practical companion to
[`docs/hello-world-spore.md`](hello-world-spore.md). It records the commands
that actually produce a shippable Spore, the checks that catch real failures,
and the audio harness used to prove the SFX path on macOS and to Isochrone.

## Prerequisites

| Tool | Why | Check |
|---|---|---|
| Rust + `wasm32-wasip2` | build the component | `rustup target add wasm32-wasip2` |
| `wasm-tools` | validate the component and inspect imports | `wasm-tools --version` |
| Lua 5.4+ | compose and fixture-test the UI | `lua -v` |
| `soil-spore` | lock, pack, verify, digest | `soil-spore` with no args prints usage |
| Zig 0.15.2 | only for hosts that build `libghostty-vt-sys` | `zig version` |

A Spore is its own Cargo workspace. Build it from `spores/<name>`, not from the
repository root; the artifact is `wasm32-wasip2`, and `cargo build` at the root
is a host build with a different target and dependency graph.

## The fast local loop

From `spores/sfx`:

```sh
cargo fmt --all --check
cargo test --lib
cargo clippy --all-targets -- -D warnings
cargo build --release --target wasm32-wasip2
cp target/wasm32-wasip2/release/spore_sfx.wasm module.wasm
wasm-tools validate module.wasm
```

Why each step exists:

- **`fmt --check`** keeps generated and hand-written code reviewable. Use
  `cargo fmt --all` to fix.
- **`test --lib`** runs the host-testable core: mapping, actions, snapshot,
  decode, synth. These are the tests that can run without wasmtime or a
  desktop.
- **`clippy --all-targets -- -D warnings`** is the CI gate. `--all-targets`
  includes the fixture and example targets, which is where stale API uses hide.
- **`build --release`** is the shipped profile. Debug wasm is larger and slower;
  do not package it.
- **`cp`** is deliberate. The manifest points at `module.wasm` at the package
  root, so the filename is part of the contract. Do not make the manifest point
  into `target/`; that path is machine-specific and does not exist in a clean
  checkout.
- **`wasm-tools validate`** checks the binary is a well-formed component, not
  just a wasm module. A component with a bad canonical ABI fails here before a
  host sees it.

### Confirm the imports are the ones you requested

A valid component can still import the wrong thing. Inspect it:

```sh
wasm-tools component wit module.wasm
```

The SFX module must show:

```text
import soil:appdata/appdata@1.0.0;
import soil:vfs/vfs@1.0.0;
import soil:audio/audio@0.1.0;
```

and must not import `wasi:audio`, `wasi:sockets`, or a filesystem package. The
manifest's `[denied]` section is a statement of intent; the component imports
are the evidence. If they disagree, the package is wrong even if it runs on one
host.

## Test the Lua view without a host

Canvas is not required to know whether the board paints.

```sh
cd spores/sfx
lua tests/ui_fixture.lua
lua tools/compose_sfx.lua /tmp/sfx-modules
```

The fixture loads the real `sdk/ui/ui.lua`, the real `ui/board.lua`, and
`ui/board.example.lua`. It stubs `canvas.text`, `canvas.rect`, and
`widget.button`, then asserts:

- the view painted more than an empty frame;
- the mapping and pad sections exist;
- the live room is named;
- every text primitive has a string and an ink role;
- pads and rule controls have stable, unique ids;
- the narrow layout still renders.

Expected fixture shape: **83 text primitives and 36 buttons**. Those numbers
are not the contract; the assertions are. If the count changes, inspect the
scene before changing the expected text.

`tools/compose_sfx.lua` resolves `fungos.ui` from the SDK package and writes one
self-contained module plus a manifest. This is the same composition Canvas
loads. A view that requires a sibling file will fail here, which is the point:
Canvas strips `require` and `package`.

## Package the Spore

From the repository root:

```sh
soil-spore lock spores/sfx
soil-spore pack spores/sfx -o dist/sfx-0.1.0.spore
soil-spore verify dist/sfx-0.1.0.spore
soil-spore digest dist/sfx-0.1.0.spore
```

- **`lock`** resolves the package's WIT dependencies and writes `spore.lock`.
  Commit it. A package that resolves differently on two machines is not a
  package.
- **`pack`** includes the manifest, module, UI, docs, and lock. It is not a tar
  of the working directory; build output and editor droppings are excluded.
- **`verify`** checks the package layout and digest. Run it on the exact file
  you intend to publish, not on a directory.
- **`digest`** is the identity of the artifact. Record it in the release note or
  the handoff message.

If `soil-spore` is not on `PATH`, the binary from a Soil checkout is enough:

```sh
/Users/ajmwagar/src/soil/.delta/worktrees/ghw2691f4ztq/soil/target/debug/soil-spore \
  pack spores/sfx -o dist/sfx-0.1.0.spore
```

Before packing, remove AppleDouble files if the package was ever copied through
Finder or a tar without `--no-xattrs`:

```sh
find spores/sfx -name '._*' -delete
```

These are metadata, not package content; including them makes the digest noisy
and can trip a verifier that expects a clean tree.

## Render the built-in pads to a WAV

The SFX package has a host-side example that renders every built-in pad with a
quarter second of silence between them:

```sh
cd spores/sfx
cargo run --example render_demo -- sfx-demo.wav
```

This proves the synth and WAV writer without a room, a host, or a network. It is
the cheapest way to answer "is the sound itself wrong?" before debugging the
output path. `dist/sfx-demo.wav` is about 5.94 seconds with a peak around
-5.2 dBFS.

## Probe the real audio path

The SFX repository contains a tiny `wasi:audio`-only component,
`tools/sfx-probe`, and a prebuilt `dist/sfx-probe.wasm`. The `wasi-audio` host
from Soil is the actual PCM sink used by the desktop path, so this is a real
integration test, not a mock.

### macOS speakers

```sh
cd /Users/ajmwagar/src/soil/.delta/worktrees/dpqz8h740d4r/soil
/tmp/sfx-target/debug/wasi-audio \
  /Users/ajmwagar/src/fungOS/spores/.delta/worktrees/dpqz8h740d4r/spores/dist/sfx-probe.wasm
```

Expected output: `audio: ok`, and an audible tone from the Mac's current output
device. If the room is silent, check the macOS output device and volume before
touching the component.

### Isochrone loopback

Run a UDP listener first, then point the host at it:

```sh
python3 - <<'PY'
import socket
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.bind(("127.0.0.1", 50040))
count = 0
while True:
    s.recvfrom(65535)
    count += 1
    if count % 500 == 0:
        print(count, flush=True)
PY
```

```sh
/tmp/sfx-target/debug/wasi-audio dist/sfx-probe.wasm \
  --isochrone-destination 127.0.0.1:50040 \
  --isochrone-bind 127.0.0.1:0 \
  --isochrone-ssrc 1399813484
```

Expected: the listener receives RTP packets continuously for the duration of
the probe. The recorded run captured **5,937 RTP packets**. The exact count
depends on the probe length and packetization; zero is the failure.

### Home-pi

The home-pi receiver listens on UDP `50040` and bridges to JACK
`system:playback_1/2`:

```sh
/tmp/sfx-target/debug/wasi-audio dist/sfx-probe.wasm \
  --isochrone-destination 192.168.2.74:50040 \
  --isochrone-bind 0.0.0.0:0 \
  --isochrone-ssrc 1399813484
```

Check both sides:

```sh
ssh ajm@192.168.2.74 'systemctl --user status isochrone-soil-receiver isochrone-jack-bridge'
ssh ajm@192.168.2.74 'journalctl --user -u isochrone-soil-receiver -n 50 --no-pager'
```

A successful run shows `stream_changes` in the receiver and the JACK bridge
bound to `system:playback_1/2`. Receiver logs and JACK state are evidence that
the RTP arrived and was routed. They are **not** evidence that a human in the
room heard it; record that separately if it matters. The initial zero-packet
run was caused by the route/socket setup, so check receiver state before
assuming the sender is wrong.

## Heavy builds: use the build machine

The full Soil workspace is larger than the spores workspace and includes
native crates. Build it on `agora-one`, not on a laptop with a full disk.

### Full Soil workspace

The successful command was:

```sh
cd ~/soil-workspace
PATH="$HOME/zig-0.15.2:$PATH" \
CARGO_TARGET_DIR="$HOME/soil-workspace-target" \
RUSTC_WRAPPER= \
cargo build --workspace
```

Result: **`Finished \`dev\` profile`** for every workspace member, including
`mush`, `soil-webgpu`, and `soil-audio-host`.

Three things made that work:

1. **Zig 0.15.2 was on `PATH`.** `libghostty-vt-sys` does not build with Zig
   0.16; it needs the 0.15 line. Install the exact tarball, not "latest".
2. **The `spores-public` submodule was present.** `crates/mush` embeds
   `../../spores/mush/wit`; an empty `spores/` symlink target fails with
   "failed to read path for WIT". Copy `spores-public` and link `spores` to
   `spores-public/spores`.
3. **`RUSTC_WRAPPER=`** avoided a stale sccache configuration. Use the empty
   assignment when the wrapper is not available on the build machine.

The first attempt used `--offline` and failed on an uncached crate (`arboard`).
Drop `--offline` on a machine with network; the private git checkouts can still
come from the copied Cargo git cache.

### Spores workspace

```sh
cd /Users/ajmwagar/src/fungOS/spores/.delta/worktrees/dpqz8h740d4r/spores
CARGO_HOME=/tmp/bob-cargo-home \
CARGO_TARGET_DIR=/tmp/spores-workspace-target \
RUSTC_WRAPPER= \
cargo check --workspace
```

This passes. It is a `check`, not a full host build: the spores workspace
excludes `spores/sfx` because SFX is its own workspace, and `spore-terminal`
links a native `soil-vt` library that needs the Zig toolchain. Use the full
Soil build above for the host, and the per-Spore commands for the artifact.

### Disk pressure

A wasm workspace target can consume multiple gigabytes. Keep the target on a
drive with room, and remove it after a build if the machine is tight:

```sh
du -sh /tmp/spores-workspace-target
df -h /System/Volumes/Data
rm -rf /tmp/spores-workspace-target
```

Do not delete `dist/wasi-audio` or `dist/sfx-probe.wasm`; they are small enough
to keep and are the harness for the next audio test.

## Bundled sample pads

The SFX package carries eight CC0 MP3 interface sounds under
`spores/sfx/assets/samples/`. They are not in the default mapping because the
VFS is the authority; install them into a personal instance with:

```sh
SOIL_COMMAND=/tmp/sfx-target/debug/soil-command tools/install-samples.sh
sfx list
```

The script verifies the VFS content id by construction (`fungos-vfs put`), then
`sfx sample add` stores the path and `blake3:<hex>` in appdata. Re-running it is
safe.

## The personal browser sheet

`tools/sfx-web.py` is a loopback browser face over the same appdata mapping. It
pre-renders pads with `sfx render` and serves them as WAV; the page mixes them
with Web Audio. That avoids spawning `soil-command` and opening a CoreAudio
stream for every press.

```sh
python3 tools/sfx-web.py --soil-command /tmp/sfx-target/debug/soil-command
# Mycelium gateway: sfx.myc -> http://127.0.0.1:8795
open 'https://sfx.myc/?v=2'
```

The Canvas Spore UI is Lua (`ui/board.lua` / `ui/pads.lua`); the browser sheet is
HTML/CSS/JS and is a host-side convenience, not a second Spore renderer.

Isochrone does not reduce local press latency: it is the transport for a remote
room, and its jitter buffer adds latency. A persistent SFX host that keeps one
`soil:audio` output open and mixes voices is the correct fix for room-routed
latency; the browser sheet gets the same benefit by keeping one `AudioContext`
open and pre-rendering the pads.

## `tend-spore`: one entry point for the repeatable work

The repository has a small workflow wrapper, `bin/tend-spore`, for the commands
above that are otherwise easy to run in the wrong directory or in the wrong
order. It is in the `tend *` family: it composes and checks the Lua UI, but it
also owns the Rust/wasm/package/audio loop that the generic `tend` tool does not
know about.

```sh
bin/tend-spore test sfx          # fmt, clippy, unit tests, Lua fixture
bin/tend-spore ui sfx            # fixture plus a composed module in /tmp
bin/tend-spore build sfx         # wasm release build, module.wasm, validate
bin/tend-spore pack sfx          # lock, pack, verify, print digest
bin/tend-spore check sfx         # all of the above, in order
bin/tend-spore probe mac         # speakers through the real wasi-audio host
bin/tend-spore probe loopback    # RTP to 127.0.0.1:50040
bin/tend-spore probe home-pi     # RTP to 192.168.2.74:50040
```

Use `SOIL_SPORE=/path/to/soil-spore` and `WASI_AUDIO=/path/to/wasi-audio` when
the binaries are not on `PATH`. `tend-spore` never downloads anything and never
installs a package; it runs the checks and prints the artifact it produced.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `can't find crate for std` on wasm build | target not installed | `rustup target add wasm32-wasip2` |
| `wasm-tools validate` fails | built for the wrong target or stale `module.wasm` | rebuild release, copy the new artifact, validate again |
| component imports `wasi:audio` | wrong WIT dep or manifest ABI | inspect `wit/world.wit`, rebuild, re-check `wasm-tools component wit` |
| `libghostty-vt-sys` build fails | Zig 0.16 | install Zig 0.15.2 and put it first in `PATH` |
| `failed to read path for WIT .../spores/mush/wit` | `spores-public` submodule missing | copy it and link `spores` |
| `pack` includes `._*` files | AppleDouble metadata | `find ... -name '._*' -delete` |
| probe prints `audio: ok` but nothing is heard | wrong output device or muted room | check host device, volume, and room name |
| Isochrone listener sees zero packets | receiver not running, wrong address/port, or route policy | check `isochrone-soil-receiver`, UDP `50040`, and sender bind |
| full workspace build cannot download a crate | `--offline` with an incomplete cache | rerun without `--offline` on a networked machine |

## What CI should run

For each Spore package:

```sh
cargo fmt --all --check
cargo test --lib
cargo clippy --all-targets -- -D warnings
cargo build --release --target wasm32-wasip2
wasm-tools validate module.wasm
lua tests/ui_fixture.lua
soil-spore pack . -o "$RUNNER_TEMP/package.spore"
soil-spore verify "$RUNNER_TEMP/package.spore"
```

Do not make CI depend on a speaker, a room, or a receiver. The probe belongs to
the integration environment, not the unit gate. A package that verifies and a
fixture that renders are the two strongest pre-merge signals.

## Evidence from the SFX build

- `cargo test --lib`: 24 passed.
- `cargo clippy --all-targets -- -D warnings`: clean.
- `wasm-tools validate module.wasm`: ok.
- Component imports: `soil:appdata`, `soil:vfs`, `soil:audio`.
- Lua fixture: 83 text primitives, 36 buttons.
- `soil-spore verify dist/sfx-0.1.0.spore`: ok, 24 files.
- Mac speakers: `audio: ok`.
- Isochrone loopback: 5,937 RTP packets captured.
- Home-pi: receiver `stream_changes`; JACK bridge on `system:playback_1/2`.
- Full Soil workspace build on `agora-one`: finished successfully with Zig
  0.15.2.

Audible confirmation in the home-pi room was not independently recorded. The
receiver and JACK state prove arrival and routing; they do not prove a person
heard it. Say which one you have.
