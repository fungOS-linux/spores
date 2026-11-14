# doc-shell

A native window onto the `fungos-vfs` document layer: the vault on one side, the
document in the middle, a slim view of an agent working on the other. The
arrangement flips.

```
cargo run --example demo_vault -- /tmp/doc-shell-demo   # build a vault to look at
cargo run -- --root /tmp/doc-shell-demo/vault
```

## Building

The Mac is the darwin lane: a native window has to be built here to be looked
at. Everything else — checks, clippy, tests — goes to the fleet, because a GPUI
debug tree is about 5 GB and this disk has filled twice.

```sh
# from the spores root, not from this crate: bob ships the tree it is given, and
# `vendor/` is ignored by this crate's .gitignore but not by the repository's
fpl bob remote --host agora-one --full --back apps/doc-shell/Cargo.toml -- \
  cargo clippy --manifest-path apps/doc-shell/Cargo.toml --all-targets -- -D warnings

# the binary itself, on the Mac
cargo build --manifest-path apps/doc-shell/Cargo.toml
```

Two things about that, both learned the hard way:

- **`--full` is not optional while iterating.** The incremental sync missed a
modified file and the remote happily rebuilt the previous version, which reports
an error you have already fixed. A full upload costs seconds; trusting a stale
build costs a debugging session.
- **Run it from the spores root.** `bob` reads the *project root's* `.gitignore`,
and this crate ignores `vendor/`, so a build started inside `apps/doc-shell`
ships everything except the doc layer and fails to resolve `fungos-vfs`.

## Why it is its own workspace

The Spore guests in this repository build against the headless `gpui` 0.2.2 fork
for `wasm32-wasip2`. This is a native window, built against `gpui-kit`'s pinned
`gpui-pre` snapshot with the desktop platform features. Those are different
crates and their types do not unify, so this crate has its own `[workspace]`,
its own `Cargo.lock`, and its own `target/`. `adapters/delta-acp` in `colony` is
isolated for the same reason.

## The document layer is a dependency, not a copy

`fungos-vfs` is staged into `vendor/fungos-vfs` read-only by
`scripts/vendor-doc-layer.sh`, which copies it from its own checkout on branch
`feat/pi-sessions` and records the revision in `vendor/fungos-vfs/PROVENANCE`.
Nothing in `vendor/` is edited from here; the copy exists so a build on a build
host does not have to reach back to this machine for a path dependency.

Once `feat/pi-sessions` is what `origin/main` carries, the path dependency in
`Cargo.toml` becomes a git dependency on the pinned revision and the script goes
away.

## One layout, not two

`src/layout.rs` holds the whole arrangement: an ordering of three panes and a
width per side pane. Flipping swaps the document and the transcript *positions*;
moving the sidebar sends it to the other end. No pane knows where it is, so there
is no second layout to keep in step.

Keybindings (`ctrl-shift-*` also works where `cmd` does not):

| key | action |
| --- | --- |
| `cmd-shift-p` | flip the document and the transcript |
| `cmd-shift-b` | move the vault to the other end |
| `cmd-shift-e` | markdown source or rendered preview |
| `cmd-s` | write the buffer and reconcile it |
| `cmd-alt-left` / `cmd-alt-right` | previous / next session |

## Views never own the document

The source view holds a buffer and hands it back as text; `DocFile::reconcile`
turns the difference into ops. The preview renders `DocFile::render()`, never the
buffer, so a preview cannot disagree with a save. The transcript pane is written
against the `TranscriptSource` trait, so a live ACP stream can replace the
records without touching the view.

## What this milestone does not do

- No in-block editing (no caret or IME inside a block), and no character-level
  CRDT: two edits to one block resolve last writer wins, and the footer says so.
- No Word/PPTX/XLSX pane. A non-markdown file shows an honest "not modelled yet".
- Threads are read-only.
- Nothing here starts or controls an agent.
