# PineTime Spore

A concrete wearable Spore for the PineTime running InfiniTime. It owns the
InfiniTime wire formats, the wearer identity, and the typed signal envelope;
the radio transport and the single-host lease stay in `soil-ble-host`.

## Why `pinetime`, not `smartwatch`

`pinetime` is the honest first target: the service UUIDs, HR layout, motion
units, notification framing, and music service are InfiniTime-specific. A
generic `smartwatch` capability would either leak those details into a fake
abstraction or become a profile registry with one row. When a second watch is
actually supported, add a `smartwatch` profile boundary and make this Spore one
implementation of it. The Spore id is `dev.fungos.pinetime` for the same reason:
renaming an id rekeys per-app Canvas settings and records readers.

## Commands

```text
pinetime watch   [--wearer W] [--device ID] [--scan-ms N] [--hold-ms N]
pinetime notify  [--category C] [--title T] [--body B] [--wearer W]
pinetime media   [--artist A] [--track T] [--album L] [--playing true|false]
pinetime find    [--wearer W]
pinetime time    --epoch UNIX [--offset-min N]
pinetime nav     --icon NAME [--narrative T] [--distance D] [--progress 0-100]
pinetime weather --epoch UNIX --icon NAME --temp-c C [--min-c C] [--max-c C]
                 [--location L] [--sunrise-min N] [--sunset-min N]
                 [--forecast "min,max,icon;min,max,icon"]
```

- `watch` reads DIS model/firmware/serial and battery, subscribes to heart rate,
  step count, motion, music events, call events, and battery, then drains for
  `--hold-ms` (default 5000).
- `notify` writes an ANS New Alert in InfiniTime's
  `<category><amount>\x00<title>\x00<body>` format. `--category` takes the
  spec's names (`simple`, `email`, `news`, `call`, `missed-call`, `sms`,
  `voicemail`, `schedule`, `high-priority`, `instant-message`, `all`) or its
  byte.
- `media` writes playback status, artist, track, and album, then drains music
  events for `--hold-ms`.
- `find` writes Immediate Alert level 2, which makes the watch vibrate.
- `time` writes the Current Time Service: local fields, weekday, and the
  manual-update reason. The watch has no timezone or network, so `--epoch` is
  the caller's instant and `--offset-min` is the local offset to display.
- `nav` writes the four Navigation characteristics: a validated `pure-maps`
  icon name, the instruction, the distance to it, and route progress.
- `weather` writes Simple Weather current conditions (version 1, with
  sunrise/sunset) and, with `--forecast`, a five-day forecast. Temperatures are
  decimal Celsius on the command line and hundredths in the packet; sunrise and
  sunset accept the service's `-1` (unknown) and `-2` (does not occur).

Every write command reports what it sent. `--read-back` also reads each
characteristic immediately after the write and records `true`, `"mismatch"`, or
`"unsupported"` (write-only on that firmware) in the snapshot.

## Spec coverage

| InfiniTime service | Spore |
|---|---|
| Device Information, Battery, Heart Rate | read and subscribe |
| Alert Notification (`0x2A46`) | `notify`, any category |
| Notification Event (`00020001`) | call responses as signals |
| Music (`0000000x`) | `media` plus event subscription |
| Motion (`0003000x`) | steps and motion signals |
| Navigation (`0001000x`) | `nav`, icon-validated |
| Simple Weather (`0005000x`) | `weather`, current + forecast |
| Current Time (`0x2A2B`) | `time` |
| Immediate Alert | `find` |

Deliberately not implemented: **DFU** and **BLE FS**. Both are file transfers —
firmware images and watch-face assets — and this Spore has no filesystem grant.
DFU in particular is a nine-step state machine that can brick a watch; it belongs
in a host tool with real file access and a recovery path, not in a capability
Spore. Gadgetbridge is the Android app that implements this same InfiniTime wire
protocol; the protocol is what is implemented here, not the app.

## Streams

