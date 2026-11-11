# The log

`worklog` shows one chronological timeline of work: commits, issues, merges and
agent threads, ordered by when they happened rather than by which tool recorded
them. Rows carry their source, so a reader can check a claim instead of trusting
the summary.

# Ingestion

Events arrive from a host-side receiver that normalises Marbles, GitHub and
thread activity into one event shape. This Spore reads that collection and
renders it; it is not the producer.

# Reading

The view windows the log to what is on screen and reads scroll and hover from
the host, so a long history scrolls without loading everything. A locator in a
row is the id, not a display string.

# What it cannot do

No writes and no sync: a viewer that could also write makes the log
unprovable. No network, no process, no secrets, no clock.
