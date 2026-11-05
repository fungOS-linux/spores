# Rich content: diagrams and images

Status: design. Two questions, one answer shape: the pane never renders bytes
it did not receive, and the host never learns what a diagram means.

## What the scene can draw today

`soil:canvas` / Canvas scenes expose `group`, `text`, `rect`, `image`, `mesh`.
There is **no vector path primitive**, so neither mermaid nor pele can be drawn
as primitives by a view. Anything richer must arrive as an image, or the host
must grow a path node.

## Diagrams: render to an asset, present as an image

```text
source (.md fence)
      │  ui.render → node kind "diagram"
      ▼
host: diagram resolver  ── mermaid/pele → SVG → raster ──► CID
      │
      ▼
scene: NodeContent::Image { asset }   (or a placeholder until resolved)
```

Rules:

1. **The view ships source, never pixels.** `ui.diagram(format, source)` becomes a
   request: `{ format, source, line }`. Same as images today.
2. **One resolver per format, host-side.** Mermaid and pele are two renderers
   behind one interface: `render(format, source) -> bytes + media type`. They may
   be external binaries, a service, or wasm; the pane does not care.
3. **Content-addressed.** The result is keyed by `sha256(format + source)`, so a
   scroll or a theme change never re-renders, and two panes sharing a diagram
   share the asset.
4. **Failure is a named placeholder.** "mermaid failed: parse error at line 3" in
   the tile, never an empty box. This is the same rule the host already applies
   to a rejected module reload.
5. **Theme is applied at render time, not baked.** Pass the palette the resolver
   should use; a theme change invalidates the cache key.

If instead the host grows a vector path node, mermaid could be compiled to paths
once and recoloured by the theme. That is the better end state and a much larger
change; the asset path works today.

## Images and attachments: resolve through `soil:vfs`

`NodeContent::Image { asset }` already exists and takes a sha256 `AssetRef`.
What is missing is the bridge from what a document *names* to an asset the host
can read.

```text
markdown:  ![site map](attach:site-map)   or   (asset:sha256:…)
   │
   ▼  pane emits Image { target }
host: soil:vfs resolve(target) → bytes → AssetRef { sha256, media_type, len }
   │
   ▼
scene: NodeContent::Image { asset }
```

Rules:

1. **Targets, not paths.** `attach:<id>` is a name in the document; `asset:<algo>:<digest>`
   is already content-addressed; `doc:<id>` refers to another document. A window
   can never be steered at a file.
2. **The VFS owns bytes; the scene owns identity.** Canvas keeps the `AssetRef`
   and the frame cache; it does not learn a directory layout.
3. **Missing is a placeholder with the target in it.** "attachment site-map not
   found" beats a broken-image glyph.
4. **Grants still apply.** Reading an attachment is a `soil:vfs.read` capability
   on that document, not a filesystem permission.
5. **Cache by digest, expire by revocation.** An asset cache key must include the
   grant, so revoking access cannot leave a decoded frame on screen.

## What this implies

| Piece | Owner | State |
|---|---|---|
| `Image` scene node | Canvas | exists |
| Asset resolution from a target | Canvas + `soil:vfs` | missing |
| Diagram fence as a request | view library | source ships; host drops it |
| Diagram resolvers (mermaid, pele) | host | missing |
| Cache keyed by format+source+theme | host | missing |

The smallest useful slice: **VFS image resolution**, because diagrams can then be
implemented as "resolve a generated asset" using the same path.
