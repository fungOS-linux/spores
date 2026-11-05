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

See [`docs/records.md`](../../docs/records.md) and
[`docs/ai-inbox-spec.md`](../../docs/ai-inbox-spec.md).

Status: scaffold. `module.wasm` is not built yet; the record-key/schema mapping
and manifest are the first reviewable slice.
