# GPU surfaces in the Lua UI

The native GraphGPU dashboard and the 20k-point rotating cloud are `soil-scene`
producers: the data, decimation and wgpu draw stay in the native host. The Lua
UI is a tree of retained `rect`/`text` nodes. Those are different contracts.
This note records how to put the former inside the latter without pretending a
Lua tree can draw a million points.

## What exists

- `soil-scene` already has `Polyline`, `Area`, `Band`, `Points`, `Heatmap`,
  `Candles`, `DenseResource` and `PointCloudResource`.
- `soil-scene-gpu` already renders all of those, including the resident 1M-point
  dense source and the 20k-point 3D cloud.
- `soil-viz` already runs a `LiveSpec` at 60 Hz. `dense-3d` and
  `chartgpu-dashboard` are Lua-authored `fungos.live` specs.
- Canvas's frontier (`canvas-soil-scene`, `pi/soil-scene`) has
  `Present::GpuLease`, `spore_socket::gpu_frame_shm`, and `gpu_lease_layer` for
  displaying a producer's frame.
- Canvas's Lua ABI is `canvas.group`, `canvas.text`, `canvas.rect`,
  `canvas.image`, `canvas.mesh`, and host resources. It has no polyline, points,
  dense or point-cloud primitive.
- `spore_present.rs` currently maps `Area`, `Band`, `Points`, `Heatmap`,
  `Candles`, `Polyline`, `Shader`, `GpuDense` and `GpuPoints3D` to `Group`. That
  is the visible regression: a Soil scene round-trips through Canvas as an empty
  group.
- The Canvas running in this session accepted `present` with a `gpu_lease` but
  rejected `gpu_frame` as an unknown kind and did not open a surface. It is not a
  build with the GPU-frame receiver.

## Option A — GPU lease surface (recommended first)

Keep the native renderer as the producer. Canvas owns the surface and presents
the producer's pixels.

```text
LiveSpec (Lua data)
    -> soil-live-gpu (native)
       -> LiveViz / wgpu render
       -> gpu_frame_shm (RGBA + revision)
       -> Present::GpuLease
    -> Canvas gpu_lease_layer
    -> Lua UI places a host surface
```

Steps:

1. Add a `soil-live-gpu` binary, modeled on `soil-vt-gpu`:
   `--spec`, `--socket`, `--source-id`, `--size`, `--fps 60`, `--hold`.
   It connects to Canvas `ui.sock`, sends `hello` + `present` (`gpu_lease`), then
   renders `LiveViz` frames and publishes `gpu_frame_shm` with a monotonic
   revision.
2. Add `HostResourceKind::GpuLease` to the Canvas Lua manifest/resource table.
   `canvas.host_resource` places the lease by rect. Canvas already has
   `gpu_lease_layer`; it only needs a resource placement path and input routing.
3. `ui.host_surface` is now the Lua-side placement point. It allocates layout
   space and calls `canvas.host_resource`; it never receives pixels or a texture
   handle. The Canvas resource kind is still missing.
4. Give the lease a hit region and forward input through the existing
   `spore_socket::send_input` path. The producer owns interaction semantics;
   Canvas owns the coordinates and focus.

This is 1:1 for the native renderer. The tradeoff is a frame copy: 1200x840 RGBA
is about 4 MB per frame, or about 242 MB/s at 60 Hz. `gpu_frame_shm` avoids
base64; a true texture lease would avoid the copy but is not implemented yet.

## Option B — SoilScene node/resource (long-term composition)

Make the Soil vocabulary the scene vocabulary:

- Extend Canvas `NodeContent`/renderer with `Polyline`, `Area`, `Band`, `Points`,
  `Heatmap`, `Candles`, `GpuDense`, `GpuPoints3D`.
- Replace the `spore_present.rs` degrade-to-`Group` arms with real nodes.
- Expose one `canvas.soil_layer` (or a generic `canvas.scene_layer`) to Lua that
  names a host resource and a rect.
- Keep GPU resources host-local. Lua references a source id; a million points do
  not cross JSON.

This is the better long-term model: one scene tree, composable charts and
particles, one hit-test path. It is more work because Canvas has to render the
Soil vocabulary, and because `GpuDense`/`GpuPoints3D` need resource lifetimes and
input attachment.

## Option C — pure Lua marks

Draw the dashboard with `ui.chart` bars and the point cloud with one node per
point. This is the current static fallback. It is not 1:1:

- Canvas caps a module at 256 nodes by default (1,024 hard cap);
- there is no polyline/point primitive;
- a 20k-point cloud would be 20k nodes;
- every frame would rebuild Lua and upload CPU-side geometry.

Keep it as a headless/portable fallback, not as the live demo.

## Budget

The frontier Canvas already answers the 256-node question:

- `canvas-soil-scene` (`pi/soil-scene`): `DEFAULT_MAX_NODES = 1_024` and
  `DEFAULT_SOURCE_BYTES = 256 * 1024`.
- The Canvas currently running here still has `256` nodes and `64 KiB`; a
  300-node probe is rejected with `Canvas Lua scene exceeds its node limit`, and
  a 1100-node probe is rejected by the source-byte limit.
- `fungos-sdk/tools/tend.lua` still defaults to `256` nodes and `64 KiB`. Raising
  the host alone leaves `tend` rejecting the same scene before it is written.

So the change is not a new ceiling: the hard cap is already 1,024. It is aligning
the running Canvas and `tend` with the frontier defaults. A 1024-node module also
needs the 256 KiB source budget; source bytes, not node count, is what the large
probe hit first.

This does **not** make the point cloud a Lua tree. A 20k-point cloud is still a
GPU resource/lease; node headroom only helps ordinary retained UI.

## Recommendation

Start with Option A. It preserves the native look and 60 Hz behavior, it uses
the `gpu_frame_shm` and `gpu_lease_layer` work already present on the frontier,
and it does not require inventing a second chart renderer. Then add
`ui.host_surface` so the lease can sit inside a Lua tree. Move to Option B when
the Canvas `spore_present` bridge can render the Soil vocabulary directly.

## Acceptance checks

- `soil-live-gpu --spec dense-3d.json --size 1200x840 --fps 60` opens a Canvas
  surface and keeps a monotonic revision.
- A capture of that surface matches `soil-live-frame` for the same spec/time.
- The 20k-point cloud is visible and rotates; the 1M dense series is decimated by
  the native host.
- `ui.host_surface` placed in a two-panel tree remains inside its rect after a
  resize and routes pointer events to the producer.
- The pure-Lua fallback is labelled fallback and is not used for the demo.
