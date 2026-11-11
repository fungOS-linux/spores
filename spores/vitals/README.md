# Vitals Spore

Cross-device health fusion over the typed signals device Spores already write.
It is **headless by design**: no radio, no Canvas Lua, no pixels. A native GPUI
host reads `vitals.latest` (or subscribes to the `vitals.summary.v1` bus frame)
and owns the UI.

## Commands

```text
vitals seed [--now MS]
vitals once [--wearer W] [--now MS]
vitals loop [--wearer W] [--iterations N] [--interval-ms N]
```

- `seed` writes deterministic owner/watch/ring and guest/ring samples, plus
  device state. It is the hardware-free data set for the GPUI viewer.
- `once` scans `signal`, fuses one wearer-scoped summary, writes
  `vitals.summary` and `vitals.latest`, and publishes `vitals.summary.v1`.
- `loop` repeats the cycle on `soil:time` ticks. A real host would run this as a
  small supervisor; the dev host is one-shot.

## Fusion policy

- **Ring primary** for overnight HR/HRV/SpO2/temperature and sleep. PineTime is
  the fallback for HR/sleep and the primary for motion/step context.
- **Sleep stages are selected from one source**, never averaged or merged.
- **Steps stay per device.** `steps.by_device` is a map; there is no total.
- **Stale is unknown.** Samples older than `fresh_ms` (60 s) are not used;
  older than `stale_ms` (300 s) are expired. Missing values are `null`, never
  zero.
- **Wearer-scoped.** A guest ring is not owner data; unknown wearers are not
  silently treated as the owner.

## Records and bus

- Reads `signal` (`signal.sample.v1`) and ignores malformed bodies.
- Writes `vitals.summary` keyed `<wearer>:<observed_at>` and `vitals.latest`
  keyed `<wearer>`, both schema `vitals.summary.v1`.
- Publishes one `vitals.summary.v1` frame per cycle. The body is the same JSON;
  a GPUI host can poll records or observe the bus.

## Grants

| Grant | Why |
|---|---|
| `soil:records` | read signals/device state; write summaries |
| `soil:unibus` | publish the latest summary frame |
| `soil:time` | monotonic timestamps and loop ticks |
| `wasi:cli`, `wasi:clocks`, `wasi:io` | Rust std runtime; no file or network I/O |

No socket, process, secret, environment, raw filesystem, camera, microphone, or
wall-clock grant.

## Build

```sh
cargo test -p vitals-core
cargo build -p vitals-spore --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/vitals_spore.wasm module.wasm
```

`soil-dev-host` already links the three capabilities this Spore needs; run it
with `vitals/module.wasm seed` followed by `once --wearer owner` to see the
summary and bus frame.
