# UI model

Status: adopted, partly implemented. `sdk/ui/ui.lua` is the canonical library;
`spores/gallery` is the fixture-driven component catalog. The older per-Spore
copies in `markdown` and `unibus` are migration debt, not the source of truth.

This reconciles a declarative-UI proposal with the constraints the system
already has: Canvas owns layout and pixels, `soil:canvas` is the guest ABI, a
Spore is a signed wasm module, and a view has no authority.

## The shape

```
snapshot (from the Spore, wasm)     actions (intents, back to the Spore)
            │                                   ▲
            ▼                                   │
      view: Lua → ui.* tree ──measure/layout──┬─► canvas.rect/text (legacy)
                                             │
                                             └─► ui.document ──► soil-scene
                                                                  │
                                                                  ▼
                                                    canvas.present / host renderer
```

- **Lua describes.** A view builds a tree and presents it (`ui.present`): no
  coordinates, no colours. The layout belongs to the library — host Taffy when
  there is one, the kit's own pass when there is not — so the same view draws the
  same geometry on Canvas, in `tend`, and in a scene recorder.
- **The library measures.** `ui.column/row/card/text/code/...` compute flow from
  theme tokens and call the existing primitives. Adopting a declarative API cost
  no host change.
- **Canvas renders.** The host ABI stays `soil:canvas`; the library is a
  convenience layer over it, not a second renderer.

There are two layout paths, and they answer different questions. `canvas.layout`
is the host path: it solves a flex tree through `soil-layout` (taffy) with measured
text, and `unibus` uses it for its inbox. `sdk/ui/ui.lua` is the portable Lua path:
one pass, no host measurement, so the same tree previews and tests without a
compositor. The dashboard nodes in this document use the portable path; a Spore
that needs measured flex layout can call `canvas.layout` directly.

Dense time series are bounded before they become scene nodes. `ui.lttb` keeps the
endpoints and the dominant point in each bucket; `ui.chart` applies it to series
that exceed the chart's mark count, and `ui.chart_window` keeps a streaming
window without mutating the snapshot. This is the portable counterpart of the
native host's resident dense-buffer decimation.

`ui.graph` is the same rule for relationships. Positions are data in 0..1, so a
producer — site rings, force, Barnes-Hut, a GPU layout — owns the shape and the
library owns the marks. Edges become rotated rects, one node each, which the
current rect/text ABI already draws; a `resource` prop places a host surface
instead, so a native GraphGPU producer can take over without changing the view.

## What we adopted from the proposal

| Proposal | Here |
|---|---|
| Declarative `ui.*` tree | `ui.column`, `ui.row`, `ui.card`, `ui.stack`, `ui.text`, `ui.code`, `ui.diagram`, `ui.bullets`, `ui.quote`, `ui.rule`, `ui.spacer`, `ui.stat`, `ui.gauge`, `ui.table`, `ui.chart`, `ui.graph`, `ui.host_surface` |
| Named tokens over raw values | `gap = "sm"`, `style = "title"`, `ink = "muted"`; sizes come from `module.theme` |
| Layout primitives instead of coordinates | column/row/card flow, measured in the library |
| Incomplete UI must not blank | already true: a rejected reload keeps the last good scene on screen |
| Escape hatch stays possible | call `canvas.*` directly, or `raw = true` in a node |

## Two rendering paths, one vocabulary

There are two wire paths from the same tree, and they have the same visual
meaning:

- `ui.render(tree, theme)` emits the legacy `canvas.rect`/`canvas.text`
  primitives directly. It is the compatibility path for a host that predates
  `soil-scene`.
- `ui.present(tree, theme)` builds a `soil_scene::Present` document, calls the
  host's `canvas.present` (or `soil.canvas.present`), and lets the host draw
  it. This is the Lua peer of `soil_sdk_canvas::Surface::present_scene` in the
  Rust SDK: same schema, same 8-unit transform space, same semantic inks, same
  no-pixels-to-the-view rule.
