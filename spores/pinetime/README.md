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
pinetime watch  [--wearer W] [--device ID] [--scan-ms N] [--hold-ms N]
pinetime notify [--category C] [--title T] [--body B] [--wearer W]
pinetime media  [--artist A] [--track T] [--album L] [--playing true|false]
pinetime find   [--wearer W]
```

- `watch` reads DIS model/firmware/serial and battery, subscribes to heart rate,
  step count, motion, music events, call events, and battery, then drains for
  `--hold-ms` (default 5000).
- `notify` writes an ANS New Alert in InfiniTime's
  `<category><amount>\x00<title>\x00<body>` format.
- `media` writes playback status, artist, track, and album, then drains music
  events for `--hold-ms`.
- `find` writes Immediate Alert level 2, which makes the watch vibrate.

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
