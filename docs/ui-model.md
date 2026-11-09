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

- **Lua describes.** A view returns a tree: no coordinates, no colours.
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
