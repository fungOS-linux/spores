# Window policy: sizing, padding, and the WM boundary

Status: design. Nothing implemented beyond `press_deadzone`.

Window management belongs to **Canvas**, not Soil. Soil launches a Spore and
enforces grants; it has no opinion about geometry. But the *values* a window
manager uses — minimum sizes, breakpoints, padding, resize feel — are interface
decisions, so they belong in Lua, and the interface they travel over belongs in
WIT so a non-Lua host can implement the same contract.

## Who owns what

| Layer | Owns | Never owns |
|---|---|---|
| **Canvas (WM)** | geometry, tiling, snapping, z-order, focus, occlusion, padding between windows | the content's preferred width, breakpoints, or what "too narrow" means |
| **Chrome spore** | the frame, title, grips, and the *policy* values it declares (deadzone, snap, grip) | gesture state, hit-testing (Canvas does it), or content layout |
| **Pane / view (Lua)** | size *constraints* and *breakpoints*; how the content reflows when the extent changes | its own rectangle, or another pane's |
| **WIT** | the contract between the three | policy |
| **Soil** | which host to launch, and grants | any of the above |

The rule: **Canvas decides the rectangle; the pane says what it can live with.**

## Content-aware sizing

A pane publishes constraints, not a preferred pixel size:

```lua
ui.render(tree, theme, {
  min = { w = 320, h = 240 },        -- below this the WM should not go
  preferred = { w = 720, h = 640 },  -- what the content is designed for
  max = { w = 1040 },                -- readability ceiling; wider is worse
  breakpoints = { rail = 560, wide = 900 },
  reflow = "required",               -- or "clip", for media surfaces
})
```

- **`min`** is a hard floor. A WM that shrinks below it produces a broken window,
  which is a bug in the WM, not the content.
- **`preferred`** seeds the initial size and the restore size.
- **`max`** exists because a document that is 1.5 screens wide is unreadable even
  though it technically fits.
- **`breakpoints`** are what make resizing content-aware: at `rail` the pane
  drops its sidebar, at `wide` it adds a right gutter. The view picks a variant;
  the WM never knows what a rail is.

The view receives the viewport and picks a variant:

```lua
local vp = module.viewport            -- { width, height, units, scale }
local wide = vp.width >= 900
local tree = wide and layout.wide(state) or layout.narrow(state)
```

That is the whole "content aware" mechanism: constraints out, viewport in,
variants chosen in Lua. No host-side layout knowledge.

## Padding is three separate things

Conflating them is how you get double insets and content that can never align:

| Padding | Owner | Meaning |
|---|---|---|
| `canvas_padding` | Canvas | the gap between the display edge and the window |
| `surface_gap` | Canvas | the gap between two windows |
| `chrome.inset` | chrome spore | space the frame reserves for its own title/grips |
| pane padding | view | space the content reserves inside its rectangle |

A pane draws in the rectangle it is given, minus the chrome inset. It does not
know about `canvas_padding`, and Canvas does not know about a heading's top
margin.

## WIT

`soil:canvas` already exposes `size()` to a guest (host-selected extent). The
window contract needs three more things:

```wit
package soil:pane@0.1.0;

interface policy {
  record constraint {
    min-w: option<u32>, min-h: option<u32>,
    max-w: option<u32>, max-h: option<u32>,
    preferred-w: option<u32>, preferred-h: option<u32>,
    aspect: option<f64>,
  }

  record viewport {
    width: u32, height: u32,
    /// Logical units used by Canvas projection: 8 across, 8 down.
    units: f64,
    scale: f64,
  }

  /// Declared once per pane; the WM may ignore `preferred` but never `min`.
  constraints: func() -> constraint;

  /// What the pane was actually given, and what it was given it in.
  viewport: func() -> viewport;

  /// Fired after a resize, before presentation. Polling is also allowed.
  resized: func() -> option<viewport>;
}

world imports { import policy; }
```

The same three calls can be served by a Lua host (`module.viewport`,
`ui.render(..., policy)`), which is the point: the contract is not Lua-shaped.

## Resize feel

`press_deadzone` was the first field of this, and the rest belongs beside it in
chrome policy:

```toml
[chrome.policy]
deadzone = 4
snap_distance = 12
snap_zones = true
grip = 8
min_size = [320, 240]
resize = "content"      # "content" honours constraints; "free" ignores them
double_click = "maximize"
```

`resize = "free"` exists for media surfaces that want to clip rather than
reflow. It is an explicit opt-out, not the default, because free resizing is how
text ends up at 12 characters per line.

## What this needs, in order

1. **`module.viewport`** in the Lua module context (width, height, units, scale).
   Small host change; unlocks breakpoints immediately.
2. **Constraints on the pane record**, applied by the WM on create, restore and
   user resize. Without it, `min` is advice.
3. **A resize event or poll** so a view re-renders when its extent changes.
   Today the module is re-rendered on input, not on geometry.
4. **The WIT above**, once the Lua shape has settled. Freeze the contract after
   a second host implements it, not before.

## Open questions

- Whether `preferred` is per-pane or per-window (a pane's preference multiplied
  by a layout's weights can fight).
- Whether a pane may *refuse* a resize (return a smaller rectangle than given) or
  must always draw into what it got. Refusing needs a WM fallback path.
- Whether breakpoints are Lua-only or declared in the manifest so the WM can
  pre-size without evaluating the view.
- Whether `max` is a constraint or a recommendation: a user dragging a window
  wider than `max` should probably get the wider window, with the content
  centred, rather than a WM that fights the hand.
