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

Preview the UI without a compositor:

```sh
lua tools/preview.lua examples/inbox_snapshot.lua > /tmp/inbox.html
```
