# vitals-view

A **Spore UI**, not a native companion app. The guest:

1. reads the Spore-owned `vitals.view.v1` record from `soil:records`;
2. builds the cards with **GPUI**;
3. renders them through the Soil-provided `wasi:webgpu` device using GPUI Fast's
   offscreen `WgpuHeadlessRenderer`;
4. presents the resulting texture through `soil:canvas-gpu`.

Canvas owns the window, compositor, input, and capture. The Spore owns the UI
structure, labels, values, units, sources, freshness, and the no-total rule. No
window, swapchain, or native handle enters the guest.

![vitals-view rendered through soil:canvas-gpu](assets/vitals-view.png)

## Run

Seed the records first with the headless `vitals` Spore:

```sh
soil-dev-host --records-dir .soil-dev/records spores/vitals/module.wasm seed
soil-dev-host --records-dir .soil-dev/records spores/vitals/module.wasm once --wearer owner
```

Then run the renderer through Soil's WebGPU host. It opens the records root
read-only, links `wasi:webgpu` + `soil:canvas-gpu` + `soil:records`, and writes
the captured frame as the same BGR24 format `soil-webgpu` already uses:

```sh
# from the soil checkout; SPORES names this checkout
cargo run -q -p soil-webgpu --bin soil-webgpu -- \
  --records-dir .soil-dev/records \
  --frame /tmp/vitals.bgr --size 1280x900 \
  "$SPORES/spores/vitals-view/module.wasm"
```

`vitals.view` is keyed by wearer and written by `vitals once`. This renderer
reads the `owner` key; an empty store renders an honest empty state rather than
inventing values or showing a guest's data.

## GPUI boundary

This uses GPUI's `headless` feature: a real GPUI window renders against
`WgpuContext::from_offscreen`, backed by `wasi:webgpu`. `soil:canvas-gpu` takes
the texture by borrow; the host chooses the extent and composites it.

The guest still has no authority over the window. A revoked surface closes the
lease; the Spore cannot keep drawing to a hidden window. The only font is the
bundled IBM Plex Sans face (SIL OFL 1.1); a WASI component has no system fonts.

## Build

```sh
cargo build --manifest-path spores/vitals-view/Cargo.toml \
  --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/vitals_view_spore.wasm module.wasm
```

GPUI Fast is a local path dependency on the `wasip2` branch. The guest's
`wgpu-wasi-webgpu` is pinned to the `wgpu-30` branch so its `wgpu` types match
`gpui_wgpu`'s wgpu 30.0.1; the Soil host still uses its wgpu-core 29 runtime on
the other side of the component boundary.
