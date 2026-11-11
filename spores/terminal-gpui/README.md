# term.myc — GPUI Spore

The terminal view, rendered by GPUI **inside** a `wasm32-wasip2` component. The
Spore owns the view and the VT grid; the host owns only the WebGPU device and
the compositor surface. No window, swapchain, or native handle crosses the
boundary.

![term.myc rendered by GPUI in a Spore](term.png)

## How it runs

```
host: wasi:webgpu instance + soil:present surface
  └─ guest: request adapter/device
       └─ WgpuContext::from_offscreen
            └─ HeadlessAppContext + TerminalView (GPUI)
                 └─ RGBA frame → host texture → present
```

The guest uses the `headless` feature added to
[`fungOS-linux/gpui-fast`](https://github.com/fungOS-linux/gpui-fast/tree/wasip2)
(`eef6434`). That feature compiles `HeadlessAppContext` and
`WgpuHeadlessRenderer` without the native-only and `proptest` dependencies, and
`WgpuContext::from_offscreen` builds a context on a caller-supplied device.

## Build and run

```sh
cargo build --release --target wasm32-wasip2

# soil's WebGPU runner provides wasi:webgpu and soil:present.
soil-webgpu --frame term.bgr --size 1000x700 \
  target/wasm32-wasip2/release/terminal_gpui.wasm

# canvas-winit provides the live window and keyboard input.
canvas-winit target/wasm32-wasip2/release/terminal_gpui.wasm
```

`soil-webgpu` writes BGR24; the screenshot above is that frame converted to
PNG.

## What is real, and what is not

Real: the GPUI view tree, Lilex text shaping, per-cell ANSI colours, cursor,
font loading from embedded bytes, offscreen render, texture upload, and
`soil:present`. Keyboard input polls the host's normalized key queue; the
footer shows the last key and `escape` exits the loop. `canvas-winit` is the
live presenter; `soil-webgpu --frame` runs the same module as a one-shot
capture.

Not yet: the grid is `terminal-core`'s `ANSI_FIXTURE`; the sidebar targets are
placeholders. The live Mycelium directory, PTY session, scrollback, and resize
are still in `spores/terminal`, which links libghostty-vt and imports
`soil:mycelium`. Moving those into this component is the next step; it needs a
host that provides both `wasi:webgpu`/`soil:present` and the Mycelium session
imports.

## See also

[`docs/gpui-spores.md`](../../docs/gpui-spores.md) is the path every new
GPUI Spore starts from. This crate is its reference implementation.
