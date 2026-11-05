# Calendar UI API

## Entrypoint

`calendar.lua` returns a module with `render(snapshot)`.

Canvas supplies `module.viewport`, `module.theme`, `module.event`,
`module.view.selected`, `module.select`, and `module.now.epoch_ms`. The view has
no filesystem, network, clock, or iCal parser.

## Snapshot

The Calendar Spore emits normalized events from `fungos.calendar/event` records:

```lua
{
  revision = 1,
  title = "Calendar",
  account = { label = "Records · calendar", sync = "live" },
  today = { date = "2026-11-05", year = 2026, month = 11, day = 5, weekday = 4 },
  calendars = { { id = "work", label = "Work", color = "primary", count = 7 } },
  events = {
    {
      id = "design-review",
      summary = "Design review",
      calendar_id = "work",
      calendar_label = "Work",
      calendar_color = "primary",
      date_key = "2026-11-05",
      end_date = "2026-11-05",
      start_ms = 1793876400000,
      end_ms = 1793880000000,
      start_time = "11:00",
      end_time = "12:00",
      all_day = false,
      is_recurring = false,
      rrule = "",
      location = "Studio",
      organizer = "Maya",
      attendees = { "Avery", "Maya" },
      notes = "Review the next slice.",
      conflicts = {},
    },
  },
  selected_id = "design-review",
}
```

`date_key` and the display times are host-resolved. The UI does not expand
recurrence rules or resolve time zones; it renders the host's normalized event.

## Actions

View-local activation targets:

- `cal-event-N` selects an event in the current data.
- `cal-week-event-D-N` selects event `N` from day column `D`.
- `cal-prev`, `cal-next`, `cal-today` move the local anchor.
- `cal-view-week`, `cal-view-agenda`, `cal-view-month` switch layout.
- `cal-filter-<calendar_id>` filters by calendar.
- `cal-mini-YYYY-MM-DD` and `cal-month-cell-YYYY-MM-DD` select a date.
- `cal-detail-open` and `cal-detail-edit` are intent actions, not direct writes.

## Slots

The wide layout uses a calendar sidebar, main view, and event detail pane. The
narrow layout keeps the same reading order and lets the host wrap the panes.

## Components

The view uses `fungos.ui` workspace, sidebar, pane, row, column, text, list row,
badge, section label, tab bar, rule, and host layout. It does not guess text width.

## Theme/config

Inks are semantic: `background`, `surface`, `text`, `muted`, `primary`,
`success`, `warning`. Calendar colors come from the snapshot. `module.theme.body`
sets type scale and `module.viewport` sets density.

## Stable IDs

`cal-title`, `cal-subtitle`, `cal-prev`, `cal-today`, `cal-next`,
`cal-view-<view>`, `cal-filter-<id>`, `cal-mini-<date>-day`,
`cal-month-cell-<date>`, `cal-day-<n>`, `cal-week-event-<day>-<event>`,
`cal-event-<n>`, `cal-detail-title`, `cal-detail-when`, `cal-detail-notes`.

## Authority

The view owns local date anchor, selected view, calendar filter, hover and event
selection. The host owns records, iCal parsing, time-zone resolution, recurrence
expansion, layout, pixels, focus, and persistence. The view never writes an
event; `open` and `edit` leave as intents.
