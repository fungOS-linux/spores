# lights-view

A room-first Philips Hue controller rendered by GPUI inside a `wasm32-wasip2`
Spore. The Spore owns the pixels and the Hue semantics; Soil owns the window,
the pointer stream, the origin allowlist, and the bridge key.

![lights-view on a desktop](assets/lights-view.png)

![lights-view on a watch-sized surface](assets/lights-view-watch.png)

## What it shows

- **Rooms** from the bridge's groups, with the bridge's all-lights group first.
- **Scenes** scoped to the selected room; never a scene from another room.
- **Lights** as cards with power, a brightness meter, and 25/50/100 presets.
- **Watch mode** below 560 px tall: room chips, a group power switch, and one
  toggle row per visible light.

When a request fails, the last snapshot stays on screen and the status line
reports the bridge error. The view never fabricates an off state or a zero
brightness for a light the bridge did not report.

## Run

Build and launch through the real Soil presenter, not a browser wrapper:

```sh
cargo build --release --target wasm32-wasip2
cp target/wasm32-wasip2/release/lights_view.wasm module.wasm
soil-spore lock . && soil-spore check .

canvas-winit module.wasm \
  --hue-bridge http://192.168.10.15 \
  --geode-socket ~/.geode/geode.sock \
  --geode-purpose hue.control
```

`--hue-bridge` binds the `hue.local` capability to the bridge. Moving the
bridge does not require rebuilding the Spore. The guest names
`geode://home/hue/bridge`; the host leases that entry and substitutes it into
the Hue v1 path. The key never enters Wasm, `spore.toml`, a record, or argv.
When this Spore is opened from the Spore launcher, start the launcher with the
same `--hue-bridge` and Geode flags so the child inherits the grant.

## Input

- click a room, scene, light, switch, or preset
- `up` / `down`: previous/next room
- `space`: toggle the selected room
- `r`: refresh the bridge snapshot
- `escape`: quit

## GPUI Kit

The view pulls `gpui-base` and `gpui-component` from the
[`fungOS-linux/gpui-kit`](https://github.com/fungOS-linux/gpui-kit) fork at
revision `8feed9ee6526d0ee8e7b3e11349290b3d38319ed` on the `fungos/gpui-fast`
branch. That branch carries the four Spore-facing crates and
pins GPUI to the same `fungos/gpui-fast` revision the rest of the wasip2 renderer
uses. The Spore initializes the dark theme, maps the kit's warning/primary
tokens to the amber light accent, and embeds the eight Lucide icons it uses as
an `AssetSource` so nothing reaches for a browser CDN at runtime. Those icons
carry Lucide's ISC license, kept beside them in `assets/icons`.

## Not yet

The kit gives us consistent buttons, switches, focus structure, and theme
tokens. The remaining UI work is mostly outside the component library:

- route host pointer/key events into GPUI so hover, press, focus rings, and
  slider drags animate inside the components; today the Spore hit-tests the
  normalized events itself
- a color/temperature picker and a real brightness slider (Hue `xy`/`ct`)
- a scroll/clip viewport for long rooms and short compact screens
- pairing, bridge discovery, and room/scene editing sheets
- a11y tree publication for the raw hit region, plus a second font weight for
  hierarchy
- a non-blocking request state so a slow bridge shows a skeleton instead of
  freezing the frame

## Build

```sh
TMPDIR=/tmp CARGO_TARGET_DIR=/tmp/lights-target \
  cargo build --release --target wasm32-wasip2
```

GPUI is pinned to `fungOS-linux/gpui-fast` revision
`60ebb175729ad2a17acf59d8e988e794ff4b7eb4`; the wgpu-wasi-webgpu pin keeps the
guest on wgpu 30 while the host remains on wgpu-core 29. `gpui-component` costs
about 8.9 MB of module size (9.5 MB before, 18.1 MB after), because the crate is
a whole desktop surface — markdown, text editing, localization, and the icon
catalog — not just the buttons and switches this view calls.
