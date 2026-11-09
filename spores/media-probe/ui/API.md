# media-probe — Lua API

A **fixture** Spore, not a view. It exists so the media host and the packaging
pipeline have something to load; it draws nothing.

## Entrypoint

`ui/probe.lua` returns a descriptor table:

```lua
{ title = "media-probe", status = "fixture spore" }
```

There is no `M.render`.

## Snapshot

None.

## Actions

None.

## Slots

None.

## Components

None.

## Theme/config

None.

## Stable ids

The package id is `dev.fungos.media-probe`; the descriptor is its only surface.

## Authority

None. It is a probe: it proves loading and grant wiring, not rendering.
