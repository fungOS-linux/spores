# xplorer

A file manager as a Spore. Lists one granted workspace, previews what is
selected, and knows nothing else: no filesystem, no network, no process, no
clock.

```sh
cargo build --release --target wasm32-wasip2
# → target/wasm32-wasip2/release/xplorer.wasm  (a component, ~18 MB)
```

## What it reads

One capability, `soil:vfs`, and it is the same call for both halves of the
window:

- `ls(parent, offset, limit)` fills the list, and each row carries the host's
  `hydration` — so a file whose bytes live on another machine says so instead of
  hanging when it is selected;
- `read(path, offset, length)` fills the preview, at a budget (24 KiB) rather
  than the whole object, and the pane says when there is more;
- every entry also carries a `content-id`, which is what a later image or media
  preview will hand to `soil:storage` once that grant is linked.

Nothing here walks a disk, and a path the host does not grant simply does not
exist — which is why this Spore cannot show somebody else's files even by
accident.

## What it draws

GPUI, headless, rendered into the host's surface: list on the left, preview on
the right, path in the header, hints in the footer. Keyboard first — arrows move,
`Enter` descends, `Backspace` goes up, `r` reloads, `q` quits. A pointer is not
hit-tested yet: the reviewable part is the data flow, and a hover that silently
changes the selection would be worse than no hover.

## Why it is not in this repository's workspace

A Spore is a `wasm32-wasip2` component built against the headless `gpui` in
`fungOS-linux/gpui-fast`, with components from `fungOS-linux/gpui-kit`'s
`gpui-component`. The native apps here build against the desktop platform of a
different `gpui`. Different crates, so this crate has its own `[workspace]`,
lockfile and target directory.

## What it still needs before it can be seen

The Spore compiles and is a well-formed component; nothing hosts it yet. A host
must link `soil:present` (surface, input, cursor) **and `soil:vfs`** and then call
`run`.

`crates/soil-vfs-host` in soil already implements `soil:vfs` and `soil:index`
over `fungos-vfs`, and `crates/soil-webgpu` already presents a component's
frames — but no binary composes the two. That runner is the next piece:
`--root <workspace> --frame <file.bgr> --size WxH <component.wasm>`.

## Later

- pointer hit-testing, tabs and split panes;
- `soil:index` behind a command palette (substring → fuzzy → full-text →
  semantic are all the host's already);
- `soil:records` for the desktop tree and for transcripts as first-class rows;
- `soil:storage` for image and media previews by content id;
- `soil:spore.launch` so a document opens in the editor instead of a preview.
