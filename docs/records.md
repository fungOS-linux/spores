# Records: storage and sync for Soil apps

Status: draft contract [`soil:records@0.1.0`](https://github.com/fungOS-linux/soil/blob/main/wit/soil-records.wit)
plus the first consumer, the `dev.fungos.inbox` Spore in
[`spores/unibus`](../spores/unibus).

## Why records

A Spore cannot keep mail/chat metadata in module memory: state must survive
restarts and sync across the user's devices. Records are the host-owned,
schema-tagged store for that state. They are not a second message bus and not a
document format.

| Concern | Owner |
| --- | --- |
| Durable bytes, revisions, cursors, tombstones | `soil:records` host |
| Schema and meaning of `body` | the Spore |
| Message identity and delivery | Unibus + the adapter |
| Attachments and documents | CID/content layer (`soil:storage`, `fungos-content`) |
| Credentials | geode |

## Model

- `key = { collection, id }`. Collections are Spore-owned namespaces:
  `email.message`, `chat.message`, `contact`, `crm.deal`.
- `entry = { key, schema, revision, updated-at-ms, origin, deleted, body }`.
- `body` is opaque bytes. JSON is the default for metadata, but the host never
  parses it.
- `revision` and `origin` are assigned by the host on local `upsert`.
- `delete` writes a tombstone; it stays visible to `changes` until retention
  expires.

## Sync

1. Local writes call `upsert`/`delete` and get authoritative revisions.
2. `changes(since, limit)` returns entries ordered by `(revision, origin)`.
3. The peer calls `apply(entries)`. Last-writer-wins:
   higher `revision` wins; equal revision is broken by lexicographic `origin`;
   tombstones participate like any other write.
4. `apply` returns `{applied, skipped}`; a skipped entry lost the comparison.
5. Cursors are opaque. `stale` means the host compacted past the cursor; the
   caller must rebuild from `scan`.

No network is implied by `apply`. The Spore or host transports the page; the
contract only defines the merge. This keeps sync testable and lets Mycelium,
Iroh, or a local file be the carrier.

## Mail and chat mapping

`spores/unibus` writes one record per message observation:

| Collection | ID | Schema |
| --- | --- | --- |
| `email.message` | `account/mailbox/uidvalidity/uid` | `email.message.v1` |
| `chat.message` | `account/message_id` | `chat.message.v1` |
| `chat.body` | same as message | `chat.body.v1` (retained text, bounded) |

Rules:

- Metadata and body are separate records. Fetching text is an explicit
  `chat.fetch`/`email.fetch`, not a side effect of reading metadata.
- Attachments are references (`cid`/document ref), never record bodies.
- The Spore never writes a record for a message it did not receive; AI output is
  a separate record with `origin = "ai"` and a source key.
- A workspace/sync engine can rebuild its cache from `scan`; the bus is not a
  database.

## Grants

- `soil:records.read` — get/scan.
- `soil:records.write` — upsert/delete.
- `soil:records.sync` — changes/apply.

Read does not imply write or sync. The Unibus Spore requests all three only
because it owns the local cache; a read-only viewer requests only read.

## Limits

- Bodies are bounded by the host (the draft allows over-budget errors, never
  silent truncation).
- Tombstone retention is host policy; `stale` is the honest failure.
- Last-writer-wins is per record, not per field. If flags/labels need field-level
  merge, model them as their own records rather than growing a CRDT in the host.
- Records are not an index. Search remains `soil:index` or a derived view.
