# Principals, workspaces, and grants

Status: design note. Nothing here is implemented beyond the pieces named as
existing; the worklog and inbox already assume it, so it is worth writing down
before a host has to guess.

## Three axes, not one

| Axis | Answers | Where it lives |
| --- | --- | --- |
| **Principal** | *whose* data is this | uid/gid at the native bridge; roles above it |
| **Workspace** | *where* the data is | `soil:vfs` mounted per session |
| **Grant** | *what an app may do* | `spore.toml` |

A **desktop** is a workspace: the surfaces and app state a set of principals is
attached to at once. Desktops can be shared, so "whose is this" has two answers
at different scopes — the workspace belongs to a desktop, and the desktop has
members.

They multiply. Alice's mail app and Alice's worklog app are the same principal
over the same workspace; they differ only in grants. Bob's identical mail app is
a different principal and cannot see Alice's workspace at all. Neither axis can
substitute for the other: uid cannot say "this app may not send", and a grant
cannot say "this is not your mail".

## POSIX identity is an ingress adapter

`uid`/`gid` are not the model; they are how a native host on UNIX decides which
workspaces a session may mount, and with which mutability. That is the entire
bridge. Everything above it speaks workspaces and grants, so the same Spore runs
unchanged where POSIX identity does not exist (Windows tokens, macOS
entitlements, a hostile-hosted session).

Groups are memberships in mounts, not bits on files. A shared mailbox is one
workspace mounted by two principals; it is not a set of files each carrying an
ACL. This is why per-folder ACLs are not needed: the folder is the object.

Mycelium reconciles local accounts *and* group membership from signed access
records, the way `mycelium access host` already reconciles accounts, sudoers and
PAM. The bridge reads that mapping at session start and never re-reads identity
at access-check time — a single direction, so `stat(2)` is never a second opinion
about authority.

## Authority is the workspace boundary, not the CID

`soil:vfs` already states the rule:

> The Spore names a workspace-relative path, never a host filesystem path,
> directory handle, descriptor, URL or CID supplied as authority.

So holding a `content-id` is not access. `entry` carries `content-id`
(`blake3:<hex>`, "not a provider CID") as *identity*, `version` as the monotonic
per-path revision, and `hydration` as present-or-placeholder. The pointer is the
namespace entry; the blob is behind it. CoW falls out: writing a shared object
makes a new content-id and bumps `version` on that path only.

Mutability is a separate grant (`soil:vfs-mut`), which is the natural place for
more aspected sharing later — read, mutate, share, administer — without growing a
second permission model.

## Two revision axes, deliberately

| Stream | Primitive | Orders |
| --- | --- | --- |
| Content namespace | `version: u64`, monotonic per path | writes to one path |
| Records | `revision` + `origin` | writes to one record, across peers |

Do not unify them into one sort key. A mixed ordering hides which stream a
conflict belongs to, exactly as a single timestamp hides whether a worklog row
arrived late or happened late (see [`worklog.md`](worklog.md)). A shared
mailbox needs no new merge policy: mail metadata is already `soil:records`, and
attachments are already CID references rather than record bodies.

## Revocation

Dropping a mount removes *path* access immediately. Bytes a peer already
hydrated are cache; `pinned` is the retention knob. So: revocation is immediate
and unforgeable for the namespace, best-effort for content. Anything that must
be revocable after the fact — a secret, a key, a one-time document — must not be
handed out through a shared mount, because content-addressed sharing has no
un-fetch.

## Non-goals

- **A shared desktop is a session, not a file.** Two principals on one
  compositor is the lease model in [`surface-windowing.md`](surface-windowing.md),
  with input routed to the lease owner. It shares nothing with file sharing and
  should not be forced to.
- **No per-Spore accounts.** Spend OS separation on principals, not on
  twenty read-only views.
- **No second identity store.** The mapping is reconciled from Mycelium's signed
  records; a host-local file that disagrees with them is a bug.

## What this changes today

Nothing in the worklog's data model: it is read-only by grant set, and it is
scoped to a **desktop** — a Canvas workspace shared by the principals attached to
it — rather than to one principal's home directory. So it is a workspace mounted
into a session, shared by the same mount rules as anything else, and not a file
under `~`. The per-principal path in the README is the degenerate case of a
desktop with one member.

What it decides for later: a shared timeline cannot be a shared append-only file
(an append-only file has one writer, and `Store::open` recovers an unacked tail
by truncating it — with two writing principals that is data loss). Timelines are
per-desktop for exactly that reason, and a merged cross-desktop view needs the
record model rather than another writer.
