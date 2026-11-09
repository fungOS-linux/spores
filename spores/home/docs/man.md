# home

The launcher surface. It draws wallpaper, a status bar, widgets, the app grid
and the dock, and it emits one intent: `home.open`.

## What it reads

A snapshot supplied by the host:

- `host` — the machine's name.
- `apps` — `id`, `name`, `host` (the name to open), `glyph`, `category`, `color`.
- `services` — `name`, `status` (`up` / `down` / `gateway`), `ms`, `host`.
- `widgets` — `today`, `machine`, `store`, `system`.
- `dock` — the ids to keep in the dock.

The view never fetches. Probing, the store catalog, memory and disk are host
facts; a launcher that fetched would be a second authority over what is true.

## What it emits

`home.open` with `{ id, host }`. Opening a surface is the host's decision, and
a Spore without a surface opens its store listing instead.