`--streams` narrows what a session subscribes to and publishes. It can only
narrow: the profile decides what the watch can emit, and a stream it does not
offer is not selectable.

```text
pinetime watch --streams hr,steps          # a health sync
pinetime watch --streams media,calls       # a control session, no health data
pinetime watch                             # every stream the profile declares
```

Streams are `hr`, `steps`, `motion`, `media`, `calls`, `battery`, or `all`. An
unknown name is an error that lists the known ones — a typo must not silently
subscribe to everything. `battery` is device state, not a body signal: it
updates the device record and never writes a sample.

## Shape

Only one of these three knows what a PineTime is:

- **`spores/infinitime`** — the profile: service and characteristic UUIDs,
  packet layouts, notification parsers, and the release each service arrived in.
- **`spores/watch-core`** — the seam: streams, the signal envelope, the device
  record body, the identity rule, and the `WatchProfile` trait.
- **`spores/pinetime`** — this Spore: the BLE session, the lease, the CLI, and
  one profile wired in.

A second watch is a new profile crate and a new Spore that reuses `watch-core`,
not a fork of this one. The id stays `dev.fungos.pinetime` because renaming it
would rekey records readers and per-app Canvas settings.

## Identity and records

- `device_id` comes from the DIS serial. If the firmware exposes no serial, the
  Spore refuses to invent one from the platform BLE address; pass a provisioned
  `--device` id instead. A rotating address must never split one watch into many
  devices.
- `wearer` is explicit and defaults to `owner`. A watch on a guest's wrist is
  the guest's data even when the owner's host saw the packet. The Spore never
  infers a wearer from proximity.
- `device.state` collection: one `device.state.v1` record per device with
  model, firmware, battery, wearer, and monotonic observation time.
- `signal` collection: append-only `signal.sample.v1` records for
  `body.hr.v1`, `body.steps.v1`, `body.motion.v1`, `media.event.v1`, and
  `call.response.v1`.
- A heart-rate measurement of zero is stored as `bpm: null` with `quality: 0`,
  never as a zero BPM. Step counts stay per device; the Spore never sums two
  devices. Raw audio is never written to the bus.

## Grants

| Grant | Why |
|---|---|
| `soil:ble` | observe (scan/services/read/subscribe/next-event), connect (take the radio lease), write (notifications, media, find) |
| `soil:records` | read previous state and write device/signal records |
| `soil:time` | monotonic observation timestamps |
| `wasi:cli`, `wasi:clocks`, `wasi:io` | Rust std runtime on `wasm32-wasip2`; no file or network I/O |

No socket, process, secret, environment, raw filesystem, or wall-clock grant.

## Build

```sh
cargo test -p pinetime-protocol
cargo build -p pinetime-spore --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/pinetime_spore.wasm module.wasm
```

The protocol crate is pure Rust and runs on the host. The Spore crate is the thin
BLE/records wrapper. `soil-spore check .` verifies the manifest against the
module's real imports.

## Host requirement

The module imports `soil:ble`, `soil:records`, and `soil:time`. On desktop,
`soil-dev-host` composes those with `soil:unibus` and runs this Spore against
the paced simulator (or `--real` for CoreBluetooth/BlueZ). `soil-ble-host`
remains the single-protocol BLE CLI and links only `soil:ble` + WASI.

## Simulator

`soil-ble-sim` exposes a virtual PineTime with the same InfiniTime services,
deterministic ticks, battery drain, packet loss, disconnects, stale reads, and
replayable golden records. Run the host with `SOIL_BLE_SIM=1` to exercise this
Spore without a radio:

```sh
SOIL_BLE_SIM=1 soil-ble-host module.wasm watch --wearer owner --hold-ms 1000
```

## Status

- The protocol mapping and the wasm component build are covered by tests.
- The records host is required for persistence; without it `run` fails loudly.
- Continuous overnight HR logging is not implemented in stock InfiniTime; this
  Spore consumes the notifications the firmware actually emits.
