# Chrome — Lua API

Chrome is the system bar and its companion surfaces: the top bar, dock,
launcher, switcher, window frame and control centre. Each file is a scene
composer over the host's chrome record; none of them is a runtime.

## Entrypoint

| file | surface |
|---|---|
| `ui/bar.lua` | the 1200x44 bar; `M.render()` reads `_G.chrome` |
| `ui/top-bar.lua` | the top-bar region |
| `ui/dock.lua` | the dock region |
| `ui/launcher.lua` | the application launcher |
| `ui/switcher.lua` | window/workspace switcher |
| `ui/window.lua` | the window frame |
| `ui/control-center-v1.lua` | the control centre; `control-center-v1.json` is its fixture |

Every composer returns a `fungos.ui` tree. `M.name` and `M.version` mirror
`spore.toml`; hosts may assert they match.

## Snapshot

The bar reads one record, `_G.chrome`, supplied by the host:

```lua
{
  workspaces = { { id, focused, occupied } },
  focus = { app, title },
  clock = string?,          -- host-formatted, never a timestamp
  battery = { percent, charging }?,
}
```

Fields are optional; a missing cluster closes its gap instead of drawing a
placeholder. The record is assembled from `soil:time` and `soil:device`, plus
the `soil:shell` workspace/focus capability once it exists.

## Actions

None. Chrome v1 is read-only: focus follows the compositor, not the pointer.
Workspace focus is a host concern (`soil:shell.focus`, planned); when it lands
the action will be `chrome.focus` with `{ workspace_id: string }`.

## Slots

None. No region loader resolves slots yet; declaring `bar.center` here would
document a feature that cannot be filled. The three bar clusters (workspaces,
focus, ambient) become the first slot candidates when one does.

## Components

None of its own. The composers share `fungos.ui` scene primitives and the SDK's
`pill` / `ambient-fact` helpers rather than copying them per region.

## Theme/config

Semantic inks only — `background`, `surface`, `border`, `accent`, `text`,
`muted` — resolved by the host. Text scale comes from `ui.theme().body` inside
`fungos.ui`; no composer reads a pixel size from config.

## Stable ids

Region ids are stable: `top-bar`, `dock`, `launcher`, `switcher`, `window`,
`control-center`. Within the bar, `chrome-workspace-<id>`, `chrome-focus` and
`chrome-ambient` identify the three clusters for tests and hit-testing.

## Authority

None. No composer reads a clock, a file or a socket; `_G.chrome` is the only
input. The bar has no ambient authority, and a failed Spore load must leave the
host's fallback bar in place, never a blank desktop.
