# surface-probe — Lua API

A **fixture** Spore for surface import, not a view. It exists so the surface
host can prove load and grant wiring; it draws nothing.

## Entrypoint

`ui/probe.lua` returns a descriptor table:

```lua
{ title = "surface-probe", status = "fixture spore" }
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

The package id is `dev.fungos.surface-probe`; the descriptor is its only surface.

## Authority

None. It is a probe: it proves loading and grant wiring, not rendering.
