# CRM — Lua API

A pure projection over a host-provided CRM snapshot. The host owns data access,
namespaces and permissions; the view ranks and paints what it is given and
emits intents.

## Entrypoint

`ui/editor.lua` exports `M.render(ctx, snapshot)`, returning a `fungos.ui` tree.
`ui/snapshot.example.lua` is the fixture used by `tend check`.

## Snapshot

```lua
{
  mode = "accounts" | "pipeline"?,
  accounts = { { id, name, selected? } }?,
  account = { id, name, stage?, owner? }?,
  pipeline = { { id, name, stages = { { id, opportunities = { { id, name, amount } } } } } }?,
  events = { { id, at, kind, text } }?,
  selected = string?,   -- account or opportunity id
}
```

Optional fields collapse; an empty snapshot renders an empty state.

## Actions

| action | payload | meaning |
|---|---|---|
| `crm.account` | `{ id }` | open an account |
| `crm.opportunity` | `{ id }` | open an opportunity |

The view never mutates; the host applies the change and re-renders.

## Slots

None.

## Components

None of its own; it composes `fungos.ui` rows, columns and buttons.

## Theme/config

Semantic inks only, resolved by the host; text scale comes from the SDK theme.

## Stable ids

Account rows are `crm-account-<id>`, stages `crm-stage-<id>`, opportunity rows
`crm-opportunity-<id>`, and the event rail `crm-events`. These are stable across
renders.

## Authority

None. The Spore has no records or network access; every fact in the snapshot
was authorised by the host before it arrived.
