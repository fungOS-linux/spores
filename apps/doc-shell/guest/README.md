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

Two more frames sit beside it, each captured from a demo build seeded into the
state it shows — the shipped build opens dark, on the reader, with no overlay:

| frame | the state it was seeded into |
| --- | --- |
| [frame-help.png](frame-help.png) | `?` — the keymap, over the reader |
| [frame-light.png](frame-light.png) | `d` — the same window in light mode |

The harness build has no pointer because the real one has none: `soil:present`
delivers keys, and a Spore cannot be tested against affordances its input path
does not carry. So the demo vault holds a directory, two Word files (one long
enough to scroll) and a Markdown file the reader refuses, and its comments are
seeded on one document only — opening the other has to look like a file with no
comments, not a copy of the first one's.

## One reader, no database

A `.docx` is a zip whose `word/document.xml` is the outline. Two things make it a
document rather than a blob: paragraphs carry an identity (`w14:paraId`, a 32-bit
hex number Word maintains) and a style name. So:

- **the guest parses it**, from bytes `soil:vfs.read` hands over;
- **the identity is the container's**, so two readers of the same bytes agree
  without a store between them — which is what makes an anchor ("this reply is on
  `1A2B3C4D`") resolvable by anyone holding the file;
- a paragraph Word did not name gets `#n` and the word `position` in the margin,
  marked by a dot rather than by colour alone, because an identity derived from
  position is not the same claim as one Word wrote.

`fungos-vfs` reads the same container in the native app; that reader is the one a
host capability would own if we ever want the parse in one place. Nothing in this
slice needs it.

## Comments, pending

`tab` moves the keyboard between the vault and the document; in the document
`j`/`k` move the block cursor, `c` (or `enter`) starts a comment on that block,
`enter` leaves it, `x` resolves the thread, `n`/`p` jump to the next and
previous thread. Left of each block is the identity the container gave it; the
rail on the right holds the pending threads in the document's own order.

A thread is the unit the reader resolves, and a resolved thread stays and dims,
because a reply to a resolved thread still has to render. The rail is its own
column and its own scroll: a thread taller than its paragraph must not push the
next paragraph down, or a document stops reading as a document.

Comments are entities, so they live in the host's records book —
`comment.pending`, one record per comment, body `doc.comment@1`. Two consequences
worth naming:

- **the store does not depend on the target.** A note on a document and a note on
  a transcript both live in the book; only the *destination* of a commit differs.
  Picking a store per target was the messy part of the earlier proposal.
- **ordering is the host's revision, not a clock.** `soil:time` is monotonic per
  process and explicitly not comparable across restarts or devices, so it cannot
  order two people's comments. The book can.

The harness build seeds three threads — one open and answered, one open and
alone, one resolved with a reply under it — because a host that has not linked
`soil:records` cannot instantiate the component at all.

## Keys

There is no pointer, so everything the window offers is a key, and the window
says so where the eye goes: the toolbar names `c`, `x` and `?`, the status bar
names `d` and `q`, and `?` names all of them.

| key | what it does |
| --- | --- |
| `tab` | the keyboard moves between the vault and the document |
| `j` `k` (or the arrows) | move: a row, a block, or a heading while the outline is open |
| `enter` | open a directory, comment on the block, jump to a heading |
| `c` | comment on the block under the cursor |
| `x` | resolve the block's thread, or reopen it |
| `n` `p` | the next and previous thread |
| `o` | the outline: the document read as headings, which `j`/`k` then jump between |
| `/` | filter the vault as you type; `esc` clears it |
| `r` | re-read the vault and the open document |
| `d` | light or dark |
| `esc` | close whatever is open |
| `q` | quit |

Nothing that is not a key looks like one: there are no buttons, no scrollbars to
drag, and no hover. The document keeps the cursor's block in view and the rail
keeps the cursor's thread in view, which is the only scrolling a window with no
pointer can honestly do.

A block the container did not name is marked by a dot and by the word
`position` rather than by colour alone, and the pane's header says how many of
the document's identities came from Word and how many did not. A pane with
nothing to read — an empty directory, a file that is not Word, a parse that
failed — says what is in the way and which key gets out of it.

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
its asset source fetches icons over HTTP, and a Spore has no network — so the
vault's folder is drawn from two rectangles, a Word file is marked with ¶ (the
paragraph mark the native shell also uses), and every other mark in the window is
a glyph Lilex actually has — · ¶ — → ↳ ✓ — checked against the font rather than
assumed, because a missing glyph renders as a box.

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
