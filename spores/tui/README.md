# tui

A [ratatui](https://ratatui.rs) dashboard that renders **ANSI bytes** instead of
taking over a terminal. No crossterm, no termion, no termwiz: the Spore owns
layout and state, the host owns the screen.

```wit
export start:  func(cols: u16, rows: u16) -> result<string, string>;
export key:    func(key: string) -> result<string, string>;
export resize: func(cols: u16, rows: u16) -> result<string, string>;
export frame:  func() -> result<string, string>;
export render: func(cols: u16, rows: u16, tick: u64) -> result<string, string>;
```

Every call returns one complete frame, so the host can write it to a real
terminal, feed it to `soil-vt` inside the terminal Spore, or rasterise it in
Canvas. The Spore has **no capabilities at all** — no filesystem, network,
clock, or GPU.

## Why this shape

A TUI library is usually coupled to a terminal backend. Splitting the backend
out is what makes a TUI portable to WASM:

```text
ratatui App ──► AnsiBackend ──► Vec<u8> ──► soil-vt ──► cells ──► pixels
                                      └──► a real terminal
```

`src/ansi.rs` is that adapter: it implements `ratatui::backend::Backend` over a
`Vec<u8>`, including truecolor, indexed colour, attributes, cursor placement and
region clears. The rest of the Spore is ordinary ratatui.

## Running it in mush / the terminal Spore

mush's `soil:command` runs installed wasm commands and captures their stdout.
A TUI needs more than that: it needs a screen and an input loop. The terminal
Spore already owns both (it vendors libghostty-vt for wasm32), so the intended
path is:

1. the terminal Spore starts this component and writes `start`'s frame to its
   VT;
2. it feeds normalized keys to `key` and writes the returned frame;
3. `resize` reflows on a window change.

A line-oriented shell can still *launch* it (`render` gives a one-shot frame for
a thumbnail or a smoke test), but it cannot host an interactive TUI without
handing over the screen.

## Build

```sh
cargo build --target wasm32-wasip2 --release -p spore-tui
cp target/wasm32-wasip2/release/spore_tui.wasm spores/tui/module.wasm
```

Native tests:

```sh
cargo test -p spore-tui
```
