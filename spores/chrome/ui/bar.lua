-- The chrome bar.
--
-- v1 composes scene primitives directly: `sdk/ui`'s layout pass assumes a
-- page-shaped surface laid out top-down, and a 1200x44 bar wants its own
-- geometry. The primitives are the same ones `ui.lua` emits, so this is a
-- different composer over one contract, not a second contract.
--
-- Everything it knows arrives from the host as `_G.chrome`, the record
-- `spores/chrome/wit/world.wit` declares. Nothing here reads a clock, a file or
-- a socket: the bar has no ambient authority.

local M = {}
M.name = "chrome"
M.version = "0.1.0"

local function rect(id, ink, x, y, w, h, radius, opacity)
  canvas.rect {
    id = id, ink = ink, opacity = opacity or 1, radius = radius or 0,
    translation = { x, y, -0.5 }, scale = { w, h, 1 },
  }
end

local function label(id, value, x, y, px, ink)
  canvas.text {
    id = id, text = value, ink = ink, px = px,
    translation = { x, y, 0 }, scale = { 1, 1, 1 },
  }
end

function M.render()
  local s = assert(_G.chrome, "chrome state missing")

  -- Rects are anchored at their top-left corner, so a full-bleed ground starts
  -- at the surface's top edge (+4), not at its middle. Starting at 0 painted the
  -- lower half of the bar and left the upper half transparent.
  rect("ground", "background", 0, 4, 8, 8, 0, 1)
  rect("edge", "border", 0, -3.92, 8, 0.06, 0, 0.9)

  -- Workspaces, left cluster. Units are anisotropic on a bar: the scene is an
  -- 8-unit square stretched to the surface, so one unit is ~150 px across and
  -- ~5 px down. Sizes below are chosen for 1200x44.
  local x = -3.85
  local pill_w, pill_step = 0.24, 0.3
  for index, workspace in ipairs(s.workspaces or {}) do
    local active = workspace.focused and true or false
    -- A pill 5.2 units tall centred in an 8-unit bar starts at +2.6.
    rect("ws-" .. index, active and "accent" or "surface", x, 2.6, pill_w, 5.2, 2.6, active and 1 or 0.85)
    -- The host centres a text node on its translation, so a label is placed at
    -- its pill's centre. Passing the pill's left edge put every numeral half a
    -- glyph run too far left, which only showed up once the bar was rendered.
    label("ws-label-" .. index, tostring(workspace.id), x + pill_w / 2, 0, 12,
      active and "background" or "muted")
    x = x + pill_step
  end

  -- Centre: what has focus.
  label("active-title", s.active.title or "", 0, 1.1, 13, "primary")
  label("active-app", s.active.app or "", 0, -1.7, 10, "muted")

  -- Right cluster: the ambient facts.
  label("network", s.network or "", 2.52, 0, 11, "muted")
  label("battery", s.battery or "", 3.02, 0, 11, "muted")
  rect("divider", "border", 3.3, 0, 0.008, 4.4, 0, 0.8)
  label("clock", s.clock or "", 3.62, 0, 13, "text")
end

return M
