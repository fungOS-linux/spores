# Spore Store — Lua API

`ui/store.lua` is the storefront: categories, featured cards, a listing table,
one listing at a time, and a classified update queue. It is a **composition**,
not a program. It draws what the snapshot gives it and emits intents; it cannot
fetch an index, install a package, or change a grant.

The slice boundary is deliberate. The store's authority to install is a host
capability (`soil:spore.install`, see `docs/spore-store.md` in
`fungOS-linux/soil`); a window that could install would be a second authority
over what runs on the machine. What ships here is the surface that asks.

## Entrypoint

`ui/store.lua` exports:

- `M.render(view)` — builds a `fungos.ui` tree and presents it. A nil or partial
  `view` keeps the fields it has and fills the rest from `M.fixture()`.
- `M.fixture()` — the snapshot this view was designed against. Tests and the
  composer use it; the module replaces it with the signed index and does not
  edit the view.

## Snapshot

```lua
{
  route = "browse" | "detail" | "updates",   -- defaults to "browse"
  source = { name = string, index = "ipfs://…", verified = boolean },
  query = string?,                 -- current filter text, drawn when present
  selected_id = string?,           -- what "detail" opens
  selected = table?,               -- a full app record; wins over selected_id
  selected_update = table?,        -- class/reason for the selected app
  categories = {
    { id = string, label = string, count = integer?, selected = boolean? },
  },
  apps = {
    { id = string, name = string, publisher = string, category = string,
      version = string, rating = number?, ratings = integer?,
      badge = string, color = string?,
      state = "get" | "installed" | "update",
      summary = string, description = string?, license = string?,
      homepage = string?, digest = string?,
      screenshots = { { caption = string, cid = string? } }?,
      grants = { { grant = string, reason = string } }?,
      histogram = { integer }?,   -- 5..1 stars, in that order
      reviews = { { author = string, rating = integer, verified = boolean,
                    age = string, body = string } }? },
  },
  updates = {
    { id = string, name = string, from = string, to = string,
      class = "live" | "restart" | "consent" | "system" | "hold",
      reason = string, digest = string? },
  },
  pending = { total = integer?, consent = integer?, reboot = integer?,
              hold = integer? },
}
```

`apps` arrives already filtered, ordered and rated. The view paints that order
and never re-sorts: ranking is a judgement about a catalog, and two places
making it is one too many.

`state` is what the store knows about *this machine*, not about the app. A row
with `state = "update"` has a newer digest in the index; a row with `"get"` is
not installed; `"installed"` means the digest the store would offer is the one
running.

## Actions

All ten are intents. Nothing in this view performs an effect, and no intent is
sent until the host routes it.

- `store.category` — show a category; payload `{ category }`. The header's
  Updates button and the sidebar share this intent.
- `store.select` — open a listing; payload `{ id }`. Emitted by featured cards
  and by every listing row.
- `store.back` — return to the previous list; payload `{}`.
- `store.install` — install the selected listing; payload `{ id, digest }`.
  The consequential one: the host fetches, verifies, asks for consent and only
  then writes the package.
- `store.open` — launch an installed Spore; payload `{ id, digest }`.
- `store.update` — apply one update; payload `{ id, digest }`. A `consent` or
  `system` update stays pending until the host asks; this intent does not imply
  approval.
- `store.update_all` — apply everything that can be applied without a person;
  payload `{}`. The host is expected to skip `consent`, `system` and `hold`.
- `store.unpin` — release a held package so it can follow the source again;
  payload `{ id, digest }`.
- `store.review` — open the review composer for a listing; payload
  `{ id, digest }`. A review requires a host attestation that the package is
  installed, so the composer is the host's, not a text box here.
- `store.source` — switch the active source; payload `{ name }`.

## Slots

None. This is a complete window rather than a contribution to another surface.

## Components

Built from the `fungos.ui` package in [`sdk/ui`](../../../sdk/ui/package.toml):
`ui.column`, `ui.row`, `ui.card`, `ui.text`, `ui.button`, `ui.icon`, `ui.kbd`,
`ui.rule`, `ui.fill`, `ui.table`, `ui.bullets`, and `ui.action` for intents.

Two layout rules are load-bearing and easy to break:

- A row that mixes a card, an icon or a button with text uses `widths`. A plain
  row gives a non-text child the remaining width, so a card in one stretches.
- A card does not register a press region. Anything selectable is a `button` or
  a `table` row, which the host owns.

Screenshots are drawn as a frame plus the caption, because the library has no
image node yet. The frame is a `ui.fill` and the caption is the alt text every
listing must carry; when an image node lands, only `screenshots()` changes.

## Theme/config

The view names ink roles (`text`, `muted`, `primary`, `success`, `warning`,
`danger`, `info`) and text styles (`title`, `heading`, `body`, `caption`), never
a colour or a pixel. Spacing is `gap`/`pad` on the space scale. Update classes
map to ink in one table (`CLASS`), so "held" cannot accidentally read as
success.

## Stable ids

`store-root`, `store-head`, `store-title`, `store-source-line`, `store-sources`,
`store-updates`, `store-search`, `store-search-text`, `store-body`, `store-side`,
`store-main`, `store-main-title`, `store-main-count`, `store-featured`,
`store-list`, `store-foot`, `store-foot-text`, `store-detail`, `store-back`,
`store-install`, `store-write-review`, `store-shots`, `store-grants`,
`store-histogram`, `store-reviews`, `store-updates-title`, `store-update-all`,
`store-pending`, `store-update-apply-<n>`.

Rows are positional: `store-card-<id>`, `store-row-<id>`, `store-cat-<id>`,
`store-update-<id>`, with `-text`, `-name`, `-rating`, `-class` where needed.

## Authority

`spore.toml` grants `soil:canvas` and nothing else. It denies
`soil:spore.catalog`, `soil:spore.install` and `soil:http`, so this package
cannot read the index, install anything or reach a network even if the view were
compromised. The next revision swaps those denials for grants when the module
that uses them exists.
