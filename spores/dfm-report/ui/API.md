# DFM report — Lua API

A pure projection over a host-supplied DFM report. The 3D view is a separate
native surface (`soil-dfm-viz`), opened by intent and never drawn here.

## Entrypoint

`ui/report.lua` exports `M.render(ctx)`, which reads `ctx.snapshot` and returns a
`fungos.ui` tree. A caller with no context gets an empty report, not an error.

## Snapshot

```lua
{
  summary = { part, process, findings, blockers?, warnings? }?,
  part = { id, name, material? }?,
  process = { id, name, kind? }?,
  findings = { { rule, severity, text, anchor? } }?,
  selected = integer?,   -- index into findings
}
```

`anchor` is the native geometry handle: when it is absent the “Open 3D view”
action is disabled rather than opening an empty viewer.

## Actions

| action | payload | meaning |
|---|---|---|
| `dfm.select` | `{ index }` | select a finding |
| `dfm.open_native` | `{ index }` | open the 3D surface at that finding |
| `dfm.note` | `{ index }` | leave a note on the finding |

## Slots

None.

## Components

None of its own; it composes `fungos.ui` panes, rows and buttons.

## Theme/config

Severity is an ink, not a colour: `blocker -> primary`, `warning -> text`,
`note -> muted`. The host resolves each name, so a theme change moves the report
without editing it.

## Stable ids

Finding rows are `dfm-finding-<index>` (with `-body`, `-rule` children), the
rail is `dfm-rail`, the detail pane `dfm-detail`, and the workspace `dfm`.

## Authority

None. The view reads no files or processes; the host decides which report and
which anchors a caller may see. `dfm.open_native` is a request, not a capability.
