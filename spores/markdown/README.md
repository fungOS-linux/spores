# Markdown

A reader and editor for Markdown that proves the Spore split: **Rust owns
document semantics, Lua owns the UI, Canvas renders, Soil launches.**

One document, three hosts, no rebuild:

| Surface | Launched by | Rendered by |
|---|---|---|
| Web | Soil, `web` | Canvas |
| Native | Soil, `native` | Canvas |
| Canvas window | Soil, `canvas` | Canvas |

## The split

```text
spore-markdown (wasm)          ui/editor.lua (JIT)
├── parse Markdown             ├── layout structure
├── edit text, cursor, undo    ├── roles, chrome, spacing
├── DocId through Vfs          ├── how images and diagrams sit in the flow
└── Snapshot ────────────────► └── Canvas calls
```

Rust emits one flat snapshot. Lua decides everything visible. That seam is what
makes live edits possible: change `ui/editor.lua`, Canvas recompiles it, and the
running app restyles without rebuilding the module or reloading the document.

`Ui::digest()` identifies the UI that produced a view, so an agent's live tweak
is attributable and reversible.

## Content support

- Headings, paragraphs, quotes, ordered and unordered lists, rules, fenced code.
- Inline strong, emphasis, code, links, and images.
- Host-rendered diagram fences: `mermaid`, `pele`, `graphviz`, `plantuml`, `math`.
  The Spore ships the source; Canvas draws it.
- Images and attachments by `Target`: `asset:<algo>:<digest>`,
  `attach:<id>`, `doc:<id>`. The Spore never fetches bytes.

## Storage

Documents are opaque `DocId`s through the `Vfs` trait. `MemoryVfs` is the current
implementation; `fungos-vfs` implements the same trait later. The boundary is
already the real one, so nothing above it changes when the store lands.

## Check

```sh
cargo test -p spore-markdown
luac -p ui/editor.lua
lua tests/ui_fixture.lua       # prints "ui: ok"
soil-spore check .             # manifest, declared files, lock freshness
```

`module.wasm` is produced by the Soil guest build, so `soil-spore check` passes
once that artifact exists. Packaging is deterministic:

```sh
soil-spore lock .
soil-spore pack . -o markdown.spore
soil-spore verify markdown.spore
```

## Grants

`soil:canvas`, `soil:time`, `soil:storage`. No transport, no process, no
secrets, no clock, no raw filesystem. The Lua UI has no `os`, `io`, `require`, or
`package`; `tests/ui_contract.rs` enforces that.

License: MIT.
