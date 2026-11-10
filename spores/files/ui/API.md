# files Lua API

This package ships one Lua surface, `ui/files.lua`: a three-pane browser over a
VFS namespace plus a storage census. It is a pure projection. It has no WIT
imports, no filesystem, no clock, no network, and no bytes — it counts entries
the host already listed.

## Entrypoint

`ui/files.lua` exports:

- `M.render(snapshot) -> nil` — draws through `canvas.rect` / `canvas.text`.
- `M.press(spec)` and `M.pending_presses` — the controls this render emitted,
  delivered to `widget.button` when the host has one. A press carries an
  `action` and a `body`; the host routes both, the view does not.
- `M.row_height` (the pitch this frame earned), `M.rows_drawn`, `M.rows_planned`
  (equal unless the bottom edge stopped the draw), and `M.wide` (whether the
  inspector is present at this width).
- Pure helpers a review aid can call: `M.format_bytes(bytes)`,
  `M.format_count(value)`, `M.chars_per_line(width, size)`.

The host supplies `module.viewport` (pixels) and `module.theme` (the type scale
it draws with), read through `ui.viewport` and preferred over `ui.theme()`'s
profile defaults. It may supply `module.hover` as the id of the control under the
pointer. All three are host state, not view state: a module is re-executed for
every render.

## Snapshot

See `examples/files_mock.lua` for a complete fixture. The view reads:

```lua
{
  revision = 1,
  title = "files",
  workspace = "Studio",            -- display name of the selected location
  root = "studio",                 -- its id, matching one roots entry
  path = "Photos/2024",            -- location-relative path shown; "" is the root
  updated_ms = 1700000000000,      -- when the host scanned
  crumbs = { { name = string, path = string } },
  roots = {
    { id = string, name = string, bytes = integer, files = integer,
      folders = integer, current = boolean,
      available = boolean,       -- false: a background scan has not finished
      errors = integer, truncated = boolean },
  },
  totals = { bytes = integer, files = integer, folders = integer,
             scanned = integer, errors = integer, truncated = boolean },
  kinds = {
    { id = string, label = string, bytes = integer, files = integer },
  },
  listing = {
    { name = string, path = string, kind = "file" | "directory",
      bytes = integer, items = integer?,        -- items only for a directory
      media_type = string, modified = string }, -- ISO 8601 date, or ""
  },
  listing_total = integer,        -- children before truncation
  listing_truncated = boolean,
  largest = { { name = string, path = string, bytes = integer } },
  error = string?,                -- the host could not read this directory
}
```

A root with `available = false` is drawn as "scanning" with an em dash for its
size. A root with `errors > 0` or `truncated` says "partial" (or "unreadable"
when nothing at all could be read) in the warning role, so the rail never
presents a denied or bounded count as a complete one. It is not an empty location; the host has simply not counted it yet.

`kinds` arrives sorted by bytes, largest first; the view paints that order and
re-sorts nothing. The listing arrives directories-first, then case-insensitive
name; the view windows it to the rows the window holds. `totals` always describe
the whole location, not the open folder, so the inspector does not swing as the
reader navigates.

`bytes` is the unit of truth. The view formats them (`12.4 GB`); a second
opinion about size is how two panes start disagreeing about one disk. Dates are
formatted by the host because a view has no clock.

## Actions

- `files-open`, body `{ root = string, path = string }` — open a location or a
  directory. A row is pressable only when it is a directory; this Spore opens no
  file, because that is a different grant.
- `files-up`, body `{ root = string, path = string }` — the breadcrumb. The body
  names the parent directory, so the host does not have to re-derive it.

Both are intents addressed to `dev.fungos.files`. The host decides how to route
one: `files-serve` re-scans the requested directory and returns a new scene.

## Slots

None. The browser is a complete view rather than a component contributed into
another window.

## Components

`ui.surface` from `fungos.ui`, and nothing else. The view places its own pixels
through `canvas.rect` / `canvas.text`: Canvas has no layout service, and a flow
engine carried inside one Spore would be a second layout implementation with one
caller. A masthead, a rail, a list and an inspector are arithmetic.

Text is measured with `surface:measure_text`, so the width a line was wrapped for
and the width it is drawn at come from one function. A `canvas.text` carries no
width and the host neither wraps nor clips it, which is why the view emits one
node per line and clips names itself.

## Theme/config

The view names roles, never colours: `text`, `muted`, `primary`, `success`,
`warning`, `danger`, `info`, `surface`, `background`. Type sizes, gaps, row pitch
and pane widths derive from the host's `micro` / `small` / `body` / `large` in
pixels. Panel and selected-row opacity come from `ui.profile.surfaces`. Per-app
overrides resolve under the app id `dev.fungos.files` via `ui.configure`.

Three panes appear at 980 px and wider; the rail alone appears at 760 px and
wider; below that the list takes the surface and the totals collapse to one line
in the masthead.

## Stable ids

- `root-<index>` — a location row; `root-<index>-name` / `-size` / `-files` for its text.
- `crumb` and `crumb-up` — the breadcrumb and its way up.
- `row-<index>` — a listing row; `row-<index>-name` / `-size` / `-date`.
- `inspect-*`, `kind-band-<index>`, `legend-<index>`, `largest-<index>` — the
  census, its bands, its legend, and the largest files.
- `mast-title`, `mast-summary`, `rail-label`, `rail-foot`, `empty`, `error`.

The index is the position in the snapshot, which is why the producer's order is
load-bearing.

## Authority

The view has no ambient authority. It cannot read a file, call a clock, open a
socket, or load code. It receives a snapshot and returns nodes and intents.

Press regions go through `widget.button`; when a host does not implement it the
spec is kept in `M.pending_presses`, so a row stays addressable instead of
crashing the window. `tools/shot.lua` records those regions into the scene
document, which is what lets the browser page (and any other host that reads the
document) route a press back to `files-serve`.
