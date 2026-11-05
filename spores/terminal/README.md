# Terminal Spore

A Mycelium directory plus one live terminal session, rendered by the host.

```
Spore: directory + VT parser (libghostty-vt, linked as a guest static lib)
  import soil:mycelium/directory
  import soil:mycelium/session
  import soil:terminal/surface
        │
        ▼
host: Mycelium adapter + host PTY + minifb/ASCII surface
```

The Spore has no sockets, SSH client, credentials, local process, or filesystem.
The host owns Mycelium identity/routing and terminal pixels.

## Real path

- Directory: `mycelium targets --json`, falling back to
  `mycelium fleet status --json` for fleet hosts.
- Session: a host PTY running `mycelium ssh <hostname>` with
  `TERM=xterm-256color`.
- Parser: libghostty-vt built for `wasm32-freestanding` and linked into the
  Spore; the C shim returns a flat cell grid.
- Surface: ASCII by default, or a real `minifb` window with
  `TERMINAL_WINDOW=1`.
- Input in window mode is forwarded to the PTY; resize is fixed at 80x24 for
  v0.

## Build

```sh
rustup target add wasm32-wasip2
cargo build -p spore-terminal --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/spore_terminal.wasm spores/terminal/module.wasm
```

The static archives are in `vendor/`; see `vendor/README.md` to rebuild them.

## Run

```sh
# real directory
cargo run -p terminal-host -- list

# real PTY to the first shell-capable target
cargo run -p terminal-host -- open-first

# real PTY to a specific target
cargo run -p terminal-host -- open mycelium:agora-one

# live window
TERMINAL_WINDOW=1 cargo run -p terminal-host -- open mycelium:agora-one

# deterministic fixture for tests
TERMINAL_MOCK=1 cargo run -p terminal-host -- open-first

# exercise the PTY path without Mycelium
TERMINAL_SHELL='printf "hello\n"' cargo run -p terminal-host -- open-first
```

`MYCELIUM_BIN` overrides the binary. `TERMINAL_SHELL` is development-only and
replaces the spawned command.

## Current limits

- The Spore runs one bounded initial drain, then the host streams
  `feed`/`present` while the window is open.
- Window resize and mouse input are not wired yet.
- `open-exec` uses `mycelium exec`; terminal sessions use `mycelium ssh`.
- No session records or GUI/TUI toggle yet.
- The 80x24 view is fixed.
