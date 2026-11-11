# Divebar view API

Divebar is the system bar: workspaces on the left, focus in the middle, ambient
facts on the right. It occupies the `top-bar` chrome region. It is an SDK view,
not a runtime: `tend` composes `ui/divebar.lua` into a single-file Canvas module
and the compositor scales the authored 1200x44 scene into whatever extent it
grants the region.

## Entrypoint

`ui/divebar.lua` exports one module:

```lua
local view = dofile("ui/divebar.lua")  -- tend composes it; see Packaging
local scene = view.render(snapshot)
```

- `M.render(snapshot: table)` — pure projection, no side effects. It draws into
  the surface `fungos.ui` hands it, so the same file is captured by `tend`,
  rendered by Canvas, or asserted by a test without change.
- `M.name = "dev.fungos.divebar"`, `M.version = "0.1.0"` — identity echoed from
  `spore.toml`; hosts may assert they match.

The file opens with `local ui = require("fungos.ui")`. That is a **build-time**
import, not a runtime one: Canvas removes the `require` global from a scene, and
`tend` inlines every SDK module and rewrites `require` to a local shim before the
view ever runs. Nothing here loads a file at render time.

## Snapshot

```lua
{
  workspaces = { { id: string, focused: boolean?, occupied: boolean? } },
  active = { app: string?, title: string? },
  ambient = { clock: string?, network: string?, battery: string? },
}
```

| field | type | meaning |
|---|---|---|
| `workspaces` | array | left-to-right order; at most one entry has `focused = true` |
| `workspaces[].id` | string | label drawn in the pill; digits or names |
| `workspaces[].occupied` | boolean? | reserved for occupancy styling; v0 draws focus only |
| `active.app` | string? | focused application id, e.g. `canvas` |
| `active.title` | string? | focused window title |
| `ambient.clock` | string? | host-formatted wall clock (`13:17`), never a timestamp |
| `ambient.network` | string? | omit when there is no network |
| `ambient.battery` | string? | omit on a desktop; the view closes the gap |

Every field is optional. An empty snapshot renders a valid empty bar: this is
the fallback bar, so a failed Spore load must never produce a blank desktop.

## Actions

None. Divebar v0 is read-only. Workspace focus follows the compositor, not the
pointer, so the view emits no intents and needs no action router. Clicking a
pill is a host concern (`soil:shell.focus`, planned); when it lands, the action
will be `divebar.focus` with payload `{ workspace_id: string }`.

## Slots

None. Slots would be an extension point a *host* resolves, and no composer or
region loader resolves them yet; declaring `bar.center` here would document a
colour of a feature that cannot be filled. When a chrome host grows slot
resolution, the three clusters (workspaces, focus, ambient) become the first
candidates, and this table becomes real.

## Components

None. Divebar is a system chrome Spore, not a library. The reusable pieces
(`pill`, `ambient-fact`) live in the SDK so other chrome surfaces share one
implementation rather than copying this file.

## Theme/config

The view names **semantic inks** — `background`, `surface`, `border`, `accent`,
`text`, `muted` — and never a colour value. The host resolves a name to a colour,
so retheming the shell moves this bar without editing it. Text scale comes from
`ui.theme().body` inside `fungos.ui`; nothing in this file reads a pixel size
from config.

No app-specific config keys are read. `ui/API.md` describing keys the view does
not consume would be a contract with no implementation, so the profile surface
today is exactly the host's ink palette.

## Stable ids

Other Spores and rices may target these node ids:

`ground`, `edge`, `ws-<n>`, `ws-label-<n>`, `active-title`, `active-app`,
`divider`, `clock`, `network`, `battery`.

`ws-<n>` is 1-based position, not the workspace id — the id is user-visible and
can be renamed without moving the node.

## Authority

None. The view has no WIT imports, no Unibus session, no filesystem, no clock,
no socket, no process. Every ambient fact arrives in the snapshot; the host
composes it from `soil:time.wall-clock`, `soil:device.battery` and `soil:shell`
(planned) and enforces the manifest grants. Logic that belongs to the platform
(workspace switching, hotkeys, occupancy) stays in the host or the compositor,
never here.

This is why `spore.toml` declares no `[module]` and no `[grants]`: a bar is a
projection of state the host already holds, so there is no guest ABI to widen.
The Lua entry *is* the entrypoint — a Spore does not need a wasm build to exist —
and Lua has no capability bindings yet, so a grant here would name a call the
view cannot make.

## Packaging

Divebar is a Lua-only Spore: `[ui] entry = "ui/divebar.lua"` is the program, and
`[chrome] regions = ["top-bar"]` says which slide of the shell it draws. The
region resolves to `[ui] entry`; `[chrome.sources]` exists only for a package
that ships a distinct artifact per region, and repeating one path twice would
only create two places to disagree.

```sh
# compose and assert the view (from a checkout of fungos-sdk)
tend check spores/divebar/ui/divebar.lua spores/divebar/ui/snapshot.lua

# load it as a package and render it (from this repo)
cargo run -p chrome-host -- spores/divebar --scene /tmp/bar.json

# lock and pack (from a checkout of soil)
soil-spore lock  spores/divebar
soil-spore check spores/divebar
soil-spore pack  spores/divebar -o /tmp/divebar-0.1.0.spore
```

The package ships both: `ui/divebar.lua` is what you edit, `dist/top-bar.lua` is
what a compositor loads. The artifact is composed (`require` inlined, no snapshot
baked) because a scene VM deletes the `require` global and a compositor must not
render a stale frame. `spore.lock` covers both, so they cannot drift apart.

Nothing composed is committed: `spore.lock` covers the Lua that ships, and
composed output is a host artifact, not package content.
