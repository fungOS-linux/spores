# elmers-ui

Two read-only views of Elmers, drawn by GPUI inside a Spore: **runs** and
**brain**.

```
┌ Elmers  [runs 1] [brain 2]            12 ok  1 error  0 other ┐
│ ● notifications/pr            19:42  │  notifications/pr     ok  19:42
│ ● notifications/marbles       19:41  │  {"message_id": 7605}
│ ● dev/pr-close-marble         19:40  │
│                                      │
└ j/k move   r refresh   q quit     12 runs · refreshed 2s ago ┘
```

## The brain view

`GET /graph` is the same document the dashboard's d3-force brain draws: every
workflow, the function that owns it, and what it calls. Here it is laid out as
one column per function — deterministic, no simulation, so the picture only
changes when the graph does — and selecting a workflow (j/k) lights up the
workflows it calls, wherever they sit.

Edges are not drawn as lines: the GPUI fork's headless renderer has no rotate or
path primitive yet, so a graph is expressed by grouping and emphasis rather than
by a line per link. When the fork grows a line, the layout does not change.

One thing the picture does show: an owner column is whatever `/graph` calls an
owner, verbatim. A workflow whose frontmatter annotation was parsed with the
surrounding prose shows up as its own column of one, which is how
`lib/exec/serves` (owners `['`', 'is', 'deliberately', …]`) was found.

## Scope

Read-only, deliberately. The Spore asks the sidecar one question — `GET /runs`
— and draws the answer. It does not start, stop, edit, or inspect a workflow's
source, and it holds no Lua: a UI that can run things is a different grant and a
different review.

| It can | It cannot |
|---|---|
| list runs, status, time, result/error | start or stop a run |
| show one run's payload | edit or read workflow files |
| draw the workflow graph | mutate it |
| refresh every 5 s, or on `r` | reach any origin but the sidecar |

## Contract

The GPUI path from [`docs/gpui-spores.md`](../../docs/gpui-spores.md): the host
owns the WebGPU device and the compositor surface; the guest renders offscreen
and hands over a texture. Two imports beyond the presenter:

- `soil:present` — `surface`, `input`, `cursor`
- `soil:http` — one bounded, allowlisted `GET`

The sidecar URL is a constant in [`src/lib.rs`](../../spores/elmers-ui/src/lib.rs)
(`RUNS_URL`), because a Spore names its origin rather than accepting one.

## Build

```sh
cd spores/elmers-ui
cargo build --release --target wasm32-wasip2
cp target/wasm32-wasip2/release/spore_elmers_ui.wasm module.wasm
```

## Host requirement

`soil:http` is not linked by the Canvas GPUI presenter yet — the World asks for
it, so a host that cannot provide it refuses this package by name instead of
instantiating it half-blind. That is the one piece between this Spore and a
live board; `spores/spotify` is the reference for the fetch shim.

## Look at it in a window

A wasm component cannot open a window by itself, so `native/` is a small
desktop binary that compiles [`src/view.rs`](src/view.rs) directly: the same
layout and render code the Spore runs, with `soil:http` swapped for two files on
disk. Nothing in `view.rs` knows which of the two is driving it.

```sh
curl -s 127.0.0.1:8792/runs  > /tmp/elmers-runs.json
curl -s 127.0.0.1:8792/graph > /tmp/elmers-graph.json

cd spores/elmers-ui/native
cargo run -- --runs /tmp/elmers-runs.json --graph /tmp/elmers-graph.json
cargo run -- … --brain          # open on the graph
```

The preview polls the two files every 2 s, so re-running the `curl`s updates an
open window; `--runs`/`--graph` pick other paths.

On macOS, GPUI needs Apple's Metal compiler, which Command Line Tools do not
ship — if `xcode-select -p` prints the CLT path, prefix the build with
`DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer`, or the fork's
`gpui_apple` build script fails with `unable to find utility "metal"`.

`cargo test` here also runs the view's parser tests. Their fixtures are copied
from live responses, which is how the `started_at`-arrives-as-a-string bug was
found and why the clock column is not silently blank.
