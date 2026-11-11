# worklog

One descending timeline of what happened across tools: Marbles lifecycle
events, GitHub merges, and (next) thread mentions. Read-only.

```text
marbles webhook ─┐
github webhook  ─┼─▶ worklog (receiver) ─▶ log ─▶ render-lua ─▶ ui/worklog.lua
```

The schema is [`fungos.worklog/event@1`](../../docs/worklog.md). This Spore is
the read-only projection of it; the receiver is a separate native process,
because it owns a socket and a file and no Spore is allowed to own either.

## Why two halves

A Spore runs in a sandbox with no network and no filesystem. Accepting a webhook
is exactly the authority a Spore must not have, so the receiver is a host-side
binary and the Spore holds only `soil:records.read` and `soil:canvas`. That grant
split is the security property: nothing in the window can forge a row, so a row
means the receiver accepted a signed delivery for it.

## Run the receiver

```sh
cargo build -p spore-worklog --release

# One payload from stdin, with the signature as a header value:
./target/release/worklog ingest --source marbles \
  --secret-file ~/.config/fungos/marbles-webhook.secret \
  --signature "sha256=$(openssl dgst -sha256 -hmac "$(cat ~/.config/fungos/marbles-webhook.secret)" -hex | sed 's/.* //')" \
  --log ~/.local/share/fungos/worklog.jsonl < delivery.json

# Or as a receiver:
./target/release/worklog serve \
  --bind 127.0.0.1:7420 \
  --log ~/.local/share/fungos/worklog.jsonl \
  --marbles-secret-file ~/.config/fungos/marbles-webhook.secret \
  --github-secret-file ~/.config/fungos/github-webhook.secret \
  --confluence-secret-file ~/.config/fungos/confluence-webhook.secret
```

Point Marbles' webhook at `http://127.0.0.1:7420/ingest/marbles`, the GitHub
repository webhook (content type `application/json`, the `pull_request` event)
at `http://127.0.0.1:7420/ingest/github`, and Confluence's space webhook at
`http://127.0.0.1:7420/ingest/confluence`. The receiver binds loopback only and
refuses to start without at least one secret: an unsigned webhook is not a
webhook.

## Confluence

Confluence Cloud signs with an HMAC over the raw body, so the same check the
other two use applies; only the route and the payload shape differ. Both shapes
Confluence sends are read: the classic space webhook (`page_created`,
`page_updated`, `page_removed`, `comment_created`, `attachment_created`) which
carries `page`, and the newer content events (`content_created`,
`content_updated`, `content_deleted`) which carry `content`.

The event lands as `confluence:<space>/<content-id>`:

```text
page_updated  FPL/948231  "Confluence webhook ingress architecture"
              -> changed   updated: Confluence webhook ingress architecture
              -> chip      FPL#948231
comment_created
to the same page  -> mention  commented on: <page title>
```

The space is what a reader recognises and the id is what survives a rename, so
the locator carries both and the title stays in the summary where it renders.
The page version is part of the event id, so three edits of one page are three
events and a redelivery of the second is still the second.

An event name this receiver does not know is rejected with a 400 rather than
stored as something generic: a wiki move is not an edit, and a row that says
"changed" for it would be a lie the reader cannot see through.

Replay one delivery by hand (the payload shapes live in `examples/`):

```sh
./target/release/worklog ingest --source confluence --allow-unsigned \
  --log ~/.local/share/fungos/worklog.jsonl \
  < examples/confluence-page-updated.json
```

Read it back:

```sh
./target/release/worklog tail --log ~/.local/share/fungos/worklog.jsonl --limit 20
./target/release/worklog render-lua --log ~/.local/share/fungos/worklog.jsonl > /tmp/worklog.lua
```

## See the view

```sh
lua tools/preview_worklog.lua                  # plain text, committed fixture
lua tools/preview_worklog.lua /tmp/worklog.lua # plain text, the real log

# A picture without a compositor: the same primitives Canvas is handed, drawn to
# SVG at the size the wall gives this surface. Geometry, density and colour roles
# are real; the type sizes come from the theme, so the picture is at the size
# Canvas draws. Only the font face is approximated.
lua tools/render_worklog.lua /tmp/worklog.lua /tmp/worklog.svg
rsvg-convert -w 1182 /tmp/worklog.svg -o /tmp/worklog.png

lua tools/compose_worklog.lua "$MODULES" /tmp/worklog.lua

# Iterate without shipping: tend re-composes on save and reloads the surface.
lua <sdk>/tools/tend.lua watch ui/worklog.lua /tmp/worklog.lua \
  --surface worklog --name spore-dev-worklog

# Do not ship what `tend` composes for this view. Its module resolves
# `require("fungos.ui")` during preflight but not when Canvas loads it, and the
# surface dies with "attempt to index a nil value (global 'ui')". This composer
# hands the view a `require` whose one entry is the SDK library, which has no
# such step.
```

`$MODULES` is the Canvas runtime modules directory
(`~/Library/Application Support/Dock/canvas/modules` on macOS). Composition
writes `spore-dev-worklog.{lua,json}`; it stays under Canvas's 32 KiB module cap
or fails at compose time with the size, not at install time with a blank window.

Placing the surface is a separate step and a layout decision — a third pane
alongside the inbox and contacts is a product call, not an installer default.

## In a browser

`worklog-serve` is the same view, in a browser window. It is not a second
renderer: the page draws the scene document that `tools/shot.lua` produces by
running `ui/worklog.lua`, so the browser and the wall cannot disagree about what
the log looks like. The renderer runs once per snapshot change and per window
size, not per request.

```sh
cargo build -p spore-worklog --bin worklog-serve
./target/debug/worklog-serve \
  --sdk "$FUNGOS_SDK" \
  --snapshot /tmp/worklog.lua    # or examples/worklog_mock.lua
```

The desktop gateway routes `worklog.myc` to it:

```json
{ "name": "worklog", "hosts": ["worklog.myc"],
  "upstream": "http://127.0.0.1:8795", "priority": 10 }
```

in `~/.mycelium/gateway/routes.json`; `*.myc` already resolves to loopback through
`/etc/resolver/myc`, so the route is the whole of the DNS story. The host answers
`GET /` (the page) and `GET /scene.json?w=&h=` (the scene at that window size).

## Tests

```sh
cargo test -p spore-worklog          # normalizer, ordering, dedupe, signatures
lua tests/ui_fixture.lua             # the view against the committed snapshot
```

## Known gaps

- **The transport is provisional.** Envelope, cursor, retention and delivery are
  `soil:unibus`'s job; this crate reimplements them only because no Soil host
  exists yet. See "Transport is Unibus's job" in [`docs/worklog.md`](../../docs/worklog.md)
  for the split that should replace it.
- **Marbles rows are terse.** The `marbles.event/1` envelope carries no actor and
  no detail text, so a row reads `closed spores/spores-42` attributed to
  `marbles`. `docs/worklog.md` names the exact Marbles change that fixes it; the
  ingester already accepts the fields.
- **The log is a file.** It stands in for `soil:records` (collection
  `worklog.event`) until a Soil host implements records. Same body, inspectable
  with `grep` in the meantime.
- **`scan` is a full read.** Fine at thousands of events, wrong at millions.
- **Nothing consumes locators yet.** A row names `marbles://...` or
  `github:owner/repo#1`; opening one is the host's job and is not wired.
