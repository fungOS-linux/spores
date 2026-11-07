# dfm-report

A Spore that shows DFM findings over the model they are about:

* `module.wasm` — a real wgpu 29 render pass on `wasm32-wasip2`. It imports
  standard `wasi:webgpu` plus `soil:canvas-gpu`, draws the mesh into a texture and
  presents it. No window, no swapchain, no native handle: Canvas owns those.
* `ui/report.lua` — the findings as a Lua projection, and the `dfm.note` /
  `dfm.open_native` intents.

Both read the same `model/data.json`, so the list and the render cannot disagree
about what is flagged. Nothing here re-derives a DFM result.

## The scene file

`model/data.json` is produced outside the Spore:

```sh
# 1. segment the mesh; this records per-triangle patch assignment
cargo run -q -p transmog-cli --release --bin transmog -- patches part.step

# 2. run DFM and keep the JSON contract
cargo run -q -p transmog-cli --release --bin transmog -- dfm part.step --json > /tmp/dfm.json

# 3. join them into the scene the Spore and the viewer both read
python3 scripts/build-dfm-spore.py patches.json /tmp/dfm.json model/data.json
```

A finding gets a 3D marker only when its rule maps onto recognised geometry.
Everything else is listed and marked `—` in the pin column: a finding is never
dropped and never given an invented anchor.

## Build the 3D module

```sh
cd guest
CARGO_TARGET_DIR=/tmp/dfm-guest-target cargo build --target wasm32-wasip2 --release
cp /tmp/dfm-guest-target/wasm32-wasip2/release/dfm_report_guest.wasm ../module.wasm
```

Building outside the Spore keeps `target/` out of the package; an earlier build
inside it produced a 149 MB Spore instead of a 500 KB one.

## Check the render without a window

The frame is presented to Canvas, so the way to look at it in CI is the WebGPU
runner, which writes the presented frame back out:

```sh
cargo run -q -p soil-webgpu --bin soil-webgpu -- \
  --frame /tmp/dfm.bgr --size 1280x900 module.wasm
```

`.bgr` is raw 24-bit BGR, top-down. Converting to BMP is a header plus a row flip;
that is how the shading was verified.

The scene is baked in at build time. A guest has no filesystem unless it imports
`soil:vfs`, and the canvas runner links only `wasi:webgpu` and `soil:canvas-gpu`.
Reading the scene at runtime is a separate grant and a separate change.

## Build the module

```sh
cargo build --manifest-path spores/dfm-report/Cargo.toml \
  --target wasm32-wasip2 --release
cp spores/dfm-report/target/wasm32-wasip2/release/dfm_report_spore.wasm module.wasm
cargo run -q -p soil-spore --bin soil-spore -- lock spores/dfm-report
cargo run -q -p soil-spore --bin soil-spore -- check spores/dfm-report
```

`check` reports `ok dev.fungos.dfm 0.1.0 9 files <digest>`.

## What is deliberately absent

The module imports nothing. The Spore has no filesystem, no network, and no say
in the renderer: `soil:vfs` read access to the bundled scene is the only grant,
and the 3D surface is opened by intent (`dfm.open_native`), not drawn by the UI.
