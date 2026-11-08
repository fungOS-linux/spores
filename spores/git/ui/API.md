# Git landing queue — Lua API

`ui/review.lua` is a read-only list of finished work waiting on a merge
decision. It is a pure projection: it ranks what the snapshot gives it, explains
why each row is there, and emits intents. It does not merge, close, or reach a
forge.

The problem it answers is narrow on purpose. On this tracker, 45 issues sat in
`review` with a median age of twelve days while the portfolio view reported them
only as a count. A review item is not a backlog item — the work is done, and the
cost of waiting is the branch drifting under it.

## Entrypoint

`ui/review.lua` exports `M.render(ctx, view) -> node`, where `node` is a
`fungos.ui` tree. `ctx` is unused in v0 and present so the signature matches the
shared Spore views.

## Snapshot

```lua
{
  title = string?,              -- defaults to "REVIEW"
  query = string?,              -- current filter text, if any
  selected_id = string?,        -- the row the host is focused on
  counts = { waiting = integer?, merged = integer?, no_pr = integer? }?,
  projects = { { id = string, label = string?, count = integer?, selected = boolean? } }?,
  rows = {
    { id = string, title = string, project = string,
      age = string,             -- "13d"; formatted by the producer
      marble = string?,         -- issue id, for the open action
      selected = boolean?,      -- producer-set selection; `selected_id` also works
      pr = { repo = string, number = integer,
             state = "open" | "merged" | "closed",
             draft = boolean?, url = string? }? },
  }?,
}
```

`rows` arrives already ordered by the producer. The view paints that order and
never re-sorts: ordering by cost of waiting (merged first, then oldest) is a
judgement about work, and two places making it is one too many.

A row with no `pr` is not an error and is not hidden. It means the work is in
`review` with nothing to land, which is a decision nobody has made yet — so the
row says `no pull request` and offers no land button.

## Actions

All four are intents. Nothing in this view performs an effect.

- `git.select` — the row was activated; payload `{ id }`.
- `git.land` — land the row's pull request (merged) or review it (still open); payload `{ id }`.
- `git.open_marble` — open the underlying issue; payload `{ id }`.
- `git.filter` — filter to a project; payload `{ project }`.

The land intent is the consequential one. The host executes it, which is where
merge authority lives; a window that could merge would be a second authority
over the work graph.

## Slots

None. This is a complete view rather than a contribution into another window.

## Components

Built from `fungos.ui`: `ui.row`, `ui.column`, `ui.text`, `ui.badge`,
`ui.button`, `ui.list_row`, `ui.sidebar`, `ui.sidebar_group`, `ui.command_bar`,
`ui.section_label`, `ui.empty_state`, and `ui.push`. No component is reimplemented
locally, so a change in the shared kit moves this view with everything else.

## Theme/config

The view names ink roles only — `text`, `muted`, `primary` — and never a colour.
Sizes are numbers because `fungos.ui` takes literal text sizes; spacing and
radius come from the kit's defaults rather than being restated here.

## Stable ids

`git-root`, `git-side`, `git-side-projects`, `git-main`, `git-head`, `git-title`,
`git-counts`, `git-bar`, `git-section`, `git-empty`.

Rows are positional: `git-row-<n>`, with `-text`, `-title`, `-meta`, `-age`,
`-state`, `-land`, `-marble` inside. The index is the position in `rows`, which
is why the producer's order is load-bearing.

## Authority

The view has no ambient authority: no network, no filesystem, no clock, no
process. `spore.toml` denies `soil:http`, `soil:records.write` and transport,
so v0 cannot learn a pull request's real state on its own — that arrives in the
snapshot — and cannot land anything even if it wanted to.
