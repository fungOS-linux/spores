# Contributing

Every spore in this repository is MIT and every contribution is accepted under
the [DCO](DCO). Sign off your commits:

```sh
git commit -s
```

## Adding a spore

Create `spores/<name>/` with:

- `spore.toml` — the manifest. Declare the minimum grants and say why each is
  required. Do not request `process`, `secrets`, `environment`, or a raw
  filesystem unless the spore cannot work without them, and state that case in
  the pull request.
- `src/` — document/app semantics only. No colours, coordinates, fonts, or
  widget names. If you reach for those, they belong in `ui/`.
- `ui/*.lua` — every presentation decision. No `os`, `io`, `package`, `dofile`,
  or `loadfile`; Canvas gives the UI no ambient authority. `require` is the one
  exception and only for a declared dependency: `require("fungos.ui")` names the
  SDK library, the composer inlines it at build time, and the module Canvas
  actually loads makes no call. There is no resolver in the sandbox to audit.
- `tests/` — a fixture that renders the UI without a browser or compositor.

## Rules that are reviewed, not just documented

1. One scene pipeline. Do not ship per-host renderers.
2. The Spore cannot tell which host is drawing it.
3. The Lua UI is deterministic: same snapshot in, same scene out.
4. Every host-resolved reference is a `Target`, never a path or a URL the Spore
   fetches.
5. A presentation change must not require rebuilding the module.

## Checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

If Lua is installed, `cargo test` also compiles `ui/*.lua` with `luac -p` and
runs the fixtures.
