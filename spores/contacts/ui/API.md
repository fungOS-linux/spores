# Contacts Lua API

`ui/contacts.lua` is a pure projection over the contact snapshot. It has no WIT
imports, no Unibus session, no filesystem, no clock, and no network.

## Entrypoint

`ui/contacts.lua` exports:

- `M.render(snapshot) -> Canvas scene`

The host supplies `module.viewport`, `module.theme`, and an optional
`module.event`.

## Snapshot

```lua
{
  account = { label: string?, sync: string? }?,
  total: integer?,
  selected_id: string?,
  contacts = { Contact },
}

Contact = {
  id: string,
  name: string,
  org: string?,
  title: string?,
  emails: { string }?,
  phones: { string }?,
  addresses: { string }?,
  birthday: string?,
  note: string?,
  favorite: boolean?,
  tags: { string }?,
  last_contacted: integer?,
  updated_at_ms: integer?,
  created: string?,
  last_modified: string?,
  interactions: {
    {
      contact_id: string?,
      identity_kind: string?,
      identity: string?,
      source: string,
      kind: string,
      last_at_ms: integer,
      count: integer,
    },
  }?,
}
```

`contacts` is already filtered by the Spore logic (`snapshot(query)`). The view
does not search records.

## Host events

| Event | Target | Meaning |
| --- | --- | --- |
| `activate` | `contact-<n>` | select the nth visible contact |

The view updates only its local selected id. Durable query/selection belongs to
the Spore.

## Actions

None. Contacts is read-only. Query mutation requires a Canvas text-input event
and a Spore `query` action, neither of which is wired yet.

## Slots

None yet. The layout is narrow/wide responsive from `module.viewport`:

- narrow: header, selected detail, list
- wide (`>= 700px`): sidebar list + detail page

## Stable ids

`contacts-root`, `contacts-head`, `contacts-title`, `contacts-count`,
`contacts-subtitle`, `contacts-search`, `contacts-search-text`,
`contacts-section`, `contacts-list`, `contact-<n>`, `contacts-detail`,
`contacts-detail-name`, `contacts-detail-role`, `detail-avatar`.

`contact-<n>` is also the activation target.

## Components

None.

## Theme/config

Uses `module.theme` and the global `fungos.ui` profile. No app-specific keys yet.

## Authority

None in Lua. Reads are performed by `module.wasm` over `soil:records` with
`soil:records.read`.

The Spore also reads optional `contact_preference` and `contact_interaction`
collections. It has no mail, telephony, chat, or calendar capability. Connectors
write bounded records; see `docs/contact-connectors.md`.
