# doc-shell, as a Spore

The first slice of `apps/doc-shell` on the Spore side of the boundary: a vault on
the left, a Word file on the right, and every paragraph shown by the identity
Word already wrote into it. It is a `wasm32-wasip2` component that draws its own
window, built against the fork's headless `gpui` — the same host loop the finder
uses.

![the reader](frame.png)

That frame is the **`demo`** build, captured by `soil-webgpu`, because the host
that presents Spores does not link `soil:vfs` yet. The document in it is a real
`.docx`: `src/data.rs` writes the zip in memory, so the frame exercises the
shipped reader rather than a stub.

## One reader, no database

A `.docx` is a zip whose `word/document.xml` is the outline. Two things make it a
document rather than a blob: paragraphs carry an identity (`w14:paraId`, a 32-bit
hex number Word maintains) and a style name. So:

- **the guest parses it**, from bytes `soil:vfs.read` hands over;
- **the identity is the container's**, so two readers of the same bytes agree
  without a store between them — which is what makes an anchor ("this reply is on
  `1A2B3C4D`") resolvable by anyone holding the file;
- a paragraph Word did not name gets `#n · position`, rendered in the warning
  colour, because an identity derived from position is not the same claim as one
  Word wrote.

`fungos-vfs` reads the same container in the native app; that reader is the one a
host capability would own if we ever want the parse in one place. Nothing in this
slice needs it.

## Reading, and only reading

| grant | why |
| --- | --- |
| `soil:vfs` | list one workspace, read the bytes of the document it opens |
| `soil:present` | the offscreen surface, keyboard input, the cursor |
| `wasi:webgpu` | one offscreen device; the host keeps the adapter and the swap |

Saving a document and writing a thread anchor are mutations, and this slice
declares neither. See [What is not here yet](#what-is-not-here-yet).

## Build

```sh
# the package: module.wasm + spore.lock (+ a tar with --tar)
./scripts/package.sh --tar

# look at the arrangement without a host that links soil:vfs
cargo build --release --target wasm32-wasip2 --features demo
soil-webgpu --frame /tmp/doc.bgr --size 1280x800 \
  target/wasm32-wasip2/release/doc_shell_guest.wasm
```

A Spore gets no window and no device handle it can keep: it renders offscreen and
hands the host a texture. `soil:present/input` is the only keyboard.

## What is not here yet

- **Saving.** `cmd-s` needs `soil:vfs-mut`; the record of *who* saved comes from
  `soil:principal` the host already attests.
- **Threads.** A reply is anchored to a block id. Whether that lives in
  `soil:records` (one record per comment, so an append cannot clobber another
  writer) or in `soil:vfs-mut` (the per-actor JSON files `fungos-vfs` writes
  today) is the next interface decision, deliberately not made by this slice.
- **The transcript pane** and **preferences**: layout is per-device view state,
  which is what `soil:appdata`'s `local` scope is for.
