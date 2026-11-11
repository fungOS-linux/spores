# The queue

`git` shows work that is finished and waiting on a decision: what changed, why it
was done, and what is blocking it. The queue is a projection of tracker state
that arrives as records; it is never the store of record.

# Decisions

A row names the decision it is waiting for. Acting on it is a host action with its
own authority. This view proposes; it never merges. A view that could merge would
be a second authority over the work graph, and the tracker already is the one.

# What it cannot do

No records writes, no network, no forge access, no process, no secrets. Pull
request state arrives in the snapshot; v0 talks to nothing.
