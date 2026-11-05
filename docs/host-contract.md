# Spore host contract

Three parties, three owners. A Spore that violates this contract is a bug even
if it looks fine on one surface.

| Party | Owns | Never owns |
|---|---|---|
| Spore module (Rust/WASM) | document semantics, editor state, revisioned snapshot | layout, colour, typography, pixels, input devices |
| Spore UI (Lua) | every presentation decision: structure, roles, chrome, spacing | document truth, capability grants, transport |
| Canvas | rendering on web, native, and in-window; layout; input normalization; asset resolution | document semantics |
| Soil | which host to launch, grant enforcement, module lifecycle | view content |

**Rule:** one scene pipeline. The Spore does not ship an HTML renderer, a
terminal renderer, or a Canvas renderer. Canvas renders all three surfaces from
the same Lua-produced scene. A Spore that branches on host identity will
eventually behave differently on one of them.

## Launch

`spore.toml` declares `[launch].surfaces` and a `default`. Soil picks one:

- `web` — Canvas in a browser (`canvas-web`)
- `native` — Canvas on a Wayland/DRM surface (`soil-runtime`)
- `canvas` — a granted owner surface inside a running Canvas workspace

The module and the UI are identical in all three cases. Only the presenter and
the available input devices differ.

## The snapshot

Rust hands Lua one flat table. No colours, no coordinates, no widget names.

```lua
{
  revision = 7, mode = "edit", dirty = true, line_count = 14, scroll = 0,
  cursor = { line = 3, column = 4 },
  selection = { { line = 3, column = 0 }, { line = 3, column = 4 } } or nil,
  blocks = {
    { index = 0, line = 0, kind = "heading", level = 1,
      ordered = false, item = 0, language = nil,
      text = "Field notes",
      spans = { { style = "text", text = "Field notes" } } },
  },
  assets = {
    { kind = "image", target = "asset:sha256:...", line = 8, label = "map" },
  },
}
```

`kind`: `heading`, `paragraph`, `quote`, `list_item`, `code`, `diagram`, `rule`,
`spacer`.

`style`: `text`, `code`, `strong`, `emphasis`, `strong_emphasis`, `link`,
`image`.

The snapshot is deliberately flat and string-keyed so a host can materialize it
without a binding generator, and so the Lua UI is readable by a person and by an
agent.

## The UI

```lua
local M = {}

function M.render(snapshot)
  return {
    revision = snapshot.revision,
    ui = "markdown@0.1.0",
    nodes = {
      { id = "b0", kind = "text", role = "primary", level = 1,
        text = "Field notes", line = 0 },
      { id = "caret", kind = "caret", role = "primary", line = 3, column = 4 },
    },
  }
end

return M
```

- `role` is a Coherence role (`text`, `muted`, `primary`, `success`, `warning`,
  `danger`), never RGB.
- The UI returns a content tree. Canvas assigns coordinates and renders.
- Determinism: same snapshot in, same scene out. No clock, no randomness, no
  I/O. Canvas runs Lua only when it supplies input or a new snapshot.
- `kind` is a Canvas primitive hint (`text`, `code`, `image`, `diagram`, `rule`,
  `space`, `row`, `caret`); Canvas may ignore it and use `role` and `text`.

## Hot reload

`[ui].hot_reload = true` and `Ui::digest()` let Canvas:

1. detect a changed UI file,
2. recompile it (JIT) without restarting the module,
3. keep the document and editor state intact,
4. record `ui@version` and the digest on the resulting view.

This is the intended way to change an app's look, and the intended way for an
agent to make a live tweak while a person is using the app. A failed recompile
keeps the last good UI and reports a named error; it never blanks the view.

## Grants

The module's grants are in `spore.toml` and enforced by Soil. The UI inherits
none of them: it cannot read storage, resolve an asset, open a socket, or see
the clock. It receives a snapshot and returns nodes.

Assets are resolved by the host. The Spore records `Target`s; `Target::spore_may_fetch`
is always false.

## Not yet implemented

- `soil:vfs` (the `Vfs` trait is real, `MemoryVfs` is the only implementation).
- Canvas's spore scene primitive and the Lua host bindings.
- Soil's surface selection at launch.
- Signed manifests and the Spore Store.

When those land, this contract should not need to change.
