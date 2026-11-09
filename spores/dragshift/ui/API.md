# DragShift ring — Lua API

DragShift is a radial menu, not a page. A drop opens one ring; Convert and Tools
are the same ring with different slices, and a tap toggles between them. The
arrangement is what the hand learns, so the two rings must not reflow into each
other.

## Entrypoint

`ui/radial.lua` is a **module-style** view: it reads `module.inputs` and
`module.theme` at load time and returns the composed scene. There is no
`M.render` and no snapshot argument.

```lua
local inputs = module.inputs   -- host-provided
local theme  = module.theme    -- host-provided
```

## Snapshot

`module.inputs` is the host's input record:

```lua
{
  pointer = { x, y }?,     -- host coordinates
  active = string?,        -- slice id under the pointer
  lit = string?,           -- slice id the host has highlighted
  ring = "convert" | "tools"?,
}
```

Everything the ring looks like is a function of that record.

## Actions

None. The host owns which slice is under the pointer and which one is lit; the
view only decides what a ring looks like. Selection is a host event, not an
intent the Spore emits.

## Slots

None.

## Components

None; the slices are drawn from the ring geometry in this file.

## Theme/config

`module.theme` carries the host's inks and radii. The ring names no colour or
pixel size itself, so a theme change restyles it without a rebuild.

## Stable ids

Slice ids are stable across both rings (`convert`, `tools`, and the per-slice
ids); a slice that moves between rings keeps its id so input bindings survive
the toggle.

## Authority

None. The view reads only `module.inputs` and `module.theme`; it cannot observe
or capture the pointer itself.
