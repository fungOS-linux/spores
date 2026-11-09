# Mush shell — Lua API

The Canvas shell surface for Mush. It is a **placeholder**: the native host is
the first frontend, and this file returns an empty table so the package declares
its UI intent without pretending to a view it does not have yet.

## Entrypoint

`ui/shell.lua` returns `{}`:

```lua
-- Canvas shell surface placeholder. The native host is the first frontend.
return {}
```

There is no `M.render` yet. The shell semantics live in the Spore's Rust guest
(`spores/mush/src`), and the native host owns the screen; a Canvas presenter
will compose the same `soil-vt` frames when it lands.

## Snapshot

None yet. The planned shape is the native host's frame: the current grid, the
prompt line, the cursor and the scroll viewport. It must not be a second shell
state — the Spore remains the source of truth for `cwd`, environment and exit.

## Actions

None yet. The planned actions are the ones a terminal needs: `mush.input`
(raw bytes), `mush.resize` and `mush.scroll`; the host keeps the pty and the VT.

## Slots

None.

## Components

None.

## Theme/config

None yet. When the Canvas presenter lands, the grid should take its inks and
text scale from the host theme, not from the Spore.

## Stable ids

The surface id `mush-shell` is reserved for the Canvas presenter; the native
host and the web terminal already present the same session, so the id must not
be reused for a different surface.

## Authority

None. The Spore owns shell semantics but has no host filesystem, process or
network access; the UI layer must not acquire any either. A Canvas shell is a
presenter of the same session, not a second executor.
