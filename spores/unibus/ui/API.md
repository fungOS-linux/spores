# Unibus Inbox Lua API

This package ships two Lua surfaces:

- `ui/inbox.lua` — mail inbox
- `ui/approval.lua` — agent approval decision window

Both are pure projections. They have no WIT imports, no Unibus producer, no
filesystem, no clock, and no network.

## Inbox entrypoint

`ui/inbox.lua` exports:

- `M.render(snapshot) -> Canvas scene`

The host supplies `module.viewport`, `module.theme`, and an optional
`module.event`.

## Inbox snapshot

See `examples/inbox_mock.lua` and `examples/inbox_live.lua` for complete
fixtures. The view reads:

```lua
{
  revision: integer,
  title: string,
  search: string?,
  sort: string?,
  account = { label: string?, email: string, provider: string?, sync: string? }?,
  counts = {
    unread: integer?,
    urgent: integer?,
    support: integer?,
    drafts: integer?,
  }?,
  selected_id: string?,
  layout = {
    list_offset: integer?,
    list_limit: integer?,
    cursor_id: string?,
    focus: string?,
    width_class: "narrow" | "wide"?,
  }?,
  accounts = {
    { id: string, label: string?, email: string?, provider: string?,
      unread: integer?, selected: boolean? },
  }?,
  inboxes = {
    { id: string, label: string, unread: integer?, count: integer?,
      selected: boolean? },
  }?,
  tags = {
    { id: string, label: string?, count: integer? },
  }?,
  threads = { Thread },
}

Thread = {
  id: string,
  collection: string?,
  kind: string?,
  title: string,
  sender: string?,
  preview: string?,
  brief: string?,
  time: string?,
  unread: boolean?,
  urgent: boolean?,
  selected: boolean?,
  status: string?,
  assignee: string?,
  priority: string?,
  labels: { string }?,
  draft: string?,
  messages: {
    { from: string, time: string, preview: string },
  }?,
}
```

The UI caps the visible list at 14 rows and treats `layout.list_offset` and
`layout.list_limit` as the page window.

## Inbox host events

| Event | Target | Meaning |
| --- | --- | --- |
| `activate` | `thread-<n>` | select the nth visible thread |

The view mutates only its local copy of the snapshot for selection. The Spore
owns durable read/selected state.

## Inbox actions

Declared Spore actions (see `spore.toml`):

| Action | Payload | Status |
| --- | --- | --- |
| `mail.read` | `{ thread_id: string }` | declared, not yet wired from Lua |
| `mail.unread` | `{ thread_id: string }` | declared, not yet wired from Lua |
| `mail.archive` | `{ thread_id: string }` | declared, not yet wired from Lua |
| `mail.tag` | `{ thread_id: string, tag: string }` | declared, not yet wired from Lua |

Until the action bridge lands, the inbox emits host activations, not action
envelopes. Canvas must not invent action semantics on its own.

## Inbox slots

| Slot | Replacement |
| --- | --- |
| `toolbar.start` | one row child |
| `toolbar.end` | one row child |
| `rail.header` | one rail child |
| `rail.footer` | one rail child |
| `detail.actions` | one detail child |
| `empty_state` | one detail child |

## Inbox stable ids

`inbox-root`, `inbox-toolbar`, `inbox-account`, `inbox-counts`,
`inbox-search`, `inbox-main`, `inbox-rail`, `inbox-list-pane`,
`inbox-detail-pane`, `mail-list`, `thread-<n>`, `detail-page`, `detail-head`,
`detail-title`, `detail-meta`, `detail-actions`, `detail-brief`.

`thread-<n>` is also the activation target. Other ids are presentation only
unless listed above.

## Approval entrypoint

`ui/approval.lua` exports:

- `M.render(snapshot) -> Canvas scene`

## Approval snapshot

```lua
{
  title: string?,
  to: string?,
  draft: string?,
  policy: string?,
}
```

## Approval controls

| Control | Intent |
| --- | --- |
| `approval-allow` | allow once |
| `approval-always` | always allow this action/surface |
| `approval-deny` | deny |

The approval surface currently draws the controls but does not emit action
intents. The host routes approval decisions and applies policy.

## Theme/config

Uses `module.theme` and the global `fungos.ui` profile. Per-app overrides are
not read by the view directly; the host/composer applies the resolved profile.

## Components

None. The inbox is an app view; reusable components are not exported yet.

## Authority

None in Lua. `mail.*` mutation is implemented by `module.wasm` over
`soil:records` with the grants declared in `spore.toml`.
