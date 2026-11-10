# JARVIS telemetry Spore

A Spore, not a standalone binary. It reads the typed signals that device Spores
write into `soil:records`, applies the wearer/freshness/fusion policy, and
publishes `presence.state.v1` and `attention.state.v1` latest-value frames for
JARVIS. It has no radio, camera, socket, microphone, or wall clock.

## Commands

```text
jarvis-telemetry seed [--now MS]
jarvis-telemetry once [--wearer W] [--synthetic] [--now MS]
jarvis-telemetry loop [--wearer W] [--iterations N] [--interval-ms N] [--synthetic]
```

- `seed` writes deterministic owner (`sim-pinetime`) and guest (`sim-ring`)
  signals into Records. It is the hardware-free starting point.
- `once` scans `signal` records, fuses one snapshot, and publishes it.
- `loop` repeats the scan/publish cycle with `soil:time` ticks.
- `--synthetic` is an explicit development switch: with no visible records it
  emits a deterministic cycle and marks `evidence.synthetic = true`. It is never
  silently substituted for real data.

## Policy

- **Wearer-scoped.** A signal is visible only when its `wearer` matches the
  requested wearer. A null/unknown wearer is not the owner. The seeded guest
  ring exists to exercise this boundary.
- **Never sum steps.** `steps_by_device` returns one row per device. The same
  walk seen by a watch and a ring stays two rows, not one total.
- **Stale is unknown.** A signal older than `fresh_ms` is stale; older than
  `stale_ms` is expired. `bpm()`/`steps()` return `None`, and presence becomes
  `unknown`, never zero. A timestamp in the future is treated as stale because
  the clocks disagree.
- **No raw audio on the bus.** `bus_payload` refuses `audio.*` bodies with
  `raw`/`bytes`; only a content-addressed `blob_ref` is publishable.
- **Presence vs attention.** Presence is `present | away | unknown`; attention
  is `focused | available | asleep | unknown`. Attention is never more
  confident than presence, and the evidence object includes the per-device
  steps and freshness counts that produced it.

## Records read

- `signal` collection, schema `signal.sample.v1`

The Spore does not own this schema; the device Spores do. It only consumes what
is present and skips bodies that do not carry the minimum identity/time fields.
Seed mode also writes `device.state.v1` records so the development store looks
like a real one, but fusion is driven by signals.

## Grants

| Grant | Why |
|---|---|
| `soil:records` | read signals, write seed records |
| `soil:unibus` | publish presence/attention frames |
| `soil:time` | monotonic timestamps and loop ticks |
| `wasi:cli`, `wasi:clocks`, `wasi:io` | Rust std runtime; no file or network I/O |

## Build

```sh
cargo test -p jarvis-telemetry-core
cargo build -p jarvis-telemetry-spore --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/jarvis_telemetry_spore.wasm module.wasm
```

The core crate is pure Rust and holds the policy tests. `soil-spore check .`
verifies the manifest against the module's real imports.

## Development without hardware

1. `jarvis-telemetry seed` — writes fake owner and guest signals.
2. `jarvis-telemetry once --wearer owner` — emits presence/attention for the
   owner only.
3. `jarvis-telemetry once --wearer guest` — sees only the guest ring.

This is the intended JARVIS development loop until the PineTime/Ring/OMI
Spores are writing real records.
