# Vitals Spore

Cross-device health fusion over the typed signals device Spores already write.
The Spore owns both the health policy and the **view model**: `view()` returns
`vitals.view.v1` cards and per-device step rows. A GPUI renderer host draws
those cards; it does not recompute health policy or own the layout semantics.

The Spore is headless in the sense that it has no pixels, window, or GPU
handle. GPUI is the rendering host; `wasm32-wasip2` has no native window.

## Commands

```text
vitals seed [--now MS]
vitals once [--wearer W] [--now MS]
vitals loop [--wearer W] [--iterations N] [--interval-ms N]
```

- `seed` writes deterministic owner/watch/ring and guest/ring samples, plus
  device state. It is the hardware-free data set for the GPUI viewer.
- `once` scans `signal`, fuses one wearer-scoped summary, writes
  `vitals.summary`, `vitals.latest`, and `vitals.view`, and publishes
  `vitals.summary.v1`.
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

## Spore UI

`view()` returns the renderer-agnostic view tree:

```json
{
  "schema": "vitals.view.v1",
  "title": "Vitals",
  "wearer": "owner",
  "rings": [
    {"id":"move","label":"Move","value":9000,"max":10000,"unit":"steps","source_device":"fused","freshness":"fresh","detail":"per-bucket-max"},
    {"id":"heart","label":"Heart","value":58,"max":180,"unit":"bpm","source_device":"sim-ring","freshness":"fresh"},
    {"id":"oxygen","label":"Oxygen","value":97,"max":100,"unit":"%","source_device":"sim-ring","freshness":"fresh"}
  ],
  "cards": [{"id":"heart_rate","label":"Heart rate","value":58,"unit":"bpm","source_device":"sim-ring","freshness":"fresh"}],
  "steps": {"rows":[{"device":"sim-pinetime","count":4231},{"device":"sim-ring","count":9000}],"primary_device":"sim-pinetime","estimate":{"value":9000,"source_device":"fused","policy":"per-bucket-max","contributors":{"sim-ring":9000}}},
  "empty": false
}
```

The rings are display scales for the current readings, not a second fusion
policy. Move uses `steps.estimate`: per-device cumulative step signals are
turned into deltas, a counter decrease is treated as a reset, each delta lands
in the five-minute bucket containing the later sample, and the estimate is the
sum of the per-bucket maxima. There is deliberately no step `total` field.
`source_device` is `fused`; `contributors` records which device won which
buckets, and `confidence` is the fraction of UTC-day buckets covered. Sparse
sampling lowers coverage, and the current implementation attributes a delta to
the later sample's bucket rather than interpolating at bucket boundaries.
A GPUI host composes the rings and cards into the actual window; the Spore owns
the labels, values, units, sources, freshness, and the no-sum rule.

## Records and bus

- Reads `signal` (`signal.sample.v1`) and ignores malformed bodies.
- Writes `vitals.summary` keyed `<wearer>:<observed_at>` and `vitals.latest`
  keyed `<wearer>`, both schema `vitals.summary.v1`, plus `vitals.view` keyed
  `<wearer>` with schema `vitals.view.v1` for a renderer Spore.
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
