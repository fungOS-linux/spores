# Markdown Lua API

`ui/editor.lua` is the Markdown reader/editor view. It is a pure projection, but
currently renders a static document fixture at module load rather than exposing
`M.render(snapshot)`.

## Entrypoint

`ui/editor.lua` evaluates to a Canvas scene:

- reads `module.theme`
- calls the composed `ui` kit
- emits primitives through `ui.render(...)`

It does not export a render function yet. The snapshot/document boundary is not
wired.

## Snapshot

None yet. The outline, document body, status text, and save state are fixture
content in the view.

## Actions

None. Buttons and icon buttons are affordances. Open/save will use
`soil:storage` or `soil:vfs` from the Spore logic, not from Lua.

## Slots

None.

## Stable ids

None promised. The view currently uses positional/theme-driven structure. Document
ids will be added with the snapshot contract.

## Components

None.

## Theme/config

Uses `module.theme`. It does not yet read the global `fungos.ui` profile or
app-specific overrides.

## Authority

None in Lua. Document reads/writes are Spore logic grants (`soil:storage`, and
eventually `soil:vfs`). The UI has no filesystem or raw clock access.
