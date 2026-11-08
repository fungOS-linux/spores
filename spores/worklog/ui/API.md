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
- `M.tree(snapshot) -> node` — the node tree, without drawing. Review aids and
  fixtures walk this instead of re-deriving the structure.
- `M.hit(spec)` and `M.pending_hits` — row hit targets; see Authority.

The host supplies `module.viewport`, `module.theme`, and an optional
`module.event`.

## Snapshot

See `examples/worklog_mock.lua` for a complete fixture. The view reads:

```lua
{
  title: string?,                 -- defaults to "WORKLOG"
  limit: integer?,                -- rows the producer was willing to emit
  sort: string?,                  -- defaults to "Newest first"
  counts = { total: integer?, today: integer? }?,
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

## Actions

None. The window is read-only: rows are addressable and selecting one changes
the detail pane, but nothing in this view writes. Opening a locator belongs to
the host, which owns the scheme.

## Slots

None. The timeline is a complete view rather than a component contributed into
another window; there is no named region a host can replace.

## Components

The view uses the shared `fungos.ui` node kinds — `container`, `text` — and
nothing else. It declares structure and copy; the host resolves geometry. When
Canvas provides no layout service the view falls back to `ui.layout` from the
shared kit, which is inlined at compose time (see
[`tools/compose_lua.lua`](../../../tools/compose_lua.lua)).

## Theme/config

The view names roles, never colours: `text`, `muted` and `primary` only. Canvas
rejects ink roles it does not know, so the two surface roles the theme documents
(`surface`, `background`) are mapped to `muted` in one place at the top of the
file rather than spelled out at each call site. Per-app overrides resolve under
the app id `dev.fungos.worklog` via `ui.configure`.

## Stable ids

Row ids are positional and documented: `event-<index>` for a row,
`event-<index>-summary` for its text, `worklog-detail-*` for the detail pane, and
`event-<index>-*` for the fields inside a row. A host that hit-tests or
addresses a row uses `event-<index>`; the index is the position in the snapshot,
which is why the producer's order is load-bearing.

## Authority

The view has no ambient authority. It cannot read a file, call a clock, open a
socket, or load code.

Interactive rows need `widget.hit`, which no host implements today: Canvas's
`widget` is the theming library and Soil exposes no hit targets. The view calls
it when a host provides it and otherwise keeps the spec in `M.pending_hits`, so
the row stays addressable instead of crashing the window. See
[`docs/interaction-gap.md`](../../unibus/docs/interaction-gap.md).
