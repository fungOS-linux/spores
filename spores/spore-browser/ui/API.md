# Launcher Lua API

`ui/launcher.lua` is a pure projection over installed Spore catalog entries. It
has no WIT imports, no Unibus session, and no filesystem access.

## Entrypoint

`ui/launcher.lua` exports:

- `M.render(snapshot) -> Canvas scene`

Host supplies `module.theme`. The view is authored against the composed,
flow-layout UI kit; it does not call `canvas.layout` directly yet.

## Snapshot

```lua
{
  entries = {
    {
      id: string,
      name: string?,
      summary: string?,
      workspace: string?,
      surface: string?,
      available: boolean?,
    },
  },
}
```

An entry is launchable only when the host catalog marked it available and gave
it a workspace/surface. The guest does not discover or execute packages.

## Actions

None yet. The current view draws `workspace-<name>` buttons as affordances. The
host performs launch through the `soil:spore.launch` grant when the action
bridge is wired.

## Slots

None.

## Stable ids

`workspace-<name>` is the intended activation/action target. Visual ids are not
stable.

## Components

None.

## Theme/config

Uses `module.theme` and the global `fungos.ui` profile. No app-specific keys yet.

## Authority

None in Lua. Catalog scanning and launch are host/Spore operations declared in
`spore.toml`.