- `ui.document(tree, theme)` returns the document without calling a host. It is
  the pure artifact a test, a non-Canvas host, or a relay can inspect.

The scene vocabulary is deliberately small: flat `rect` and `text` nodes plus
the layout and dashboard primitives that lower to them. Buttons and host
surfaces are host-owned controls. They stay outside the document; a button is
still a hit region, and a host surface is still a placement, not pixels in the
scene tree.

`ui.present` falls back to `ui.render` when the host does not grant
`canvas.present`, so a running old Canvas keeps drawing. The fallback is a
migration bridge, not a second renderer: both paths are measured by the same
one-pass layout and carry the same tokens.

## What we changed, and why

**State does not live in the view.** The proposal keeps `local state = {count=0}`
and mutates it in `on_click`. Canvas runs a *fresh, sandboxed VM per render*;
state kept there would vanish on the next frame, and a view that can mutate
would be a second authority beside the Spore. So:

```lua
-- view: pure
ui.button("Save", { variant = "primary", on_click = ui.action("save") })

-- Spore (wasm): receives the action, decides, emits a new snapshot
```

The general rule is unchanged: **a view is a function of the snapshot.** Actions
are intents, not callbacks.

**Lua does not call Rust directly.** The proposal exposes `rust.save()` to
views. In this system the Spore *is* the wasm module; it holds state and
capabilities. A view that can call the host has become a Spore without a
manifest. Components inherit the caller's render context and nothing else.

**Memoisation is host work, not Lua work.** "Settled nodes stay stable" cannot
live in the sandbox: the VM is recreated per render, so there is nothing to keep
between frames. It has to be a host-side merge keyed by node `id` — which the
scene already carries. The Lua layer stays pure and cheap; the host does the
diff.

**No second renderer.** Canvas already composites through wgpu (iced + `fpl-gfx`),
and `soil:canvas-gpu` is the path for a Spore that owns its pixels. Adding a
bespoke WGPU renderer beside that would give the system two geometry owners.

**Components cross Spores.** The proposal has no notion of sharing a chrome or a
list between applications. `docs/components.md` does: CID-locked component sets
in `spore.lock`, `components.require`, no authority. Until the host resolves
components, `tools/compose.lua` pre-composes library + view into the one file the
module host reads — the same artifact, produced early.

## Host work this implies

1. **`components.require`** — resolve the locked set, compile each in the same
   sandbox, expose by name. Removes `tools/compose.lua`.
2. **Action intents** — `ui.action(name, payload)` must reach the Spore as an
   input, not be dropped by the scene validator.
3. **Id-keyed scene merge** — keep nodes whose id and description are unchanged
   between renders; replace only dirty ones. Turns "rebuild the whole page" into
   "patch the page", and makes streaming cheap.
4. **Partial source tolerance** — a truncated `ui.column {` should render what is
   complete, not keep the whole previous page. The last-good-scene guard is the
   fallback, not the feature.
5. **Text metrics** — Canvas exposes `canvas.measure_text`, but the portable
   library still estimates a character width to sequence rows and right-align
   labels. Adopting measured widths in the one-pass layout is a separate change;
   it would improve long labels without changing the tree shape.

## Still open

- Whether `ui.*` ships as a component (locked per Spore) or as part of the host's
  Lua prelude. Component is more honest: it can be themed, forked and versioned
  like anything else.
- Whether views may keep *ephemeral* state (hover, scroll) in a host-provided
  table keyed by node id, since the sandbox cannot.
- Whether the tree is ever sent over the wire as data (Streamdown-style) instead
  of being evaluated from Lua. Evaluating Lua is the current answer: the source
  is the stream.

## The browser is another Canvas backend

