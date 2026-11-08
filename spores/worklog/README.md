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
  --github-secret-file ~/.config/fungos/github-webhook.secret
```

Point Marbles' webhook at `http://127.0.0.1:7420/ingest/marbles` and the GitHub
repository webhook (content type `application/json`, the `pull_request` event)
at `http://127.0.0.1:7420/ingest/github`. The receiver binds loopback only and
refuses to start without at least one secret: an unsigned webhook is not a
webhook.

Read it back:

```sh
./target/release/worklog tail --log ~/.local/share/fungos/worklog.jsonl --limit 20
./target/release/worklog render-lua --log ~/.local/share/fungos/worklog.jsonl > /tmp/worklog.lua
```

## See the view

```sh
lua tools/preview_worklog.lua                  # plain text, committed fixture
lua tools/preview_worklog.lua /tmp/worklog.lua # plain text, the real log

lua tools/compose_worklog.lua "$MODULES" /tmp/worklog.lua
```

`$MODULES` is the Canvas runtime modules directory
(`~/Library/Application Support/Dock/canvas/modules` on macOS). Composition
writes `spore-dev-worklog.{lua,json}`; it stays under Canvas's 32 KiB module cap
or fails at compose time with the size, not at install time with a blank window.

Placing the surface is a separate step and a layout decision — a third pane
alongside the inbox and contacts is a product call, not an installer default.

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
