# Home — Lua API

`ui/home.lua` is the launcher: a status bar, four ambient widgets, a 6×N app
grid, a services rail and a dock. It is a **composition**, not a program. It
draws the host-owned snapshot and emits one intent; it never probes a service,
reads the store catalog or opens a path.

## Entrypoint

`ui/home.lua` exports:

- `M.render(view)` — builds a `fungos.ui` tree and presents it. A nil or partial
  `view` keeps the fields it has and fills the rest from `M.fixture()`.
- `M.fixture()` — the snapshot this view was designed against. Tests and the
  composer use it; the host replaces it with the live machine snapshot.

## Snapshot

```lua
{
  host = string,                  -- the machine's display name
  wallpaper = string?,            -- named wallpaper, when the host draws one
  apps = {
    { id = string, name = string, glyph = string?, host = string,
      category = string?, color = string?, state = string? },
  },
  services = {
    { name = string, status = "up" | "down" | string?, ms = integer?,
      host = string },
  },
  dock = { string },              -- app ids, in dock order
  widgets = {
    today = { time = string, date = string, day = string? },
    machine = { up = integer, total = integer, median = string?,
                down = string? },
    store = { spores = integer, source = string?, current = integer?,
              yanked = integer?, classes = { { name = string,
              count = integer, ink = string? } }? },
    system = { load = string?, memory = string?, disk = string?,
               battery = string? },
  },
}
```

A missing widget is drawn empty rather than dropped: the launcher keeps its
shape while the host is still probing. `apps` arrives already ordered; the view
paints that order and never re-sorts it.

## Actions

- `home.open` — open an app or Spore; payload `{ id, host }`. Every grid tile and
every dock item emits it. The view never resolves the host itself; the host
routes the intent through its launcher.

## Slots

None. This is a complete window rather than a contribution to another surface.

## Components

Built from the `fungos.ui` package in [`sdk/ui`](../../../sdk/ui/package.toml):
`ui.column`, `ui.row`, `ui.card`, `ui.text`, `ui.icon_button`, `ui.spacer`,
`ui.rule`, and `ui.action` for the one intent.

A row that mixes cards with text uses `widths`; a plain row gives a non-text
child the remaining width, which stretches a card. A card is a surface, not a
press region; the icon button is the control.

## Theme/config

The view names ink roles (`text`, `muted`, `primary`, `success`, `warning`,
`danger`, `info`) and text styles (`title`, `heading`, `body`, `caption`), never
a colour or a pixel. Spacing is `gap`/`pad` on the space scale.

## Stable ids

`home-root`, `home-bar`, `home-widgets`, `home-body`, `home-grid`,
`home-services`, `home-dock`. App tiles are `home-app-<id>` and their control is
`home-open-<id>`; dock controls are `home-dock-<id>`.

## Authority

`spore.toml` grants `soil:canvas` and nothing else. It has no host catalog,
service or filesystem grant: the snapshot is handed in, and opening an app is an
intent the host decides whether to honour.
