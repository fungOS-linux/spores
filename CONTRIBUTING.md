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
- `ui/*.lua` — every presentation decision. No `os`, `io`, `require`, `package`,
  `dofile`, or `loadfile`; Canvas gives the UI no ambient authority.
- `docs/man.md` — the manual `man <name>` prints. Write what the Spore is for,
  how a person starts it, what it may reach, and what it deliberately cannot do.
  The rest of the page (version, licence, launch surfaces, grants) is generated
  from `spore.toml`, so do not repeat it here.
- `tests/` — a fixture that renders the UI without a browser or compositor.

`spore.toml` also carries the listing metadata a store reads: `description` (one
paragraph), `[docs] usage` (the synopsis lines), `[catalog] categories` from the
fixed vocabulary, and keywords. Screenshots, icons and the rich listing record
are `ipfs://` CIDs, never files in the package: a bundle carries the reference
and not the bytes. `soil-spore check --publish` lists what a listing is still
missing.

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

A package does not ship build inputs: `src/`, `wit/`, `Cargo.toml`, tests and
repo docs stay out of the tar. What ships is the manifest, the module, every
`.lua` under `ui/`, declared assets, `docs/man.md`, and `spore.lock`.
`soil-spore lock .` records exactly that.
