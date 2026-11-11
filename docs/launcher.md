# The launcher

Status: requirements and design, 2026-11-07. Nothing here is implemented on
`main` yet; the launcher regions described in [What exists today](#what-exists-today)
are on the `pi/inbox-canvas` branch.

This document answers three questions in order: what the reference launchers
actually do, what this repository already has, and what the missing middle is.

## 1. What the reference launchers do

Every application in this class is the same five jobs:

1. **Summon** — a global hotkey puts a floating bar over any app, in one frame.
2. **Resolve** — a typed string becomes a typed list of things.
3. **Rank** — the thing you meant is first, without you spelling it out.
4. **Act** — one primary action, plus a contextual action panel.
5. **Extend** — somebody else's integration, installed without shipping a new
   binary.

Everything else is a built-in module hanging off those five: clipboard, snippets,
calculator, window management, AI.

### The field

| App | Licence | Stack | AI | Extension model |
|---|---|---|---|---|
| **Raycast** | proprietary, free tier | Rust + Node + WebView | first-party, tiered | React/JS extensions, 1000s in store |
| **Alfred** | proprietary (Powerpack) | Objective-C | none built in | Workflows: script filter → object graph |
| **Floe** | AGPL-3.0 | native macOS | launcher answers, BYO endpoint/Apple Intelligence | runs Raycast extensions unmodified |
| **asyar** | GPL-3.0 core, MIT SDK | Rust + Tauri | BYOK chat, agents, MCP, in-place commands | own system, sandboxed iframes |
| **SuperCmd** | MIT (v1) | Electron + React, Swift helpers | BYOK chat, Quick Fix, agents, dictation | `@raycast/api` compatibility shim |
| **Ainto** | GPL-3.0-or-later | Swift + Rust | Claude Code CLI | not established |

The licence and AI columns were supplied from a side-by-side review; the stack
and extension rows are from each project's own README. Notably, asyar describes
its core as GPL-3.0 in the licence section while its badge says AGPL — worth
resolving before anyone depends on the exact terms.

### Feature inventory

**Summon**

- Global hotkey, configurable; double-tap-modifier as an alternative trigger.
- Compact mode: the window is only the search bar until you type.
- Pop-to-root: return to root search after a timeout.
- Appears over full-screen apps and on every Space.
- Non-activating: the app you were in stays the app you were in.

**Resolve**

- Applications, including alternate names and Spotlight keywords (`iCal` finds
  Calendar).
- Files and directories, with recent files when the query is empty.
- Commands: built in, extension-contributed, script commands, saved shell
  commands.
- Quicklinks, system settings panes, calendar events, contacts, open windows,
  open browser tabs, bookmarks, menu-bar items.
- Calculator: math, units, currency, dates, time zones, in natural language.
- URL detection with scheme inference; colour-value detection and conversion.
- Scopes: a prefix (`files`, `apps`, `windows`) narrows the search to one source.
- Typed `@` mentions with stable locators.

**Rank**

Raycast documents its order explicitly, highest first:

1. exact alias match
2. alias prefix match
3. title fuzzy match
4. subtitle and keyword matches
5. frecency — a blend of how often and how recently you picked it

Search sensitivity is a three-level setting. Favorites pin above everything;
aliases and per-command hotkeys are assigned inline; both show as a tag on the
row. Ranking is per-query learned and resettable per item.

**Act**

- Enter runs the primary action; `⌘K` opens the action panel for the selection.
- Argument bar: up to three inline fields in the search bar, typed as text,
  password, or dropdown, navigated with the arrow keys.
- Secondary views: list + detail, metadata panels, forms.
- Preview: Quick Look on a file; `Shift-Tab` walks to the parent directory.
- Results carry sections; `⌘↑/↓` jumps between them, `⌥↑/↓` pages.

**Extend**

- Raycast: React components (`List`, `Detail`, `Form`, `ActionPanel`) plus a
  host API, distributed through a store.
- Alfred: a workflow is a graph of objects; a script filter returns JSON that
  becomes rows.
- asyar: sandboxed iframes over a `postMessage` bridge with a per-permission
  capability check; the host process makes the network request.
- Floe and SuperCmd both ship a `@raycast/api`-compatible runtime, and both are
  explicit that compatibility is partial.

**Built-in modules, by app**

| Module | Raycast | Alfred | Floe | asyar | SuperCmd |
|---|---|---|---|---|---|
| Clipboard history | yes | Powerpack | yes | yes | unlimited |
| Snippets / text expansion | yes | yes | yes | yes | yes |
| Quicklinks | yes | yes | yes | portals | yes |
| Calculator | full | basic | units | full | inline |
| Window management | yes | Powerpack | tiling | 17 presets | yes |
| File search | indexed | yes | yes | yes | yes |
| Menu-bar item search | — | — | yes | — | yes |
| Window switcher / Alt-Tab | — | — | yes | — | yes |
| AI chat | yes | — | answers | agents + MCP | chat + agents |
| Selection actions | yes | — | — | in-place | Quick Fix |
| Dictation / TTS | dictation | — | — | — | both |

## 2. What exists today

There were **two** different things called a launcher, and they do not overlap.
The package browser has since been renamed `spores/spore-browser` so that
"launcher" means one thing.

### `spores/spore-browser` — a package browser

38 lines of module, 49 of Lua. It imports `soil:spore/catalog`, lists installed
Spore packages with their launch surfaces, and calls `catalog.launch(id, surface)`.
It is the installed-Spore browser, not a root search.
`spores/spore-browser-host` is its wasmtime host.

### `spores/chrome` — the shell, with a launcher region

A single Spore that owns six subject regions — `top-bar`, `dock`, `launcher`,
`switcher`, `control-center`, `window` — each backed by a Lua file in
`[chrome.sources]`. It is deliberately presentation-only:

> The host owns the hit targets and the selection; this file only has to make the
> selection visible.

`ui/launcher.lua` already draws the shape the reference apps converge on: a
heading, a right-aligned target label, a scoped search (`all` / `workspaces` /
`native` / `canvas`), variable-length rows supplied as flat `module.inputs`
(`row0-title`, `row0-available`, …), a footer that names the keys, and an Alt-Tab
window cycle on the same surface.

**This is the right seam.** Presentation is Lua and reloads live; selection and
hit-testing are host-owned; the host decides what a row means. What is missing is
not UI. It is the middle: nothing produces ranked, typed results, and nothing
defines what acting on a row does.

### The capability that already exists for this

`soil:spore@0.1.0` (`catalog.entries`, `catalog.launch`) is the installed-package
catalog. It is used by `spores/spore-browser` but is **not yet in soil's canonical
`wit/` directory** and is therefore not enforced by `crates/soil-abi`. That is a
consolidation bug, not a design gap — see [Consolidation](#5-consolidation).

Alongside it, soil already ships most of the middle:

| Reference feature | Existing capability | State |
|---|---|---|
| Root search over files, records, spores, services, peers | `soil:search@0.5` | WIT + host |
| Fuzzy / substring / full-text / semantic / regex modes | `soil:search` `mode`, `soil:index@1.1` | WIT + host |
| Fuzzy path ranking, exact-path promotion | `soil:index` | host |
| Command palette, labels, shortcuts, argument schemas | `fungos:actions@1.0` `catalog` + `actions` | WIT + host |
| Installed-app rows and launch | `soil:spore@0.1.0` `catalog` | WIT + host, uncanonical |
| Frecency, aliases, favorites | `soil:appdata@1.0` | WIT + SDK |
| `@` mention autocomplete | `soil:search/mentions` | WIT + host |
| Result notification | `soil:notify@1.1` | WIT + host |
| Shell-command mode | `soil:terminal@0.1`, `soil-command` registry | WIT + host |
| AI answers in the launcher | `soil:inference@0.2` | WIT + models |
| Installed wasm commands on a virtual PATH | `crates/soil-command` | host |

## 3. The gap

Four things are missing, in descending order of how much they block.

1. **No root search pipeline.** `soil:search` composes files, records, spores,
   peers and services, but its `action` kind is documented as "reserved for a
   host action registry" and nothing populates it. There is no `application`
   kind. A launcher that cannot find an app is not a launcher.
2. **No ranker.** `soil:search` merges by score then title. Alias, keyword and
   frecency ordering — the part users actually feel — has no owner.
3. **No action panel.** `fungos:actions/catalog` returns exactly the descriptors
   an action panel needs (`label`, `description`, `shortcut`, `input-schema`,
   `requires-input`, `approval`), and nothing in this repository renders one.
4. **No selection model.** `ui/launcher.lua` is told which row is selected; it
   does not choose. Nothing above it converts a query into rows plus a selection
   plus a resolved action.

## 4. Design

### Ownership

| Party | Owns | Never owns |
|---|---|---|
| Host | hotkey, window, hit targets, selection, ranking policy inputs, app inventory, macOS launch | what a row looks like |
| Launcher Spore (wasm) | query → rows; alias/frecency state; action resolution | pixels, window geometry, app discovery |
| Lua view | every presentation decision | which row is selected |
| `soil:search` | the granted source set | ranking across sources |
| `fungos:actions` | what may be invoked | which action is offered first |

The rule that keeps this honest: **discovery is search, execution is actions.**
A row is a locator plus display metadata; pressing Enter resolves it to an action
id and invokes that. Nothing about listing a thing grants acting on it. This is
already the stated doctrine of `soil:search`, applied to apps.

### Bootstrapping from what exists

Do not restart. `spores/chrome`'s launcher region is the view; it keeps its
`module.inputs` contract. What changes is who fills it:

```
query keystroke
   │
   ▼
host  ── soil:search.search(kinds=[file, record, spore, application, action])
   │      + soil:spore.catalog.entries()
   │      + fungos:actions.catalog.list-actions()
   ▼
launcher Spore ── merge, alias-match, frecency-rank  ──> rows + selection
   │
   ▼
Lua view (ui/launcher.lua) ──> scene
   │
Enter ──> fungos:actions.invoke(id, input)   or   soil:spore.catalog.launch(id)
```

### The two additions soil needs

Both are additive and both have precedent.

**`soil:search` gains `kind.application`.** The host supplies an explicit,
host-granted inventory of installed applications exactly the way it already
supplies `peers` and `services` from snapshots. The hit's `id` is a bundle
identifier, `title` the display name, `source` the string `apps`. The Spore still
cannot enumerate without the host opening the source.

**`soil:search.hit` gains an optional icon.** Application rows are unrecognisable
without one. Add `icon: option<string>` carrying an asset reference —
`asset:sha256:…` — resolved by the host on the render path, never fetched by the
Spore. `docs/content-rendering.md` already defines that resolution and the
"missing is a named placeholder" rule. Until it lands, the Lua view draws the
monogram badge the chrome Spore already uses (`[presentation] badge`).

### Ranking

Ranking is policy, so it lives in the Spore, not in `soil:search`:

```
exact alias            > alias prefix > title fuzzy > subtitle/keyword > frecency
```

The host's `score` is a tiebreaker within the last tier, not the final order.
The Spore therefore asks for a generous page and re-ranks locally. Frecency is
`(uses, last-used-at)` per item key, kept in `soil:appdata` so it syncs per user
per app for free. Reset-ranking is one key delete.

### Action panel

`⌘K` opens a panel built directly from three sources, in this order: actions the
selection owns, actions the launcher owns, host/system actions. Each row is
`catalog.action.label`; a row with `requires-input = true` opens the argument
bar, which is a form generated from `input-schema` (JSON Schema 2020-12 is
already the declared contract). Approval classes are rendered, not enforced —
the host enforces.

This means the argument bar is not special-cased per command. A command that
declares three string arguments gets three fields, because the schema says so.

### Manifest

The launcher's grants, stated as intent:

```toml
[grants]
"soil:canvas"         = "present the launcher region"
"soil:search"         = "resolve a query to locators"
"soil:spore.catalog"  = "list and launch installed Spores"
"fungos:actions"      = "resolve a row to an invocable action"
"soil:appdata"        = "keep aliases, favorites and frecency"

[denied]
transport = ["socket", "quic", "iroh"]
host = ["process", "secrets", "environment", "raw-filesystem", "clock"]
```

No `soil:vfs` — a launcher names locators; opening a file is the file's
business. No clipboard. No accessibility. Those are host surfaces, described in
`soil/docs/launcher-host.md`.

## 5. Consolidation

The `pi/inbox-canvas` branch is ahead of `main` by 66 files, and the launcher
work is split across two packages plus a host plus a WIT package that soil does
not know about. Sequence:

1. **Canonicalise `soil:spore`.** Move `spore.wit` from
   `spores/spore-browser/wit/deps/soil-spore/` into soil's `wit/soil-spore.wit`, add
   it to the table in soil's `README.md`, and add coverage in
   `crates/soil-abi` so an accidental ABI change fails the build. Everything else
   depends on this being real.
2. **Land `pi/inbox-canvas` on `main`** as-is. The chrome regions are good and
   they are the UI we want; do not rewrite them during the merge.
3. ~~**Rename.**~~ Done on 2026-11-07: the package browser is
   `spores/spore-browser` with `dev.fungos.spore-browser`, and its host is
   `spores/spore-browser-host`. The `launcher` region stays where it belongs, in
   `spores/chrome`.
4. **Then build the middle** described in §4 against the existing view.

## 6. What we deliberately do not copy

**Raycast extension compatibility.** Floe and SuperCmd both ship a
`@raycast/api` runtime, and it is their strongest migration story. Here it would
mean embedding a JavaScript engine and a WebView in the launcher to run code with
more authority than any Spore, which contradicts the whole model. The equivalent
is an extension author writing a Spore: a signed manifest, a wasm module, a Lua
view. State that plainly and let the store be the pitch.

**Unsandboxed extensions.** Floe's own documentation says extensions run without
a sandbox under your user account. A Spore is sandboxed by construction; do not
import the failure mode.

**A second renderer.** The launcher is Canvas like everything else. The radial
menu in the DragShift Spore (see `docs/dragshift.md`) needs `mesh`, which the
scene already has; it does not need a new one.

## 7. Open questions

- **Is the launcher a Spore or host chrome?** The chrome Spore already contains
  it, and `docs/surface-windowing.md` says security-critical overlays are never
  Spores. A launcher is not a consent prompt, but it does take exclusive keyboard
  focus. Proposed: the launcher region stays a chrome Spore, and Canvas grants it
  an exclusive modality only because the host allowlisted its id — the same
  mechanism `[system] role = "consent"` already uses.
- **Does the launcher need a resident module, or can it be cold?** asyar measures
  13.9 ms from hotkey to visible, Raycast 22.2 ms. A wasm instantiation plus a
  Lua JIT compile per summon will not fit. The launcher should stay warm and
  present on demand; `soil:canvas` is polled with no per-frame callback, so the
  host can drive exactly one `present()` per keystroke.
- **Where does the wall clock come from?** `soil:time` is monotonic on purpose;
  there is no wall clock in the ABI. A top bar draws a clock and a calculator
  converts time zones, so this is now load-bearing. Either a narrow `soil:clock`
  or a host action; see `soil/docs/launcher-host.md`.
- **One row list or typed rows?** Flat `row0-title` keys are cheap and easy for a
  person to read, and they lose the type of each row. A typed row would let the
  action panel choose its first action without a second round trip.
