# Surface windowing semantics

Status: proposed. Canvas owns the implementation; Soil owns the guest bridge;
this file is the contract both sides code against.

## Decision

**A Surface is Canvas's window.** A Spore never gets a window. It gets a
*surface lease*: a rectangle in the workspace, a revision, one present path, and
normalized input.

There are two content classes, and they look identical to a workspace document:

| Class | Rendered by | Examples |
|---|---|---|
| **Owned** | Canvas, from a scene or widget tree | scene, widget, chart, asset, terminal, project browser |
| **Leased** | a producer outside the scene tree presents frames | stream, live image, live video, **gpu** |

Both are `Surface { id, title, content, inputs }` in a placement. Z-order is
document order. A producer can appear, move, hide, and close without Canvas
learning its internals, and Canvas can capture it without asking it to redraw.

## Why not iframe or a nested compositor

Embedding a browser iframe or letting a Spore run its own compositor inside a
rect looks cheaper for one demo and costs correctness everywhere else:

- **Two focus models.** Someone has to decide what has keyboard focus when the
  nested thing and Canvas disagree, and that decision leaks into every input
  path.
- **Two revocation points.** Unibus revokes a grant; a nested compositor can
  keep drawing from a cached surface. A lease has exactly one place to fail.
- **Capture lies.** Agent parity requires that a capture matches what the person
  saw. A nested compositor can present pixels Canvas never saw, and then the
  agent reasons about a different screen.
- **Occlusion.** Clipping, cover-cropping, and display-move are Canvas's. A
  nested surface has to reimplement all of them badly.

If a Spore needs rich pixels, it presents a texture. It does not get a window.

## Surface roles

Not every surface is a tile. Three roles, and the role is a property of the
surface, not a hint to the renderer:

| Role | In layout flow | Above content | Example |
|---|---|---|---|
| **content** | yes, tiled by Canvas | no | a spore's document, a widget, a terminal |
| **chrome** | no, anchored to a display edge | yes | dock, top bar, app switcher |
| **overlay** | no | always, above chrome | permission prompt, capture indicator, revocation notice |

Content participates in tiling and gets the remainder of the display after chrome
insets. Chrome anchors to an edge (or a corner) and is excluded from tiling, so a
dock can never be counted as a tile and can never be scrolled away.

### Chrome may be a Spore

Dock and top bar can be spores, and should be, for the same reason any app can:
live restyling without a rebuild. The constraints are narrow:

1. **Chrome proposes, Canvas disposes.** A chrome spore declares an anchor and an
   inset; Canvas computes the final rect against the display and the reserved
   system margins. A chrome spore cannot move content directly.
2. **One chrome owner per display.** A second chrome spore does not stack; it
   replaces the first atomically, or is refused by name.
3. **Chrome gets less authority than content, not more.** It can read the
   workspace summary and request layout actions; it cannot mutate the document,
   open surfaces, or read other surfaces' content.
4. **Fail-safe fallback.** If the chrome spore crashes, is revoked, or fails to
   compile, Canvas draws its built-in minimal bar. Losing the window controls must
   never be a way to lose the window.
5. **Input priority is fixed:** overlay, then chrome, then content. Regions a
   chrome surface does not claim pass through; a chrome spore cannot make a
   full-screen invisible pointer trap.

### Overlay is never a Spore

Security-critical UI stays Canvas-owned: the permission prompt, the capture
indicator, the revocation notice, and the "this spore is drawing" marker. A
spore that could draw these could impersonate consent. Overlays are also the only
surfaces that may draw above chrome.

### Why this matters for revocation

A revoked capability must be visible. If chrome were the only place that showed
grants, a broken chrome spore would silently hide a revocation — which is why the
revocation notice is an overlay and why the fallback bar exists.

## Chrome as a wrapping spore

Chrome is what makes a content surface a *window*: the frame, the drag handle,
the resize grips, the close affordance. It should be a spore, so "how dragging
feels" is a value in a Lua table rather than a constant in the renderer.

The split that keeps it safe: **a chrome spore declares regions and policy;
Canvas enforces both.**

```toml
[chrome]
role = "window"          # names the frame a content surface is wrapped in
anchor = "top"           # top | left | right | bottom | floating
inset = 36               # logical points reserved from the content rect
regions = ["title", "close", "grip"]      # named, hit-tested by Canvas
draggable = ["title"]                     # which regions initiate a move
resizable = ["e", "s", "se"]              # which edges initiate a resize

[chrome.policy]
deadzone = 4             # travel before a press becomes a drag
snap_distance = 12
snap_zones = true        # half/quarter dock offers at display edges
grip = 8                 # resize handle thickness, logical points
min_size = [160, 120]
double_click = "maximize"
```

Rules:

1. **Regions are geometry, not pixels.** Canvas hit-tests the named regions it
   computed; a chrome spore cannot claim a region and then not draw it, and it
   cannot claim a full-screen invisible one.
2. **Policy is data, not behaviour.** Deadzone, snap, grip size, double-click
   action and motion tokens are read by the interaction engine. A chrome spore
   changes feel without owning the gesture state machine.
3. **One chrome per surface, one surface per chrome.** Chrome wraps; it does not
   tile, scroll, or nest.
4. **Content never knows.** A content spore presents into the inset rect and
   receives input already normalized to that rect. Swapping chrome does not
   touch content.
5. **Fail-safe.** A chrome that fails to compile, load, or render is replaced by
   Canvas's built-in bar for that surface. Window controls must never be lost
   because a spore broke.
