# Pulse — Lua API

One view, `ui/console.lua`: crons on a schedule and the webhook ingress that
arrives unasked. It is a pure projection of the snapshot `pulse snapshot` emits;
it has no WIT imports, no clock, no filesystem and no network.

## Entrypoint

```lua
local view = require("fungos.ui") -- via the composer
view.render(snapshot)             -- returns nothing; ui.present paints
```

`M.render(snapshot)` builds a `fungos.ui` tree and hands it to `ui.present`,
which lays it out and paints it. `M.hits` is the hit-target list that render
returned. The view never keeps state: a module is re-executed for every frame.

`ui/snapshot.example.lua` is generated from `examples/records` by
`pulse snapshot --now 1767326400000 --select nightly-backup`; tests render it
the way Canvas does.

## Snapshot

```lua
{
  now_ms = integer, now = "2026-01-02T04:00:00Z",
  tab = "crons" | "ingress", title = "PULSE",
  selected = string?, selected_hook = string?,
  summary = { crons, overdue, failing, paused, idle, invalid,
              hooks, hooks_failing, hooks_degraded, hooks_silent },
  crons = {
    { id, name, schedule, target, enabled,
      health, health_text,
      next_at = integer?, next_iso = string?, next_text = string,
      prev_iso = string?, description = string,
      last = { at_ms, text, outcome, detail }?,
      runs = { { at_ms, text, outcome, detail } },
      problems = { { field, text } },
      editable = { minute = boolean, hour = boolean },
      weekdays = { { day, label, on } } },   -- Monday first
  },
  presets = { { id, label, schedule, active } },
  targets = { { id, label, detail, selected, registered } },
  hooks = {
    { slug, name, provider, tenant, kind, credential,
      created_at_ms, created_text,
      health, health_text,
      accepted_24h, rejected_24h, accepted_total, rejected_total,
      last = { at_ms, text, status, reason }?,
      reasons = { { text, count } },        -- top three, 24h
      deliveries = { { at_ms, text, status, reason, bytes, event_type, node } } },
  },
}
```

Every optional field collapses; a missing list renders an empty state. `crons`
and `hooks` are key order, not next-fire order: editing a schedule must not move
the row under the pointer.

## Actions

Every control carries an intent table (`{ kind = ... }`), not a
`fungos.action` name/payload pair. The Pulse host applies it with the same
reducer the CLI uses. The vocabulary:

| `kind` | fields | meaning |
|---|---|---|
| `tab` | `tab` | switch pane |
| `select` | `id` | select a cron |
| `select-hook` | `slug` | select a webhook |
| `preset` | `id`, `preset` | set a schedule from a preset |
| `step` | `id`, `field` (`minute`/`hour`), `delta` | move one field, wrapping |
| `weekday` | `id`, `day` (0 = Sunday) | toggle a weekday |
| `target` | `id`, `target` | set a registered dispatch target |
| `toggle` | `id` | pause or resume |
| `delete` | `id` | tombstone the cron |
| `schedule` | `id`, `schedule` | set a typed schedule (host text input) |
| `save` | `id`, `name?`, `schedule`, `target?`, `enabled?` | create or replace |

The reducer refuses an invalid schedule or an unregistered target with
`{ field, text }` problems and writes nothing; the view paints those problems in
the detail pane. Schedules are UTC.

## Slots

None. The console is a complete surface rather than a component contributed
into another window.

## Components

`fungos.ui` panels, rows, tabs, list rows, action buttons, badges and empty
states. It defines no components of its own, and no pixels: every length and
colour is a token the host resolves.

## Theme/config

Semantic inks only: `primary`, `text`, `muted`, `success`, `warning`, `danger`.
The health ink is the view's one classification decision:

| `health` | ink | glyph |
|---|---|---|
| `ok`, `healthy` | `success` | ● |
| `idle`, `unknown`, `paused`, `silent` | `text` / `muted` | ○ |
| `overdue`, `failing`, `invalid`, `unschedulable`, `degraded` | `danger` / `warning` | ▲ ✕ ! |

Per-app overrides resolve under `dev.fungos.pulse` through `ui.configure`.

## Stable ids

- `pulse-root`, `pulse-header`, `pulse-tabs`, `pulse-body`, `pulse-footer`
- list rows `pulse-cron-<index>`, `pulse-hook-<index>`; fields `…-name`,
  `…-schedule`, `…-health`, `…-next`, `…-route`, `…-counts`
- editor `pulse-detail-*`: `-name`, `-schedule-value`, `-description`, `-next`,
  `-prev`, `-target-value`, `-state`, `-health`
- controls `pulse-adjust-hm|mm|pm|ph`, `pulse-weekdays-<day>`,
  `pulse-presets-<id>`, `pulse-targets-<index>`, `pulse-actions-toggle|delete`
- ingress `pulse-ingress-*`: `-provider`, `-tenant`, `-slug`, `-kind`,
  `-credential`, `-created`, `-accepted`, `-rejected`, `-last`
- reasons `pulse-reasons-<index>-count|text`; deliveries
  `pulse-deliveries-<index>-*`

The index is the row's position in the snapshot, which is why the producer's
order is load-bearing.

## Authority

None beyond painting. The view cannot read a record, a clock or a file, and it
cannot apply its own intents: a press is a proposal the host checks against the
Spore's grants. The browser host (`pulse-serve`) owns the records directory and
the reducer; the CLI is the same reducer without a window.
