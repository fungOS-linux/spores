# GPUI spores — the native path

Status: the required path for new Spores that draw. Existing scene spores keep
the Lua contract; they are not rewritten. The web bridge is not built yet.

A GPUI Spore owns its view: layout, text shaping, colours, and the render
commands. The host owns exactly two things:

1. a WebGPU device, handed to the guest through `wasi:webgpu@0.3.0-rc.2`;
2. a compositor surface, reached through `soil:present/surface`.

No window, swapchain, display handle, or native texture handle crosses the
boundary. The guest renders offscreen and hands over a texture the host already
knows how to present.

[`spores/terminal-gpui`](../spores/terminal-gpui) is the reference. Copy its
shape for a new Spore.

## Why this replaces the per-Spore scene work

The older path (Rust snapshot → Lua `ui.lua` → Canvas scene) is still the
contract for existing scene spores. It keeps the host in charge of every pixel
and is the right fit for documents that should inherit Canvas chrome.

A GPUI Spore is the right fit when the application *is* the interface: a
terminal, an editor canvas, a visualiser, a tool with its own interaction
model. It gets a real layout tree, real text shaping, and real GPU drawing
without extending the host's scene vocabulary for every new widget.

The rule that survives is the boundary, not the renderer: the Spore never gets
a window or a device handle it can keep, and it cannot tell which host is
presenting it. Only the presenter changes.

## The contract

```text
host                                   guest (wasm32-wasip2)
────                                   ─────────────────────
wasi:webgpu instance ────────────────► request_adapter / request_device
                                       WgpuContext::from_offscreen
                                       HeadlessAppContext + a GPUI view
                                       offscreen render → RGBA frame
soil:present/surface.present ◄──────── host_texture(frame)
```

The host chooses the physical extent. The guest calls `surface::size()` and
renders at that size; it must not assume a window size or a scale factor.

`soil:present` takes a `wasi:webgpu` texture, not raw bytes. The reference
implementation reads the GPUI frame back to RGBA, uploads it into an
`Rgba8Unorm` texture with `COPY_DST | TEXTURE_BINDING | RENDER_ATTACHMENT |
COPY_SRC`, and passes that texture to `present`. A later host can grow a
zero-copy path; the contract does not change.

## The GPUI fork

GPUI is used from [`fungOS-linux/gpui-fast`](https://github.com/fungOS-linux/gpui-fast/tree/wasip2),
branch `wasip2`, rev `eef6434`. The branch adds only what a wasm component
needs:

- a `headless` feature that compiles `HeadlessAppContext` and `TestPlatform`
  without `test-support`'s native dependencies or `proptest`;
- `WgpuHeadlessRenderer` and its `PlatformHeadlessRenderer` implementation on
  wasm;
- `WgpuContext::from_offscreen(instance, adapter, device, queue)`, which builds
  a context on a device the guest resolved itself;
- wasi-specific cfg splits so `wasm32-wasip2` does not compile the browser
  canvas path or enable wgpu's `webgl`/`webgpu` features. `wasm32-wasip2` is
  wasm-family but has no DOM.

The guest uses released `wgpu` **30.0.1** and
[`fungOS-linux/wgpu-wasi-webgpu`](https://github.com/fungOS-linux/wgpu-wasi-webgpu/tree/wgpu-30)
rev `bec9213`, which implements the 30.0.1 custom-backend dispatch traits over
`wasi:webgpu`.

Do not enable wgpu's `webgl`/`webgpu` features for this target. They add
JS-only dispatch methods the custom backend does not implement, and the guest
fails to link before it ever reaches the host.

## Checklist for a new GPUI Spore

1. **Crate.** `cdylib`, standalone `[workspace]`, `edition = 2024`.
2. **Dependencies.** `gpui` and `gpui_wgpu` from the fork with
   `default-features = false, features = ["headless"]`; `wgpu = "30"` with
   `default-features = false, features = ["wgsl"]`; `wgpu-wasi-webgpu` at the
   pinned rev; `wit-bindgen`.
3. **WIT.** A `runner` world with `include wasi:webgpu/imports@0.3.0-rc.2;` and
   `import soil:present/surface@0.1.0;`, exporting `run: async func() -> u32`.
   Map `"wasi:webgpu/webgpu@0.3.0-rc.2"` to `wgpu_wasi_webgpu::wasi::webgpu::webgpu`.
4. **Fonts.** Embed the faces (`include_bytes!`) and build a
   `CosmicTextSystem::new_without_system_fonts(family)`. There are no system
   fonts and no filesystem in the component. Call `add_fonts` before the first
   frame.
5. **View.** A normal GPUI `Render` implementation. The root is a sized `div`;
   no window APIs, no `cx.spawn` that parks, no `std::thread`.
6. **Present.** Render through `HeadlessAppContext`, capture the frame, upload
   it, and call `surface::present`.
7. **Build.** `cargo build --release --target wasm32-wasip2`.
8. **Check.** Run it under `crates/soil-webgpu` with `--frame` and inspect the
   captured pixels before committing.

## Build and run

```sh
cd spores/terminal-gpui
cargo build --release --target wasm32-wasip2

# The native soil host; provides wasi:webgpu and soil:present.
soil-webgpu --frame /tmp/term.bgr --size 1000x700 \
  target/wasm32-wasip2/release/terminal_gpui.wasm
```

`soil-webgpu` writes BGR24 at the host-selected extent. On a 2x display a
requested `1000x700` produces `2000x1400`, because the guest renders at the
physical size the host reports.

## Constraints worth knowing

- `HeadlessAppContext` runs on `TestDispatcher`, which is single-threaded and
  deterministic. Do not call blocking I/O or park inside a render.
- The component has no filesystem, clock beyond wasi's, or system font access.
- The first frame pays for pipeline creation and the readback; a live host
  should keep the `HeadlessAppContext` alive and re-render on input instead of
  constructing one per frame.
- Today's `soil-webgpu` is a one-shot runner: run, present one frame, exit. A
  live Spore needs a long-lived host with a frame sink and session/input
  imports. That is the next host change.

## Hosts today

- `crates/soil-webgpu` — one-shot CLI presenter used by the fixtures and by
  `--capture` checks.
- `crates/canvas-winit` (soil) — a winit window that presents a GPUI Spore
  frame natively. It is a presenter, not a scene host: the Spore is unchanged.
  One frame per run today; the long-lived loop and input are next.
- Canvas's external-surface path is the compositor-side host; the web bridge is
  still deferred.

## What is deferred

- **Native live host.** `canvas-winit` presents one frame today. It needs a
  long-lived guest loop, `soil:surface` input, and the launcher/catalog before
  it can replace `term.myc`.
- **HTTP/SSE bridge in the host.** Frames out, pointer/key in, carried by the
  same WIT shapes. Deferred, but it belongs in the soil/spore host rather than
  in each page.
- **Browser GPUI.** Transpile this component with `jco` and implement
  `wasi:webgpu` over `navigator.gpu`. This is the true canvas-web path and the
  reason the component is kept free of native assumptions.

## See also

- [`spores/terminal-gpui`](../spores/terminal-gpui) — the reference component
  and its captured frame.
- [`docs/host-contract.md`](host-contract.md) — the scene-spore contract and
  how the GPUI path relates to it.
- [`docs/gpu-surfaces-in-lua-ui.md`](gpu-surfaces-in-lua-ui.md) — the
  `soil-scene`/GPU-surface path for spores that stay in Lua.