6. **Overlays stay Canvas-owned.** Permission prompts, capture indicators and
   revocation notices are never drawn by chrome, because a window frame that can
   impersonate consent is worse than an ugly one.

This is the same authority rule as the rest of the system: the *user* chooses
their chrome (like choosing a window manager theme), and the content author
cannot force one.

## Theming out of the gate

Three levels, and each one only maps roles to values:

| Level | Owner | Scope |
|---|---|---|
| **Token set** | Coherence | palette, type scale, spacing, radius, motion |
| **Theme** | project or user | role → token choices for one look |
| **Spore view** | spore author | *names* roles; never supplies RGB, sizes or coordinates |

Today a view can name `ink` (`text`, `muted`, `primary`, `success`, `warning`,
`danger`) and read `module.theme` (`micro … hero`, `space_2`, `radius`, the
eight role colours). That is enough to look consistent and not enough to look
designed. The role set needs to grow before spores ship:

- **Surfaces:** `background`, `surface`, `raised`, `sunken`, `border`, `divider`,
  `scrim`.
- **Text:** `title`, `body`, `caption`, `mono`, plus `on_primary`, `on_danger`
  for text that sits on a filled role.
- **State:** `hover`, `pressed`, `selected`, `disabled` as modifiers on a role.
- **Elevation and motion:** shadow steps, and `duration`/`easing` tokens so
  chrome transients feel the same everywhere.
- **Scales, not literals:** `space_1 … space_6`, `radius_sm/md/lg`, and the type
  scale that already exists.

The invariant stays: a view may *derive* from a role, never invent a colour. If
a spore needs an RGB literal to look right, the role set is wrong, not the spore.

## The GPU lease

Soil already ships the narrow bridge: `soil:canvas-gpu@0.1.0` takes a
`wasi:webgpu` `gpu-texture` by borrow and returns success or an error string.
The host chooses the extent; the guest cannot create a device, window, or
swapchain.

Rules:

1. **One lease per Spore.** One granted surface, host-selected extent.
2. **One frame in flight.** `present` returns `busy` rather than queueing. The
   guest drops or coalesces; the host never grows a frame backlog.
3. **Monotonic revision.** Every presented frame carries a revision that only
   increases. A reader never moves backwards, and a capture can name exactly
   what it drew.
4. **`source_id` is a name, never a path or handle.** The guest never learns the
   surface id, and cannot steer the host at a file.
5. **Revocation closes the surface.** It does not blank it silently: the surface
   shows a named placeholder and the grant is reported.
6. **Extent changes invalidate guest textures.** `size()` may change on display
   move or resize; the guest must rebuild. A stale present is refused, not
   stretched.

## Surface JSON

```json
{
  "kind": "gpu",
  "source_id": "markdown",
  "width": 1280,
  "height": 720,
  "format": "rgba8",
  "revision": 7
}
```

Surface kinds are a **version boundary**, not a typo: an older Canvas retains an
unknown kind in `SurfaceContent::Unsupported` and draws a placeholder, so adding
`gpu` does not break a workspace written by a newer build. Malformed known kinds
still fail loudly.

## Input

- Hit-test by rectangle. Focus is the topmost visible surface under the pointer
  that accepts input; a leased surface is not special.
- The host normalizes events and stamps a generation. A presenter that has
  already moved on refuses a stale generation instead of replaying it out of
  order.
- A fully occluded leased surface may stop presenting. It does not silently
  lose focus while visible.
- Pointer coordinates arrive relative to the surface, so a workspace move does
  not change a Spore's coordinate system.

## Lifecycle

```text
opening ──▶ ready ──▶ presenting ──▶ hidden ──▶ closed
   │           │            │
   └── error ──┴── revoke ──┴──▶ revoked
```

- `present` while `opening` is `not-ready`.
- `present` after `closed` or `revoked` is `closed`.
- `hide` keeps the lease; `close` releases it. Hiding is not revocation.
- A renderer failure is `backend`, not an empty success.

## Present paths

| Content | Producer | Transport |
|---|---|---|
| scene, widget, chart, asset, terminal | Canvas | none |
| stream | local relay | reviewed URL, resolved host-side |
| live image, live video | media edge | named frame in the runtime dir |
| **gpu** | Soil WebGPU host | texture → host frame → surface |

The GPU path is the only one where the producer hands over a GPU object, and
even then only to the host. No device, queue, or mapped memory crosses into
Wasm.

## Capture and agent parity

A GPU lease must support bounded capture for the same reason every other surface
does: a person and an agent looking at the same window must see the same pixels.
Capture returns one bounded image and always names the surface revision it came
from. A capture that cannot state its revision is a bug.

## Implementation steps

1. `SurfaceContent::Gpu { source_id, width, height, format, revision }` in
   Canvas, version-safe via the existing `Unsupported` fallback.
2. A `CanvasGpuPresenter` implementation in Canvas that maps a `source_id` to a
   surface lease and publishes into the same frame store the video paths use.
3. A control command to open a spore surface, reusing `PlaceSurface` for
   geometry rather than inventing a second layout path.
4. Input routing to the lease owner through Unibus, not directly into Wasm.
5. Overlay: the user-visible "this spore is drawing" indicator, the same one
   `soil:view.capture` requires.

## Open questions

- Whether one Spore may hold two leases (split view) or must be two instances.
- Whether present pacing is host-driven (`deadline`) or guest-driven.
- Whether a lease can outlive its module while the frame stays visible.
