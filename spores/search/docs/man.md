# Queries

`search QUERY` ranks matching documents, installed commands and declared
actions. Matching is fuzzy and score-ordered; an exact prefix outranks a scattered
match, and a shorter label outranks a longer one with the same match.

# Results

A result says what it will do before it is chosen: open a document, launch a
Spore, or invoke an action with a named input. The view names intents; it never
performs one itself. Dispatch belongs to the host that owns the target.

# Empty queries

The surface opens on recent items and installed Spores rather than an empty
list, so the first Enter is always meaningful.

# What it cannot do

No network, no process, no secrets, no environment, no raw filesystem, no clock.
It asks `soil:search` for locators and draws the answer.
