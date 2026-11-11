# elmers-ui

A read-only board of Elmers workflow runs, drawn by GPUI inside a Spore.

```
┌ Elmers   runs (read-only)              12 ok  1 error  0 other ┐
│ ● notifications/pr            19:42  │  notifications/pr     ok  19:42
│ ● notifications/marbles       19:41  │  {"message_id": 7605}
│ ● dev/pr-close-marble         19:40  │
│                                      │
└ j/k move   r refresh   q quit     12 runs · refreshed 2s ago ┘
```

## Scope

Read-only, deliberately. The Spore asks the sidecar one question — `GET /runs`
— and draws the answer. It does not start, stop, edit, or inspect a workflow's
source, and it holds no Lua: a UI that can run things is a different grant and a
different review.

| It can | It cannot |
|---|---|
| list runs, status, time, result/error | start or stop a run |
| show one run's payload | edit or read workflow files |
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
