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

## Comments, pending

`tab` moves the keyboard between the vault and the document; in the document
`j`/`k` move the block cursor, `c` (or `enter`) starts a comment on that block,
`enter` leaves it, `x` resolves the thread. Left of each block is the identity
the container gave it; right of it is the thread still pending on it. A resolved
thread stays visible and dims, because a reply to a resolved thread still has to
render.

Comments are entities, so they live in the host's records book —
`comment.pending`, one record per comment, body `doc.comment@1`. Two consequences
worth naming:

- **the store does not depend on the target.** A note on a document and a note on
  a transcript both live in the book; only the *destination* of a commit differs.
  Picking a store per target was the messy part of the earlier proposal.
- **ordering is the host's revision, not a clock.** `soil:time` is monotonic per
  process and explicitly not comparable across restarts or devices, so it cannot
  order two people's comments. The book can.

The harness build seeds three comments (one resolved, one reply) because a host
that has not linked `soil:records` cannot instantiate the component at all.

## Grants

| grant | why |
| --- | --- |
| `soil:vfs` | list one workspace, read the bytes of the document it opens |
| `soil:records` | read and write the pending comments left on a document's blocks |
| `soil:principal` | the commenting identity, attested by the host rather than typed |
| `soil:present` | the offscreen surface, keyboard input, the cursor |
| `wasi:webgpu` | one offscreen device; the host keeps the adapter and the swap |

Saving a document, and accepting a comment into its text, are mutations against
the namespace: they arrive as `soil:vfs-mut` with their own review. See
[What is not here yet](#what-is-not-here-yet).

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

## Which part of the kit

`gpui-base` and `gpui-component` — the kit's two Spore-facing layers — for the
style extensions (`font_semibold`, the text scale), the `StatusBar`, the theme
and its tokens. Not the `gpui-kit` umbrella: the fork carries exactly four crates
for wasm and excludes the umbrella, because it pulls the desktop and browser
platforms in with it. Not `gpui-kit-assets` either — on `target_family = "wasm"`
its asset source fetches icons over HTTP, and a Spore has no network — so the two
icons in this window (a folder and a page) are drawn from rectangles.

## What is not here yet

- **Saving.** `cmd-s` needs `soil:vfs-mut`; the record of *who* saved comes from
  `soil:principal` the host already attests.
- **Threads that leave the reader.** A reply is anchored to a block id, and a
  thread has three fates:
  it is *pending* (live, syncing, attached), it is *committed* into the durable
  artifact, or it is gone. "Render the same" is the requirement that decides the
  shape: a committed comment has to keep its provenance — comment id, author,
  time, the block it was on — or the view cannot draw it like the ones that are
  still pending, and nobody can tell it began life as a comment.

  Where pending threads live follows where the artifact lives, so a commit can
  be one transaction rather than two:

  | target | pending in | commit |
  | --- | --- | --- |
  | transcript (a records book) | `soil:records`, one record per comment | append the durable entry and drop the pending record, in one store |
  | document or sheet (bytes in the namespace) | `soil:vfs-mut`, per-actor files | not a commit: resolving is the end of a comment |

  A doc comment is *accepted into the text* on save, which is why it has no
  agent-facing commit step: nobody reads a spreadsheet as instructions. A
  transcript comment is the opposite — committing is what puts it where an agent
  will read it, so it must never be lost and never be ordered after the reply it
  answers.

  This slice implements only the first: pending, resolved, and rendered from the
  book. The model behind the rest —
  [comments and attachments](../../../docs/comment-attachments.md) — settles the
  rest: commit is a projection into content at a stable boundary (so a prompt
  prefix, and its K/V cache, survives), the record stays as the pointer the
  reader draws in place, and release is a compaction decision that may never
  come.
- **The transcript pane** and **preferences**: layout is per-device view state,
  which is what `soil:appdata`'s `local` scope is for.
