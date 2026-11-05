# Terminal Spore

A Mycelium directory plus one terminal session, rendered by the host.

```
Spore: directory + VT parser (terminal-core / vt100 now, libghostty-vt next)
  import soil:mycelium/directory
  import soil:mycelium/session
  import soil:terminal/surface
        │
        ▼
host: fixture or Mycelium adapter + ASCII or Canvas grid renderer
```

The Spore has no sockets, SSH client, credentials, local process, or filesystem.
The host owns Mycelium identity/routing and terminal pixels. This is the first
vertical slice; `soil:ssh` is deliberately deferred.

## Current slice

- `soil:mycelium.directory.targets/get` — access-filtered targets.
- `soil:mycelium.session.open-shell/read/write/resize/status/close/exec`.
- `soil:terminal.surface.open/present/next-input/observe/close`.
- The mock host serves a fixture directory and a fake PTY that emits ANSI.
- `terminal-core` converts bytes to a bounded cell grid with `vt100`.
- The host prints the grid as ASCII and returns a JSON snapshot.

Handles are `u64` for v0, scoped to the component instance. A later revision can
replace them with WIT resources without changing the verbs.

## Build

```sh
rustup target add wasm32-wasip2
cargo build -p spore-terminal --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/spore_terminal.wasm spores/terminal/module.wasm
```

## Run the mock

```sh
cargo test -p terminal-core
cargo run -p terminal-core --bin terminal-mock
cargo run -p terminal-host -- open-first
cargo run -p terminal-host -- list
```

Expected: a `status: ready` frame, cursor at `(0, 13)`, title
`terminal fixture`, and a JSON snapshot with four targets.

## Next

1. Replace `vt100` with libghostty-vt linked as a guest library.
2. Replace the fixture adapter with `mycelium targets --json` and a host PTY
   running `mycelium ssh <device>`.
3. Replace the ASCII renderer with the real `soil:terminal` surface.
4. Add session records and the GUI/TUI toggle.
