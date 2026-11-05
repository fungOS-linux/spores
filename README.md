# Spores

Spores are the applications of fungOS. A Spore is a signed manifest plus a
WebAssembly module that runs against exactly the capabilities it was granted.
No filesystem, network, clock, or device access exists beyond that grant.

This repository is the public home for spores that ship with fungOS and for the
spore format itself. Personal and unreleased spores live in
[`ajmwagar/spores`](https://github.com/ajmwagar/spores) until their author
publishes them.

**License:** every spore in this repository is MIT. See [LICENSE](LICENSE).

## One scene, three hosts

A Spore owns document semantics. Canvas owns every pixel. Soil decides which
host to launch. The same module and the same Lua UI run on three surfaces
without a rebuild and without a per-host renderer in the Spore:

| Surface | Launched by | Rendered by |
|---|---|---|
| Web | Soil, `web` | Canvas (`canvas-web`) |
| Native | Soil, `native` | Canvas on a Wayland/DRM surface |
| Canvas window | Soil, `canvas` | Canvas, as a granted owner surface |

The first spore here, [`spores/markdown`](spores/markdown), is a Markdown
reader/editor that proves the split:

- **Rust/WASM** parses Markdown, owns editor state, and emits one flat snapshot.
- **Lua** (`ui/editor.lua`) owns every presentation decision. Canvas JITs it and
  hot-reloads it, so an agent can restyle a running app without a rebuild.
- **Canvas** lays out and renders all three surfaces.

The hosts are still landing. What already holds is the contract: the snapshot
shape, the Lua entry point, and the rule that a Spore cannot tell which host is
drawing it. See [`docs/host-contract.md`](docs/host-contract.md).

## Storage

Spores never see paths. A document is an opaque `DocId` resolved through
`soil:vfs`, which is still work in progress. Until it exists, the spore is
compiled against a `Vfs` trait with an in-memory implementation, so the
storage boundary is already the real one.

## Layout

```text
spores/markdown     the Markdown reader/editor Spore
                    src/   semantics and editor state
                    ui/    the Lua UI Canvas renders
                    spore.toml  manifest, grants, launch surfaces
docs/               the host/spore contract and surface windowing
```

## Build and check

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Contributing

Contributions are accepted under the [Developer Certificate of Origin](DCO).
Sign off your commits (`git commit -s`). Every spore declares its capabilities
in `spore.toml`; a pull request that adds a capability must say why the spore
cannot meet its purpose without it.
