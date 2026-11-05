# Search UI API

## Entrypoint

`ui/search.lua` returns one module with `render(snapshot)`.

Canvas supplies `module.viewport`, `module.theme`, `module.event`,
`module.view.selected`, and `module.select`. The view has no `os`, `io`,
`require`, network, or filesystem access.

## Snapshot

The Rust Spore emits one bounded snapshot:

```lua
{
  title = "Search",
  query = "",
  workspace = "vfs",
  mode = "browse" | "search" | "not-ready",
  ready = true,
  generation = 3,
  total = 12,
  error = nil,
  selected_path = "notes/trip.md",
  results = {
    {
      path = "notes/trip.md",
      name = "trip.md",
      media_type = "text/markdown",
      size = 2048,
      kind = "file",
      is_dir = false,
      snippet = "Athens and Nafplio",
      score = 12,
      matched_by = "text",
    },
  },
  detail = {
    path = "notes/trip.md",
    preview = "bounded text preview",
  },
}
```

`detail` is authoritative only for `detail.path`. The view must not show it for
another result.

## Actions

The view emits local `widget.hit` targets only:

- `result-N` selects the Nth result in the current filter.
- `filter-all`, `filter-files`, `filter-folders` change the local filter.

These are view actions, not durable Spore actions. No action leaves the process.

## Stable IDs

- `search-title`, `search-count`, `search-subtitle`, `search-query`
- `search-status`, `search-error`
- `filter-<id>`, `filter-<id>-label`
- `result-<n>`, `result-<n>-name`, `result-<n>-meta`, `result-<n>-snippet`
- `detail-name`, `detail-meta`, `detail-path`, `detail-preview`,
  `detail-preview-empty`, `detail-empty`

## Components

The view composes a sidebar, a result list, and a detail pane. Wide surfaces use
three columns; narrow surfaces stack. It uses host layout through `canvas.layout`
and never guesses text width.

## Theme And Config

All colors come from semantic theme inks: `background`, `surface`, `text`,
`muted`, `primary`. The view uses `module.theme.body` for type scale and
`module.viewport` for layout.

## Authority

The view owns presentation and local selection/filter memory only. The host owns
VFS paths, index generation, preview bytes, text input, window geometry, and hit
testing. Query text arrives in the snapshot; this view cannot read the VFS.
