# Components

A **component** is a Lua module that turns props into Canvas nodes. It is not a
Spore: it has no module, no grants, no state, no I/O. Chrome, autocomplete, a
list, a dialog, a command palette are components.

The purpose is composition without a second capability system. A Spore owns
behaviour and authority; a component owns how a piece of interface looks and
behaves given data someone else already has the right to show.

## What a component is not

| Not | Because |
|---|---|
| a Spore | no wasm, no manifest grants, no persistence |
| a package | it ships inside, or beside, a signed package and is covered by its lock |
| a plugin | no lifecycle, no background work, no events of its own |
| a capability | it inherits the caller's render context and nothing else |

A component that needs data asks for it as a prop. A component that needs
authority is a Spore, and should be reviewed as one.

## Contract

A component file returns one table:

```lua
-- components/chrome-v1.lua
local M = {}

function M.window(props, theme)
  -- returns a list of Canvas nodes
end

return M
```

- **Pure.** Same props and theme in, same nodes out. No clock, no randomness, no
  I/O, no globals. Canvas supplies props and theme, then collects nodes.
- **Bounded.** Every node a component emits counts against the calling Spore's
  node budget. A component cannot buy itself headroom.
- **Declared ABI.** `abi` names the Canvas Lua API revision it was written
  against (`1`). A host refuses a component whose ABI it does not implement,
  by name, rather than rendering something half-right.
- **No overlays of authority.** Permission prompts, capture indicators and
  revocation notices are Canvas-owned. A component may not draw them, and a
  chrome component may not claim their regions.

## Declaring a component set

```toml
[components]
"fungos/chrome"  = { cid = "sha256:…", version = "1.2.0", overridable = true }
"fungos/autocomplete" = { cid = "sha256:…", version = "0.3.0" }
"acme/chart-frame" = { cid = "sha256:…", version = "2.0.0" }
```

Rules:

1. **Locked, not resolved at runtime.** Component CIDs are part of `spore.lock`,
   so the signed digest covers exactly the interface that will render. A
   component swap is a new package, not a quiet update.
2. **`overridable = true`** lets a user substitute their own implementation (a
   personal chrome, a different palette) without editing the Spore. The Spore
   declares intent; the user's config decides. `false` means the author pinned
   it and a substitution is refused by name.
3. **One degree of sharing.** A component may use other components from the
   same declared set, but the graph is resolved and locked up front; nothing is
   fetched while drawing.
4. **Namespaced.** `components.require("fungos/chrome")`, never a bare global. A
   name is a package identity (`owner/name`), not a file path.

## API

```lua
local chrome = components.require("fungos/chrome")
local list   = components.require("fungos/list")

local rows = list.rows({ items = lines, selected = cursor_line })

return chrome.window({
  title = "Field notes",
  subtitle = "edit · 3:12",
  body = rows,
  actions = { "close", "maximize" },
})
```

`components.require` is supplied by Canvas. `load`, `package` and `require` are
stripped from the sandbox, so component loading cannot become an import system
of its own: the host resolves from the locked set, compiles with the same
sandbox, and hands back the table.

## Why chrome is the test case

Chrome is the component that proves the model, because it is the one place where
*look* and *policy* meet:

- the component draws the frame, title and grips;
- the `[chrome]` manifest declares regions and interaction policy (deadzone,
  snap, grip size, double-click), which **Canvas enforces**;
- content receives input already normalized to the inset rect.

So a third-party chrome can change how a window feels without owning gesture
state, and a user can replace chrome globally without forking any Spore that
happens to open windows. See `surface-windowing.md`.

## The core set

Reviewed, MIT, versioned, in this repository under `components/`:

| Component | Purpose |
|---|---|
| `fungos/chrome` | window frame, title, grips, policy declaration |
| `fungos/list` | virtualised rows with selection and keyboard traversal |
| `fungos/autocomplete` | inline suggestions over a list, anchored to a caret |
| `fungos/dialog` | modal surface with focus trap and escape handling |
| `fungos/toast` | transient status, deduplicated, no authority |
| `fungos/command` | fuzzy command palette over a declared action set |
| `fungos/markdown` | block and inline rendering from a snapshot |
| `fungos/table` | columns, alignment, truncation, sort affordance |

"Core" means reviewed and maintained here. It does not mean privileged: a core
component has exactly the same rights as any other, which is to say none beyond
drawing.

## Open questions

- Whether a component may hold **derived** state between renders (a scroll
  offset) or must recompute from props every time. Recomputing is simpler and
  reproducible; retaining is cheaper for long lists.
- Whether `overridable` is a manifest property or a user policy override. A
  user who wants their chrome everywhere should not need every author's
  permission, but a signed look should not be silently replaceable either.
- Whether components can be shared across workspaces (one chrome, many projects)
  or are resolved per document.
- Node budget: a composed interface can spend the current 256-node limit quickly.
  Either raise it for composed surfaces or count per component.