The renderer decision is settled: **there is one renderer, and it is iced 0.14.**
Native Canvas already draws through it; the browser compiles the same host to
wasm with the `webgl`/`wgpu` renderer and draws the same view. The browser is a
fourth backend beside mac, linux and tv, not a separate product.

The first slice is live in `canvas-web`: the same iced application builds
for `wasm32-unknown-unknown` with the `webgl`/wgpu renderer and draws the home
Spore's scene document. There is no screenshot bridge in that path. A pointer
layer hit-tests the document's host-owned control rectangles; pressing one
emits a typed `canvas-intent` to the page, it does not open anything itself.
The scene fixture is generated from `spores/home/ui/home.lua` by
`spores/home/tools/dump_scene.lua`, so the browser is drawing the Spore's own
layout, not a hand-built approximation. Replacing that fixture with a document
produced by a Luau-in-wasm runtime is the next slice; the renderer does not
change.

That rules out three things that were considered and rejected:

- **No DOM renderer.** Mapping scene nodes to elements would have made the web
  a second implementation with its own layout, focus and text behaviour. It also
  contradicts the paragraph above about geometry owners.
- **No scene over the wire.** `ui.present` hands a document to a host; a browser
  host is that host. Serialising the document to a second renderer adds a
  protocol where there was a function call.
- **No PNG mirror.** The old `canvas-web` polled `/v1/frame` every 125 ms and
  painted a display capture. It is gone, along with `direct.rs` and the
  `canvas-web-host` bridge: nothing in that path was clickable, scalable or
  attributable to a surface, and `screenshot` ignored `surface_id` (every
  surface returned the same image).

### Lua in the browser

Canvas runs views on `mlua` with the **Luau** backend. Luau is C++, and
`wasm32-unknown-unknown` is not one of its targets, so the browser needs Luau
compiled to wasm through `wasi-sdk`/emscripten and loaded as a wasm runtime.

The accepted consequence: **a browser cannot compile wasm at runtime**, so the
web loses live Lua editing and hot reload. A module is composed and its runtime
shipped once; changing a view means rebuilding and reloading the page. Native
keeps the hot loop. That is a fine trade for a stable surface.

### What the browser loses, stated plainly

- **Accessibility.** iced 0.14 exposes no accessibility tree. Text selection and
  copy are weaker than the DOM. Acceptable for a launcher, store, terminal and
  charts; not acceptable for prose.
- **Documents.** `wiki` and `markdown` stay host pages on purpose. A canvas is
  not a document, and pretending otherwise produces a worse wiki.
- **Find-in-page, IME quirks, browser extensions.** Winit supplies a hidden input
  for IME; that is the only DOM the UI uses, and it is not part of the view.

### The node service

A node that runs Spores can optionally publish them over HTTP:

```text
GET /spore/<id>/            the wasm host + composed module
GET /healthz                is this node serving surfaces
```

`soil:spore` knows which Spores are installed; the surface server knows which
are running and under which surface id. The two are different facts and must
not be conflated: installed is not running.

Mycelium already proves the selection mechanism — `service_kind` plus
`services.candidates(kind)` prefers loopback, then the mesh, then configured
fallbacks. A `spore-surface` kind advertises the server; a node not running the
Spore is simply not a candidate. That is the anycast: same name, whichever node
actually has the surface.

### Work, by repo

| Repo | Change |
|---|---|
| `canvas` | **done:** `direct.rs`/frame mirror deleted; iced app target for wasm renders the home scene; remaining: serve assets and modules from a node |
| `canvas-native` | isolate the Lua host behind a runtime trait so the wasm build can use the Luau-to-wasm VM |
| `luau` build | compile Luau for wasm (`wasi-sdk`), ship it as a wasm module the host instantiates |
| `soil` | a node surface server (grow `soil-home`'s binary or add `soil-surface`) |
| `mycelium` | `spore-surface` service kind; route selection by kind |
| `spores` | keep `[launch] canvas_module` truthful; compose on install, not by hand |
