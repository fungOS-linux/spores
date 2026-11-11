# Search launcher — Lua API

Quick search: one query line and a ranked list of hits. It is a pure projection
over the host's search results; it does not query an index itself.

## Entrypoint

`ui/launcher.lua` exports `M.render(snapshot)`, returning a `fungos.ui` tree. It
resolves `_G.ui` (composed by `tend`) and asserts when the library is absent, so
a bare `lua` run fails loudly instead of drawing nothing.

```lua
local view = dofile("ui/launcher.lua")
local tree = view.render({ query = "crm", hits = { ... } })
```

## Snapshot

```lua
{
  query = string?,
  hits = { { id, title, subtitle?, icon?, kind? } }?,
}
```

`kind` is the protocol's word for the hit kind (`application`, `file`, …); the
view maps it to a label next to the title. An empty hit list renders the query
line and a hint.

## Actions

None yet. Opening a hit is the host's selection event; when an action router
lands it will be `search.open` with `{ id: string }`. The view does not navigate.

## Slots

None.

## Components

None of its own; it composes `fungos.ui` rows and text.

## Theme/config

A `DEFAULT_THEME` (`body`, `title`, `large`, `micro`, `radius`) is used when
`module.theme` is absent; the host theme wins when present. Inks are semantic
names, never colour values.

## Stable ids

Hit rows are `hit-<id>` (falling back to `hit-<index>` for an id-less hit); the
query line is `search-query`. Row ids must not change as the ranking moves.

## Authority

None. The view never queries `soil:index` or the network; the host supplies the
hits it is allowed to show.
