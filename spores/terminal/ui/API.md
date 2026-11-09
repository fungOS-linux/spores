# Terminal — Lua API

The terminal Spore has two views over one frame the host owns. The host runs the
pty, the SSH client and the VT parser; neither view reads a socket, a process or
a key.

- `ui/grid.lua` — the terminal itself: one cell grid, painted.
- `ui/terminal.lua` — the status/fallback projection: Mycelium targets and the
  latest plain-text frame, for a surface that cannot draw a grid.

## Entrypoint

Both export `M.render(snapshot)` and paint through `ui.surface`; neither returns
a tree. `ui/grid.lua` also exports `M.cell(cols, rows, width, height)`, the cell
arithmetic it drew with, and sets `M.meta` for the page that renders the scene.

`ui/snapshot.example.lua` is the fixture used by `tend check`.

```lua
local grid = dofile("ui/grid.lua")
grid.render(snapshot)      -- paints the grid; grid.meta describes the frame
```

## Snapshot

`ui/grid.lua`:

```lua
{
  cols = integer, rows = integer,
  title = string?,
  cursor = { x = integer, y = integer, visible = boolean? }?,
  scroll = { offset = integer, total = integer, len = integer }?,
  rows = {
    {
      y = integer,
      runs = { { text = string, fg = "#rrggbb"?, bg = "#rrggbb"?, flags = integer? } },
    },
  },
}
```

`flags` is a bit set: `1` bold, `2` italic, `4` underline, `8` faint, `16`
inverse. A missing `fg`/`bg` means the terminal default, which is the theme's
text and surface.

`ui/terminal.lua`:

```lua
{
  targets = { { id, label?, kind?, health?, shell? } }?,
  terminal = { title?, text? }?,
}
```

## Actions

None. Opening, resizing and closing a session are host operations under the
`soil:mycelium.shell` and `soil:terminal` grants; keys reach the host as pty
bytes, not as intents.

## Slots

None.

## Components

None of its own; `ui/grid.lua` uses `ui.surface` primitives directly because a
grid is the one layout that must not be negotiated.

## Theme/config

The theme supplies the surface, the default text ink and the cursor ink. Cell
colours are literal: sixteen ANSI colours are not sixteen theme roles, and a
terminal that ignores the program's colour is not the program's terminal.

## Stable ids

`terminal-bg`, `terminal-cursor`, `cell-<row>-<run>`, `cell-bg-<row>-<run>`,
`cell-ul-<row>-<run>`, `cell-inv-<row>-<run>`. Row and run indexes are stable
for a frame; a repaint renumbers nothing.

## Authority

None. The view sees a grid the host already parsed and a directory the host
already authorized.
