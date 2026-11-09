# Worklog: one timeline across tools

Status: schema `fungos.worklog/event@1` plus a first producer set (Marbles, GitHub)
and the receiver in [`spores/worklog`](../spores/worklog). The read-only Spore
that renders it is [`spores/worklog/ui`](../spores/worklog/ui).

## Why one schema

Every tool that says "this happened" says it differently. Marbles publishes a
signed lifecycle envelope, GitHub publishes a webhook with a different shape and
a different signature header, and a thread mention will publish something else
again. A worklog that kept three shapes would push the differences into the
view, and every new producer would mean a new branch in the renderer.

So the boundary is the normalization. A producer's payload is reduced to one
shape on the way in, and everything downstream — the log, the query, the Spore —
knows only that shape. Adding a producer is a mapper, not a schema change.

```text
marbles webhook ─┐
github webhook  ─┼─▶ normalize ─▶ worklog log ─▶ Snapshot ─▶ ui/worklog.lua
mentions (later)─┘    (this doc)
```

## The event

```json
{
  "schema": "fungos.worklog/event@1",
  "id": "marbles:fpl:7",
  "at": 1767225600000,
  "hlc": { "physical": 1767225601234, "counter": 0 },
  "source": "marbles",
  "kind": "closed",
  "actor": "marbles",
  "subject": "marbles://fpl/spores-42",
  "summary": "closed spores/spores-42",
  "links": [{ "rel": "subject", "locator": "marbles://fpl/spores-42" }],
  "payload": { "company": "fpl", "project": "spores", "issue_id": "spores-42" }
}
```

- `id` is derived from the producer's own identity, never minted here. Marbles'
  `(store_id, seq)` is already unique and already stable across redelivery, so
the id is `marbles:<envelope id>`. Deduplication keys on it, which is what makes
  a retried webhook a no-op instead of a second row.
- `payload` keeps the producer's own fields. It is evidence, not a contract:
  nothing downstream reads it, and it is allowed to differ per producer.

### Two timestamps, on purpose

| Field | Meaning | Used for |
| --- | --- | --- |
| `at` | when the producer says it happened, in ms | what the view prints |
| `hlc` | the order it entered this log | sorting and paging |

Sorting by `at` interleaves events from machines whose clocks disagree, and the
order changes when a producer retries an old delivery days later. `hlc` is a
hybrid logical clock — physical milliseconds plus a counter for ties — assigned
on ingest and monotonic for one store. "Newest first" then means *newest to this
log*, is stable across reads, and pages without duplicates or gaps.

The consequence is deliberate: a Marble that happened three days ago but arrived
now sorts at the top, because that is when it entered the log. Printing `at`
keeps the reader from being lied to about when it really happened.

A backwards system clock cannot produce an event that sorts before one already
written: the physical component never decreases and the counter absorbs the
collision.

### Locators, not URLs

`subject` and `links[].locator` are typed references with a known scheme:

| Scheme | Example |
| --- | --- |
| `marbles://` | `marbles://fpl/spores-42` |
| `github:` | `github:fungOS-linux/spores#41` |
| `delta://` | `delta://thread/<id>` |
| `vfs:` | `vfs:notes/x.md` |

A URL rots: the tracker moves, the session is renamed, the host changes, and the
link is dead with no way to tell what it used to point at. A locator names the
thing by identity and scheme, so the client that owns that scheme can still open
it later. An unknown scheme is an error rather than a passthrough, so adding one
is a decision somebody makes once instead of a typo that ships.

## Producers

### Marbles

`marbles.event/1`, delivered to `POST /ingest/marbles`, authenticated with
`x-marbles-signature: sha256=<hex>` over the raw body.

| Marbles event | `kind` |
| --- | --- |
| `created` | `created` |
| `updated`, `status`, `labels` | `changed` |
| `claimed` | `claimed` |
| `released`, `claim_expired` | `released` |
| `closed` | `closed` |
| `escalated` | `escalated` |
| `depends_on`, `unblocked_from`, `unblocked_from_remote` | `dependency` |
| `heartbeat` | `lease` |

An event name that is not in this table is rejected with a 400 rather than
stored as an unknown kind. A producer that invents a lifecycle event should make
someone decide how it renders, once, here.

`kind = "lease"` is stored but dropped from the default view. A heartbeat is
not a thing that happened; it is a claim staying alive. Keeping it in the log
trades a small file for the ability to answer "was this claim ever alive?"
without a second store.

**Known gap.** The `marbles.event/1` envelope carries no actor and no detail
text, so a Marbles row currently reads `closed spores/spores-42` with
`actor = "marbles"`, and a `status` row cannot say *which* status. Both fields
exist in Marbles' `history` table and are dropped when the outbox row is
written. The ingester already accepts optional `actor` and `detail` members, so
the fix is on the Marbles side: add the two columns to `outbound_event`, copy
them in `log_event`, and add them to `EventEnvelope`. Until then the worklog is
as descriptive as the producer makes it, and no prose is invented to hide that.

### GitHub

Delivered to `POST /ingest/github`, authenticated with
`X-Hub-Signature-256: sha256=<hex>` over the raw body. Same construction as
Marbles', so one verifier serves both; only the header name differs.

