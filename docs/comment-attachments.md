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
- **released** — the pending form is dropped. This is *deferred*, on purpose:
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
| transcript (a records book) | append the durable entry, carrying provenance | deferred, self-observed |
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
