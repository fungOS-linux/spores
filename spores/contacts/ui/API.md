# Contacts — Lua API

A pure projection over a records-backed contacts view. Contacts are personal;
sharing is explicit and projects only a first name and a token.

## Entrypoint

`ui/editor.lua` exports `M.render(ctx, view)`, which returns a `fungos.ui` tree.
`ui/view.example.lua` is the fixture used by `tend check`.

```lua
local view = dofile("ui/editor.lua")
local tree = view.render(ctx, snapshot)
```

## Snapshot

```lua
{
  contacts = { { id, name, token, selected? } },
  contact = { id, name, shares = { { account_id } }? }?,
  selected = string?,   -- id of the open contact
  shared = { { account_id } }?,
}
```

Every field is optional; an empty view renders an empty list with a hint, not
an error.

## Actions

| action | payload | meaning |
|---|---|---|
| `contacts.select` | `{ id }` | open a contact |
| `contacts.unshare` | `{ account_id, contact_id }` | revoke one share |

The view emits intents only; the host performs the records write and re-renders.

## Slots

None.

## Components

None of its own; it composes `fungos.ui` list rows and buttons.

## Theme/config

Semantic inks only (`background`, `surface`, `border`, `accent`, `text`,
`muted`) resolved by the host. No colour or pixel size is named in the view.

## Stable ids

Contact rows are `contacts-row-<id>`; the detail pane is `contacts-detail`; the
share rows are `contacts-share-<account_id>`. Tests and hit-testing rely on
these, so they must not change with the data.

## Authority

None. The view never reads `soil:records`; the host supplies the snapshot and
enforces the personal/shared boundary. A share is only visible because the host
put it in the snapshot.
