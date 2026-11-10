# Worklog Lua API

This package ships one Lua surface, `ui/worklog.lua`: a descending timeline of
normalized events.

It is a pure projection. It has no WIT imports, no producer, no filesystem, no
clock, and no network. The snapshot it paints is produced by the receiver in
`src/` and by `worklog render-lua`; see [`docs/worklog.md`](../../../docs/worklog.md)
for the event schema behind it.

## Entrypoint

`ui/worklog.lua` exports:

- `M.render(snapshot) -> nil` — draws through `canvas.rect` / `canvas.text`.
- `M.hit(spec)` and `M.pending_hits` — row hit targets; see Authority.
- `M.press(spec)` and `M.pending_presses` — the controls this render emitted,
  delivered to `widget.button` when the host has one.

The render also publishes what it planned, because the fixture and the host read
the same numbers the view decided: `M.row_height` (the pitch the page earned),
`M.max_row_height` (the view's own cap on it), `M.rows_drawn` and
`M.rows_planned` (equal unless the node budget stopped the draw), `M.window`
(`first`, `shown`, `total` — what a host needs to clamp a scroll), and `M.wide`
(whether the detail column exists at this width). Pure helpers used by review
aids: `M.chars_per_line(width, size)`, `M.summary_lines`, `M.locator_label(locator)`,
and `M.matches(event, needle)`.

The host supplies `module.viewport`, `module.theme`, and an optional
`module.event`, all read through `ui.viewport`, `ui.theme` and `ui.event`. It may
also supply `module.scroll = { offset }` in pixels, `module.hover` as the id of
the control under the pointer, and `module.query` as the search text. All three
are host state, not view state, because a module is re-executed for every render
and cannot remember any of them.

The view decides what a query *matches* — summary, actor, kind, source, subject,
id and every locator, case-insensitively — because that is a fact about the
events. The host owns the text, the caret and the keyboard.

## Snapshot

See `examples/worklog_mock.lua` for a complete fixture. The view reads:

```lua
{
  title: string?,                 -- defaults to "WORKLOG"
  limit: integer?,                -- rows the producer was willing to emit
  sort: string?,                  -- defaults to "Newest first"
  counts = { total: integer?, today: integer? }?,
  matches: integer?,              -- set by the view when a query filtered this
  sources = { { id: string, label: string?, count: integer? } }?,
  selected_id: string?,
  events = {
    { id: string, kind: string, source: string, actor: string,
      summary: string, subject: string,
      at: integer,                -- milliseconds since the epoch
      at_text: string,            -- ISO 8601, UTC
      age: string,                -- "now", "12m", "3h", "2d", "1w"
      selected: boolean?,
      links = { { rel: string, locator: string } }? },
  }?,
}
```

`events` arrives newest-arrival-first and already filtered (lease heartbeats are
dropped unless the producer was asked for them). The view paints that order and
never re-sorts: two opinions about order is how a timeline disagrees with
itself. `at_text` is printed and `age` is derived from it, so the reader is
never lied to about when something happened even though the row is ordered by
when it arrived.

A query narrows that list without touching it: the view copies the snapshot,
keeps the matching events, and writes `matches` on the copy, so `counts.total`
still says how big the log is and the header can say both. Dropping the query
returns the whole list, because nothing was ever removed from it.

## Actions

One intent: `worklog-open-locator`, carrying `{ locator, rel }`. Every locator a
row or the detail column shows is a `widget.button` over the label, so pressing
it hands the host the full URI and leaves opening it to whoever owns the scheme.
Nothing in this view writes: rows are addressable and selecting one changes the
detail pane, and that is all.

## Slots

None. The timeline is a complete view rather than a component contributed into
another window; there is no named region a host can replace.

## Components

`ui.surface` from `fungos.ui`, and nothing else. The view places its own pixels
through `canvas.rect` / `canvas.text`: Canvas has no layout service, and a flow
engine carried inside one Spore would be a second layout implementation with one
caller. A list, two panes and a toolbar are arithmetic.

Text is measured with `surface:measure_text`, so the width a line was wrapped
for and the width it is drawn at come from one function. A `canvas.text` carries
no width and the host neither wraps nor clips it, which is why the view emits one
node per line — see `docs/interaction-gap.md`.

## Theme/config

The view names roles, never colours: `text`, `muted`, `primary`, `surface` and
`background`. Type sizes, gaps, row pitch and pane width are all derived from the
host's `micro` / `small` / `body` / `large` in pixels — Canvas multiplies a text
node's scale by `theme.body`, so those are lengths it draws with, not hints. A
token that cannot be a font size falls back rather than shrinking the view.
Panel, selected-row and chip opacity come from `ui.profile.surfaces`. Per-app
overrides resolve under the app id `dev.fungos.worklog` via `ui.configure`.

## Stable ids

Row ids are positional and documented: `event-<index>` for a row,
`event-<index>-summary` for its text, `worklog-detail-*` for the detail pane, and
`event-<index>-*` for the fields inside a row. A host that hit-tests or
addresses a row uses `event-<index>`; the index is the position in the snapshot,
which is why the producer's order is load-bearing.

The search field is `worklog-search` (a hit target) with its text at
`worklog-search-value`. A host that owns a keyboard puts its own input over the
first and draws nothing itself: the caret is the only thing it adds. The rest of
the chrome is `worklog-title`, `worklog-sources`, `worklog-range`,
`worklog-today`, `worklog-sort`, and `event-hover-<index>` for the row under the
pointer.

## Authority

The view has no ambient authority. It cannot read a file, call a clock, open a
socket, or load code.

Interactive rows need `widget.hit`, which no host implements today: Canvas's
`widget` is the theming library and Soil exposes no hit targets. The view calls
it when a host provides it and otherwise keeps the spec in `M.pending_hits`, so
the row stays addressable instead of crashing the window. See
[`docs/interaction-gap.md`](../../unibus/docs/interaction-gap.md).
