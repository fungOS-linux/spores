# unibus Spore

Initial `soil:unibus` app. It consumes bounded mail/chat metadata from the
Unibus and stores it as `soil:records` entries so the inbox survives restarts
and syncs across devices.

- **Read-only.** No `soil:unibus.publish`; sending is a future, separate grant.
- **Metadata only.** `email.message.v1` / `chat.message.v1` bodies are metadata;
  text and attachments are references, not record bodies.
- **Records, not a second database.** Identity, schema, and body live in the
  Spore; durability, revisions, cursors, and tombstones live in the host.
- **Sync** is `changes`/`apply` with last-writer-wins, not a network call.

Build:

```sh
rustup target add wasm32-wasip2
cargo build -p spore-unibus --target wasm32-wasip2 --release
cp target/wasm32-wasip2/release/spore_unibus.wasm module.wasm
```

See [`docs/records.md`](../../docs/records.md) and
[`docs/ai-inbox-spec.md`](../../docs/ai-inbox-spec.md).

Status: **component built**, host capabilities pending. `module.wasm` is a
wasm32-wasip2 component importing `soil:records@0.1.0` and `soil:unibus@0.1.0`
and exporting `run` + `snapshot`. The manifest, UI, and module are the first
reviewable slice; a Soil host must implement the two capabilities before it can
run end-to-end.

Preview the new declarative inbox UI without a compositor:

```sh
lua tools/preview_inbox.lua examples/inbox_mock.lua > /tmp/inbox.html
```

The old node-tree preview remains at `tools/preview.lua` for the legacy view.
The new UI is `ui/inbox.lua` + `ui/lib/ui.lua`; the feature plan is
[`docs/inbox-ui-plan.md`](docs/inbox-ui-plan.md).

Run it in the current Canvas `spore-dev` workspace:

```sh
./tools/install_spore_dev_inbox.sh
```

The installer composes `spore-dev-inbox.{json,lua}` into the Canvas runtime
(under 32 KiB) and points the workspace surface at it. `spore-dev.lua` stays
owned by the markdown composer, so the two views do not overwrite each other.
Layout state supports `layout.rail_collapsed` and `layout.detail_collapsed`;
the header column/document affordances are the controls. Press routing is M2.

Preview with a real AgentMail inbox:

```sh
AGENTMAIL_API_KEY_FILE=~/agentmail_api_key_fungOS.txt \
  python3 tools/agentmail_snapshot.py /tmp/agentmail_snapshot.lua
lua tools/preview.lua /tmp/agentmail_snapshot.lua > /tmp/inbox.html
```

`tools/agentmail_snapshot.py` is a UI preview adapter, not a Soil host. It
fetches one inbox, maps unread/urgent state, and writes the Lua snapshot
`ui/inbox.lua` renders. It never prints the key.
