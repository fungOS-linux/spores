# Terminal Lua API

`ui/terminal.lua` is the fallback/status projection for the terminal Spore. The
interactive terminal is a host-rendered `soil:terminal` grid; this view lists
Mycelium targets and shows the latest plain-text frame for debugging and
non-Canvas surfaces.

## Entrypoint

`ui/terminal.lua` exports:

- `M.render(snapshot) -> { revision, ui, nodes }`

This view uses the older semantic-node shape, not the `canvas.layout` tree.

## Snapshot

```lua
{
  targets = {
    {
      id: string,
      label: string?,
      kind: string?,
      health: "healthy" | string?,
      shell: boolean?,
    },
  }?,
  terminal = {
    title: string?,
    text: string?,
  }?,
}
```

If `terminal` is present, its text is split into one node per non-empty line.

## Actions

None. Opening/closing a PTY session is host/Spore logic with
`soil:mycelium.shell` and `soil:terminal` grants.

## Slots

None.

## Components/slots

None.

## Stable ids

`title`, `targets`, `empty`, `term.title`, `term.<n>`, `t<n>`, `t<n>.name`,
`t<n>.meta`.

## Theme/config

This fallback view uses role tokens from the host theme. It has no app-specific
profile keys.

## Authority

None in Lua. SSH, routes, host keys, credentials, PTY lifetime, and the cell grid
belong to the host capabilities named in `spore.toml`.
