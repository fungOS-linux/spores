# Comments and attachments

Status: design, 2026-11-13 — the model behind the comment half of doc-shell and
of any surface an agent works on. Nothing here is implemented; the reader
(`apps/doc-shell/guest`) implements only its precondition.

## The requirement

A person can comment on anything — a block in a Word file, a cell in a sheet, an
entry in a transcript — and the agent sees it. The comment is attached to *the
version of the thing the commenter was looking at*, it stays viewable while the
agent works, and it can be committed into the transcript the agent reads.

That last part is the one that cannot be got wrong: committing is what puts a
comment where an agent will read it, so a commit never loses one and never
reorders one behind the reply it answers.

## Two words, not one

**A comment** is prose plus an anchor. **An attachment** is a comment shown to an
audience, and it has its own state. Separating them is what lets one comment be
visible in a document pane *and* committed into a transcript without becoming
two comments.

```text
comment      id, anchor, author, at, body, reply-to
anchor       target{kind, id, version}, spot        e.g. doc "nda.docx" @v3, block 1A2B3C4D
attachment   comment id, audience, state, destination
```

`spot` is surface-specific and must be resolvable without the store: a `.docx`
block id is `w14:paraId` (Word wrote it), a transcript entry is its record id, a
sheet cell is a cell coordinate. That is the property the reader already relies
on — an anchor written today resolves after the record it came from is released.

`version` is the `content-id` (or records `revision`) the comment was made
against. It is what "it's that version of it" means mechanically: a renderer can
say *the block you commented on has changed* or *is gone* instead of silently
drawing the note over different text, and an agent can be handed the bytes the
commenter saw.

## What delta.app already does

Delta's own note surface is the closest working precedent, and three of its
decisions are worth copying rather than re-deriving:

```sh
delta note add FILE --kind review -L 12,30      # a kind, and the ranges it is on
delta note add FILE --revision HEAD~3 -L 12,30  # or the revision it was left on
delta note show ID                              # "as it was left": content, author, the text it was left on
delta note list --kind review FILE              # notes that touch a file or ranges
delta note remove ID                            # for everyone
```

- **Notes are addressed by ranges, at a revision, with a small open vocabulary of
  kinds** (`todo`, `review`, `rationale`, `decision`) rather than a schema per
  surface. A block id is the same idea for a container that has stable ids.
- **`show` renders the note as it was left**, including the text it was left on.
  That snapshot is what makes a note renderable after the file moved on — the
  same job our `version` pin does, with the evidence stored rather than
  re-derived.
- **`remove` is for everyone.** Notes are one synced store with no per-actor
  files — which is the argument for `soil:records` over per-actor comment files.

## Projection, and why release may never happen

A comment has to reach the agent's prompt, and a prompt prefix is only reusable
while its bytes hold still: the model's K/V cache is keyed on the prefix, so
inserting a note in the middle of a conversation re-derives everything after it.

So the two halves of a comment point in opposite directions:

- **the anchor is for the reader** — it says where the note belongs on screen,
  and it is free to move or dangle;
- **the projection is for the model** — the rendered note is appended into the
  content at a stable boundary (the end of the current content, not the middle of
  a past turn), where it becomes part of the prefix every later turn reuses.

That makes "commit" a projection into content, not a move: the record stays as
the pointer that the UI renders in place, and the content gains the rendered
line. Which means a comment may **never be released at all** — the table keeps
the pointer for as long as the UI wants to show the note where it was left, and
release becomes a compaction decision (when the projection is the only copy
anyone needs) rather than a step of commit.

This is also why "it is a projection" and "it lands in the prompt" are not in
tension: both are true, of different copies.

## Lifecycle

```text
pending ──commit──► committed ──release──► released
   │                                        (record and attachment dropped)
   └── resolve (documents, sheets): the end. No commit, no release.
```

- **pending** — live, syncing, viewable by whoever has the book. Every comment is
  created here, whatever its target.
- **committed** — a durable copy exists in a destination an agent or a reader
  will read (today: the transcript record). Commit appends first and marks the
  attachment committed second, so a crash between them leaves a duplicate that
  dedupes on comment id. The other order loses comments.
- **released** — the pending form is dropped, if it ever is. Deferring is not a
  schedule detail: after a projection into content, the record is the *pointer*
  the reader uses to draw the note where it was left, so releasing it is a
  compaction decision and may never come. Where it does come, it is deferred:
  while the agent is working, the comment has to stay viewable, and dropping it
  the instant the append lands would make a crash-free but still-in-flight commit
  invisible to the person who wrote it. A defensible release rule is
  self-observation: release once the destination replica holds a record carrying
  our `origin` at the revision we wrote. Until then both copies exist, and the
  view dedupes them by comment id — the same dedupe a crash already requires.

Resolve is not commit. A document comment ends by being resolved or by being
accepted into the text on save; nobody reads a spreadsheet as instructions, so
there is no agent-facing stream to commit into and no release to defer.

## Where pending lives

One store, whatever the target: **`soil:records`**, one record per comment
(`collection: "comment.pending"`, `id` = comment id, body = the comment and its
anchor). Comments are entities — they sync, get edited, get replied to, get
resolved — and the book is where entities live. Picking the store by target kind
was the messy part of the earlier proposal: a comment addressed to an agent on a
document would then live in one store and a comment addressed to a person in
another, with no rule deciding which.

The destination varies instead, and that is all the table needs:

| destination | what commit does | release |
| --- | --- | --- |
| transcript (a records book) | append the durable entry at a stable boundary, carrying provenance | deferred; often never |
| the document or sheet itself | *not* a commit: the save accepts the text (`soil:vfs-mut`, `expected-version`) and resolves the comment | n/a |
| another agent's context | append to that session's transcript | same as transcript |

## Provenance

A committed entry carries `from-comment { id, author, at, thread, anchor }`.
Without it the view cannot draw the artifact form of a comment like the pending
form — the requirement that the two render the same — and nothing can tell that a
transcript answer began life as a note on block `1A2B3C4D` of version 3.

## For the tools

An agent's tool surface takes *attachments*, not strings: for each, the
version-pinned target, a way to fetch what was seen (a `soil:vfs` read at a
version, a records `get`), and the note. That is the shape Delta's highlight
notes already have — a note on a span an agent can read — and it is why the
pending book has to be queryable by an agent rather than only by a UI.

## Open

- **Release policy**: self-observation is checkable but needs each replica to see
  its own commit in the destination's change feed. Explicit release ("dismiss")
  is simpler and never loses anything; it just leaves a second copy around until
  someone presses it.
- **Multi-destination**: one comment, several attachments — visible to the
  document pane and committed to a transcript. The model allows it; the first
  implementation should decide whether the UI offers it or hides it behind
  "ask agent about this".
- **Comment on a transcript entry that is itself committed from a comment**: the
  provenance chain is finite (an entry is an entry), but the renderer needs a
  rule for showing a reply to a committed comment. It is the same problem as a
  reply to a thread nobody has opened, which `CommentBook` already defers.