| Webhook | `kind` |
| --- | --- |
| `pull_request` `closed` + `merged: true` | `merged` |
| `pull_request` `closed` without merging | `closed` |
| `pull_request` `opened` / `reopened` | `changed` |

The summary is the pull request title with the verb: `merged: Render the
worklog`. `at` is `merged_at`/`updated_at` parsed as RFC 3339; when that cannot
be read the event is still stored at arrival time and the raw string stays in
`payload.timestamp` with `payload.timestamp_parsed = false`, so the degradation
is visible instead of silent.

Everything else GitHub sends is acknowledged and dropped. A push is not a thing
that happened to a person.

## The receiver

The receiver is a native process, not a Spore: it owns a socket and a file, and
no Spore is allowed to own either. It binds loopback, requires a configured
secret to start, caps a body at 1 MiB, and handles one delivery at a time.

Order of operations on every delivery, because each step protects the next:

1. **Verify the signature over the raw bytes**, before parsing. Parsing an
   unauthenticated body and then deciding is how a parser bug becomes an
   authority bug.
2. **Normalize.** A payload that does not map is a 400 and is not stored.
3. **Deduplicate by `id`.** A redelivery after a lost response is a no-op.
4. **Append, sync, then acknowledge.** The id is only claimed once the bytes are
   on disk, so a failed write leaves the producer free to retry.

### Reconciliation

A webhook that was down still has to be caught up, so push is not the only
input. `worklog ingest-batch` reads one envelope per line and appends them in a
single process — one process per row would re-read the whole log each time,
which is quadratic in exactly the case that matters.

For Marbles, `tools/pull-marbles.sh` is the reconciliation leg, and it goes
through `mb backup` rather than opening the live store: a WAL database cannot be
read by a read-only reader at all (`attempt to write a readonly database`), and
`mb backup` is Marbles' own atomic `VACUUM INTO`.

The outbox is also where Marbles keeps what its webhook envelope drops.
`outbound_event.seq` is the rowid of the `history` row the same transaction
wrote, so joining them recovers the actor and the detail text — the difference
between `closed spores-42` credited to nobody and `ajmwagar closed spores-42 —
ack: superseded by scan recognition`. That join is why the pulled log is richer
than the pushed one, and it is the evidence for adding those columns to the
envelope rather than working around their absence.

### Durability

An append is acknowledged only after its terminating newline is durable. That
makes recovery unambiguous: a final line with no newline is by definition an
append that was never acked, so the store truncates it and starts. Refusing to
start would need a human to clear a file over a crash that already cost nothing,
and the producer will redeliver anyway. A *complete* line that does not parse is
corruption and does fail loudly, with its line number.

### What this store is not

The log is JSONL because no Soil host implements `soil:records` yet. It is the
stand-in, and it is deliberately shaped like the real thing: one record per
event, keyed by `id`, in collection `worklog.event`, with the event as the body.
When records exist, the file is imported and the file format disappears; the
schema does not change.

## Transport is Unibus's job

`soil:unibus` already defines the envelope this receiver reimplements: a `frame`
of `{ kind, producer, issued-at-ms, body }` where the body is opaque and "the
Spore owns the schema"; `poll(kinds, cursor, limit)` returning an opaque cursor
and `stale` when that cursor predates the host's retention window; and `publish`
gated on `soil:unibus.publish`. A cursor, a retention window and a signed
producer are exactly what this crate grew on its own, and that duplication is not
meant to survive.

The split that should hold:

| Concern | Owner |
| --- | --- |
| Envelope, cursor, retention, delivery, signatures | `soil:unibus` |
| Event body schema (`fungos.worklog/event@1`) | this crate |
| Normalizing a producer's payload into that body | this crate |
| Rendering the timeline | `dev.fungos.worklog` |

So `fungos.worklog.event.v1` becomes a **frame kind**: a Marbles or GitHub
publisher normalizes and publishes it — the same shape as the Confluence adapter
in [`ai-inbox-spec.md`](ai-inbox-spec.md) — and the Spore becomes
`soil:unibus.observe` + `soil:records`, which is the inbox's already-working
pattern.

This binary exists because no Soil host implements `soil:unibus` or
`soil:records` yet, so something has to hold a cursor today. It is **demoted, not
deleted**: keep the schema, the normalizer and the ordering rules, and lose the
socket, the secret file and the append-only store when a host lands.

## The Spore

`dev.fungos.worklog` is read-only. It holds no producer capability, no socket,
and no `soil:records.write` — the receiver writes through the host and the Spore
only reads. That is the same grant-boundary rule that makes the inbox
`dev.fungos.inbox` and sending `dev.fungos.mail`: a viewer that could also write
would make "who wrote this?" unanswerable.

## Limits

- **Ordering is per store.** Two worklog stores have two clocks. Merging them
  needs their own reconciliation, not a bigger sort key.
- **`scan` is a full read.** Fine at thousands of events, wrong at millions; the
  fix is a host-side index, not a smarter file reader.
- **No retention policy yet.** The log only grows. Tombstones and compaction
  belong to the host store, not here.
- **Not the stack.** "What do I owe a reply to?" is a query over Marbles and
  unread threads, not a worklog table. See the stack notes in
  [`docs/ai-inbox-spec.md`](ai-inbox-spec.md).
