# The `widget.hit` gap

Status: **open.** Recorded 2026-11-07 after the inbox was found to call a host
function that no host implements.

## What was wrong

`widget.hit { id, x, y, width, height, action?, body?, enabled? }` was called by
four UI files:

```text
spores/unibus/ui/inbox.lua        thread row hit regions
spores/unibus/ui/contacts.lua     contact row hit regions
spores/unibus/ui/lib/ui.lua       button and row hit regions
spores/chrome/ui/launcher.lua     launcher row hit regions
```

No host implements it.

- Canvas's `widget` is the **theming library**: `pixel_space`, `button`,
  `icon_button`, `panel`, `label`, `badge`, `metric`
  (`crates/canvas/src/lua_module.rs`, `WIDGET_PRIMITIVES_V1`). No `hit`.
- Soil exposes no hit-target surface: the `hits` in `soil-search-host` are search
  results, not interaction targets.

So the call resolved to `nil` and raised `attempt to call a nil value (field
'hit')` on the first list row. The inbox could not run in Canvas at all.

## Why it went unnoticed

`tests/ui_fixture.lua` stubs its own host, and it stubbed `widget.hit`. The
fixture was validating the view against a host that existed only in the fixture.
That is the general failure mode to watch for in this repo: a fake host is a
place for unimplemented contracts to hide.

The fixture also never exercised the call. Its placeholder layout does not
produce the row ids the hit loop looks for, so the loop never ran even with the
stub present. The stub's call counter was never asserted.

## What was changed (this repository)

- `chrome/ui/launcher.lua`: `widget.hit` → **`widget.button`**. This one had a
  real target: the surrounding calls are all genuine Canvas APIs, and the spec
  passed exactly `button`'s shape (`id`, `label`, `action`, `body`, `enabled`,
  geometry) plus `appearance = "quiet"`, matching `widget.icon_button`'s own
  delegation. It now emits a real `activate` event.
- `unibus/ui/inbox.lua`, `unibus/ui/contacts.lua`, `unibus/ui/lib/ui.lua`: the
  call is now guarded. When a host provides `widget.hit` it is called; otherwise
  the spec is kept in a `pending_hits` list and the module keeps rendering.
- `tests/ui_fixture.lua` exercises the guard directly, in both directions, and
  asserts a hit is delivered when a host provides one and queued when it does
  not.

## What is still unresolved

The inbox and contacts rows still have **no working interaction path**. The
guard stops a crash; it does not make a row pressable.

Three options, none free:

1. **Implement `widget.hit` in the host.** The spec shape is already used by
   four files and matches `widget.button` closely. This makes existing code
   correct, but it adds a second interaction mechanism to Canvas alongside
   `activate` events, which is how two ways of doing one thing happen.
2. **Migrate the rows to `widget.button`.** Uses the real Canvas API and gets
   `activate` events for free, at the cost of a visual change: the rows would be
   drawn by the host as quiet buttons rather than by the view as raw rects. This
   is the option with no new host surface.
3. **Draw the rows as the declarative `ui.*` node tree and let `ui.capture`
   produce normalized hit targets** (the fungos-sdk path). Largest change, but it
   puts the inbox on the same footing as every other Spore and removes the
   bespoke imperative renderer.

Recommendation: **2**, then 3 when the node tree can express the detail pane.
Option 1 is the most tempting and the worst long-term, because it would leave two
interaction mechanisms in one host with no rule about which to use.

## The check that would have caught this

The fixture should not be able to stub a host function that no host provides. A
committed list of the host surface (Canvas `widget.*`, `canvas.*`, `module.*`),
asserted against the fixture's fake host, would fail the moment the fixture
invents a capability. `tend` in `fungos-sdk` builds the same kind of fake host
and has the same exposure.
