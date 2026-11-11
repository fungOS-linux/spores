# DragShift

Status: design, 2026-11-07. A port of
[llamaboi55/DragShift](https://github.com/llamaboi55/DragShift) (MIT) into a
Spore.

Upstream is MIT, so the port carries the same licence and only needs attribution.
The question is not whether we may port it; it is which half of it can be a Spore.

## What upstream is

A Python/PySide6 application, ~5,600 lines across 17 modules. You start dragging
a file in Finder, press **Shift**, a liquid-glass radial menu opens under the
cursor, and you drop the file on a slice. The result is written **next to the
original**, never over it, and a notification offers **Open**, **Show in folder**
and **Undo**.

Its operation set:

| Source | Convert to | Tools |
|---|---|---|
| Images (JPG/PNG/WEBP/HEIC/AVIF/TIFF/BMP/GIF/ICO) | any of those, or PDF; several images become one PDF | compress (quality or target size), resize, crop, edit, annotate, redact, add background, metadata |
| PDF | JPG/PNG per page, DOCX, TXT | compress, split, rotate, merge, AES-256 protect, extract images, metadata |
| DOCX | PDF, TXT | — |
| Text (.txt/.md/.csv) | PDF, DOCX | — |

Dependencies: Pillow, pillow-heif, PyMuPDF, python-docx, numpy. The trigger uses
pyobjc on macOS, `win32` on Windows, and X11 on Linux — and upstream's own
README says Wayland cannot do it, because Wayland does not let applications read
global mouse and Shift state.

That sentence is the whole design problem. Everything upstream does that makes
it feel good is either a global input monitor or a codec.

## The split

| Concern | Owner | Capability |
|---|---|---|
| Radial menu scene | Spore (Lua) | `soil:canvas` |
| Operation model, parameters, output naming | Spore (wasm) | — |
| Read the source, write next to it | host | `soil:vfs`, `soil:vfs-mut` |
| PNG/JPEG/WebP/GIF/TIFF/BMP/ICO | Spore | pure Rust in wasm |
| HEIC/AVIF decode | host | new codec capability |
| PDF raster (PDF → image) | host | new codec capability |
| DOCX → PDF via Microsoft Word | nobody | impossible: no process spawn |
| Shift + drag trigger | host | host policy |
| Screen capture for glass refraction | host | Screen Recording, opt-in |
| Result notification + Undo | host | `soil:notify`, `soil:appdata` |

### Why the codecs are the hard part

A `wasm32-wasip2` guest can decode PNG, JPEG, WebP, GIF, TIFF, BMP and ICO with
pure-Rust crates. It cannot decode HEIC or AVIF — those need libheif and libavif,
which are C/C++ with their own build requirements — and it cannot rasterise a
PDF, which needs pdfium or an equivalent C++ engine.

Two honest options:

1. **Ship narrow.** v1 supports the formats wasm can decode, and offers PDF
   *output* only rather than rasterising PDF input. This is most of what people
   actually do and it needs no new host capability.
2. **Add a codec capability.** A bounded `soil:image` / `soil:pdf` that decodes
   host-side and returns RGBA or a content id, in the same shape `soil:media`
   already uses for `image.rgba(max-width, max-height)`. This is the right long
   term answer, and `docs/media.md` already sketches the pattern.

Recommendation: narrow first. `soil:media/image` exists and is bounded; extend it
when a second Spore needs the same decoder, not before.

### Why DOCX → PDF is not a port bug

Upstream shells out to Microsoft Word when it is installed and falls back to a
built-in renderer. A Spore has no `soil:process` grant by design, and adding one
so a document converter can launch Word is exactly the trade this system exists
to refuse. Ship the built-in renderer and say so in the UI, the same way the
host refuses an unsupported capability by name rather than degrading silently.

## The radial menu in Canvas

This is the part that most needs to feel good, so it matters that the scene can
draw it.

Canvas exposes `group`, `text`, `rect`, `image` and `mesh`, and every node carries
a `rotation`. There is no polygon primitive, and `mesh` takes a *host-declared
asset* rather than inline geometry, so it is not the answer here.

**The ring is built from rotated rects.** `ui.radial` lays a short run of rects
along each slice's arc, each rotated by a real quaternion about Z. That is
geometry, not a picture of geometry: the slices are drawn by the same renderer as
everything else, they scale with the surface, and each one carries a hit region
the host registers like any other control. `segments` trades node count for
smoothness — Canvas bounds a scene at 1024 nodes and defaults to 256, so a
nine-slice ring at six segments costs 54 rects plus labels and hit regions.

One radius for both axes, deliberately: a rotated rect only stays a rect in a
square frame, so the ring is sized by the shorter axis rather than letting the
surface's aspect ratio shear every slice into a parallelogram.

The interaction is a ring with a second ring for Convert/Tools, toggled by a
tap — the same model upstream uses. That is state, so it lives in the Spore: the
view is a function of the snapshot, and the snapshot carries `ring`, `selected`,
`slices[{id, label, available}]`, and the source summary. A slice this build
cannot honour is drawn, not hidden, and is not pressable — the ring says what it
will not do.

Liquid glass is a *renderer* effect, not a scene one. If we want refraction, it
is a host feature (capture what is behind the surface, blur and tint) applied to
the surface, never something the Lua view fakes. Without the capture permission
the surface is a normal translucent panel, which is a fine default.

## The trigger

A Spore cannot observe a drag that starts in Finder. Three options, in order of
how much authority they need:

1. **A host drop surface.** The host accepts a drop, resolves the dropped items
   to workspace paths, and asks the Spore to present the ring. No global
   monitoring, no new permission. `soil:input` has no drag payload today, so this
   is a small host addition.
2. **A Finder Quick Action** (right-click → DragShift…). Zero monitoring; upstream
   already ships an equivalent through `Open With`. This is the honest v1 on
   macOS.
3. **A global monitor** (CGEventTap reading Shift and mouse state). This is what
   upstream does and it needs Accessibility or Input Monitoring permission. It is
   a host surface with a permission prompt, and it should be explicit and
   revocable, not inferred from "the app has a menu".

Recommendation: (2) for v1, (1) as the real ergonomic answer, (3) only if a
person explicitly wants the exact upstream trigger.

## Grants

```toml
[grants]
"soil:canvas"    = "present the radial menu and the tool surfaces"
"soil:vfs"      = "read the source file"
"soil:vfs-mut"  = "write the result next to it"
"soil:notify"   = "report what changed, offer Undo"
"soil:appdata"  = "settings and undo receipts"
"soil:time"     = "progress animation"
```

No `soil:search`, no `soil:http`, no `soil:inference`. Every tool window is a
Canvas surface with the same extent contract; crop and annotate are the only ones
that need pointer input beyond keys.

Undo is a receipt in `soil:appdata` naming the written path, so it survives a
restart — upstream's Undo moves the new file to Trash, and a durable receipt is
the smallest honest form of that.

## Open questions

- **One Spore or two?** The converter and the tool windows share a source model
  and nothing else. Splitting them would let "convert" ship without the editor
  windows.
- **Does a Spore get a pointer drag?** `soil:input` has `pointer-move`,
  `pointer-button`, `scroll` — but no drag-and-drop payload with file identity.
  If the host owns the drop, the Spore only ever sees resolved paths, which keeps
  it honest.
- **Batch.** Upstream combines several images into one PDF on a single drop. That
  is `soil:vfs` listing plus one output write; make sure the snapshot carries a
  count, not one path at a time.
