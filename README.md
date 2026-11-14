# Spores

Spores are the applications of fungOS. A Spore is a signed manifest plus a
WebAssembly module that runs against exactly the capabilities it was granted.
No filesystem, network, clock, or device access exists beyond that grant.

This repository is the public home for spores that ship with fungOS and for the
spore format itself. Personal and unreleased spores live in
[`ajmwagar/spores`](https://github.com/ajmwagar/spores) until their author
publishes them.

**License:** every spore here is MIT, or Apache-2.0 OR MIT where its manifest
says so. The manifest is the licence of record; `spore.toml` and `LICENSE`
are checked together in review.

## Identity, manuals, and listings

Every Spore declares what `man` prints and what a store may list:

```toml
[spore]
summary = "Markdown reader/editor with a hot-reloadable Lua UI"
description = "Read and edit Markdown as one document."

[docs]
usage = ["markdown [FILE]", "markdown --version"]
man = "docs/man.md"

[catalog]
categories = ["productivity"]
keywords = ["markdown", "editor"]
```

`soil-spore man <dir>` renders the page: NAME, SYNOPSIS, DESCRIPTION, VERSION,
LICENSE, LAUNCH, GRANTS and SEE ALSO. Only the synopsis and the prose are
authored; everything else is generated from the manifest, so a page cannot drift
from what the package declares. `soil-spore info <dir>` prints the compact
identity block, and `--version` works on the tool itself.

Store artwork — icons, screenshots, the rich listing record — is a CID, not a
file:

```toml
[presentation]
icon = "ipfs://bafy…"

[catalog]
screenshots = [{ cid = "ipfs://bafy…", caption = "Editing a note" }]
```

That keeps the bundle to what a Spore needs at runtime. `soil-spore
check --publish` refuses a listing without two screenshots, a manual, an icon, a
homepage and a category; human review comes after. See
[`docs/spore-store.md`](https://github.com/fungOS-linux/soil/blob/master/docs/spore-store.md).

A package is the manifest, the module, every `.lua` under `ui/`, declared
assets, `docs/man.md`, and `spore.lock`. `src/`, `wit/`, `Cargo.*`, tests and
repo docs are build inputs and are not packed.

## One scene, three hosts

A Spore owns document semantics. For a scene Spore, Canvas owns every pixel.
Soil decides which host to launch. The same module and the same Lua UI run on
three surfaces without a rebuild and without a per-host renderer in the Spore:

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

A Spore can also own its drawing when it needs GPUI semantics: it lays out an
offscreen GPUI window, renders through `wasi:webgpu`, and presents the texture
through `soil:present`. The host still owns the window, compositor, input,
and capture; no native surface enters the guest. [`spores/vitals-view`](spores/vitals-view)
and [`spores/lights-view`](spores/lights-view) are the working examples. The Lua path above remains the renderer-independent
option; the GPU path trades host portability for GPUI layout and text.

The hosts are still landing. What already holds is the contract: the snapshot
shape, the Lua entry point, and the rule that a Spore cannot tell which host is
drawing it. See [`docs/host-contract.md`](docs/host-contract.md).

New spores that draw their own interface use the **GPUI path** instead: the
Spore owns the view and renders it, the host owns the WebGPU device and the
compositor. [`spores/terminal-gpui`](spores/terminal-gpui) is the reference,
and [`docs/gpui-spores.md`](docs/gpui-spores.md) is the path every new drawing
Spore should start from. The scene contract above is unchanged for existing
spores.

## Storage

Spores never see paths. A document is an opaque `DocId` resolved through
`soil:vfs`, which is still work in progress. Until it exists, the spore is
compiled against a `Vfs` trait with an in-memory implementation, so the
storage boundary is already the real one.

## Spores here

```text
spores/chrome              shell regions: top bar, dock, launcher, switcher, control center, window
spores/store               Spore Store front end: browse, listings, reviews, classified updates
spores/spore-browser       installed-Spore browser, launched through soil:spore
spores/spore-browser-host  wasmtime host for the browser Spore
spores/dragshift           convert and edit a dropped file from a radial menu
spores/search              fuzzy search, ranking, and action dispatch
spores/fleet               mycelium peers, sites and topology on one wall
spores/markdown            Markdown reader/editor
spores/files               VFS browser + storage census (files.myc)
spores/unibus              read-only inbox (dev.fungos.inbox)
spores/worklog             cross-tool timeline (dev.fungos.worklog)
spores/vitals              cross-device health fusion (dev.fungos.vitals)
spores/vitals-view         GPUI vitals rings, sleep, + cards over wasi:webgpu + soil:present
spores/lights-view         GPUI Hue room rail, scenes, and per-light controls over soil:present
spores/pulse               cron schedules and webhook ingress health (dev.fungos.pulse)
spores/terminal            terminal Spore
spores/terminal-core       terminal semantics shared by the tmux and host front ends
spores/terminal-gpui       terminal view rendered by GPUI inside a wasm32-wasip2 Spore
spores/launcher-gpui       GPUI launcher: installed Spores, selection, and launch
spores/crm                 CRM Spore over namespaced records
spores/contacts            personal contacts + explicit CRM projections
spores/notify              notification tray and ringer
spores/rooms               audio/video rooms with a Holodeck twin
spores/consent             system consent popup
spores/dfm-report          DFM report renderer
spores/mush                sandboxed VFS shell
spores/hue                 Philips Hue CLI over the local bridge API
spores/spotify             Spotify Connect remote for RASpotify
spores/tui                 ratatui dashboard that emits ANSI frames
spores/cli-http            the narrow host-mediated HTTP seam shared by CLI spores
spores/media-probe         soil:media fixture
spores/surface-probe       soil:surface-import fixture
```

`terminal-host` is not here: it is the native Mycelium/PTY host in
[`fungOS-linux/soil`](https://github.com/fungOS-linux/soil),
`crates/terminal-host`. The terminal package names it as its
`[launch] native` host.

## TUI spores

A TUI is bytes. A TUI Spore owns layout and state and returns one complete ANSI
frame per call; the host owns the screen. [`spores/tui`](spores/tui) is the
proof: it uses ratatui with **no terminal backend** and implements
`ratatui::backend::Backend` over a `Vec<u8>`, including truecolor, attributes,
cursor placement and region clears.

```text
ratatui App ──► AnsiBackend ──► Vec<u8> ──► soil-vt ──► cells ──► pixels
                                      └──► a real terminal
```

Because the frame is just bytes, the same module can run on all three surfaces:
a real terminal, `soil-vt` inside the terminal Spore, or Canvas. `mush` can
*launch* a TUI as a command, but a line-oriented shell cannot host an
interactive one without handing over the screen — that handover is the terminal
surface's job, not the shell's.

## Layout

```text
spores/markdown     the Markdown reader/editor Spore
                    src/   semantics and editor state
                    ui/    the Lua UI Canvas renders
                    spore.toml  manifest, grants, launch surfaces
spores/store        the storefront Spore (id `dev.fungos.store`)
                    ui/    browse, listing and updates views
                    docs/  the manual `man store` prints
spores/unibus       the inbox Spore (id `dev.fungos.inbox`)
                    src/   record keys/schemas for mail + chat metadata
                    spore.toml  records/unibus grants, read-only
spores/worklog      the worklog Spore (id `dev.fungos.worklog`) + receiver
                    src/   fungos.worklog/event@1: normalize, order, store
                    ui/    the timeline view Canvas renders
                    (composed by tend, like every other Lua view)
spores/files        the VFS browser and storage census (id `dev.fungos.files`)
                    src/   VFS adapter, bounded scan, flat snapshot
                    ui/    the three-pane view Canvas and files.myc render
                    src/bin/serve.rs  the host stand-in that serves files.myc
docs/               host/spore contract, records/sync, AI inbox spec
spores/fleet        the mycelium wall
                    src/   fleet status + topology -> one snapshot
                    ui/    the graph-and-tables view Canvas renders
                    web/   the same scene document, served at fleet.myc
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
