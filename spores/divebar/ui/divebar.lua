-- Divebar: the system bar as an SDK view.
--
-- Authored at a fixed 1200x44 space and authority-free: everything it draws comes
-- from the snapshot. `tend` composes this file into a retained Canvas module, and
-- the compositor scales the scene into whatever surface it grants.

local ui = require("fungos.ui")

local M = { name = "dev.fungos.divebar", version = "0.1.0" }

local WIDTH, HEIGHT = 1200, 44
local PILL_W, PILL_H, PILL_GAP, PILL_TOP = 34, 28, 8, 8

local function text_width(surface, value, size)
  local measured = surface:measure_text(value, size)
  if type(measured) == "table" and type(measured.width) == "number" then
    return measured.width
  end
  return #tostring(value or "") * (tonumber(size) or 10) * 0.58
end

-- Vertical centre of a text run inside a box. Text anchors top-left, so every
-- centred label needs its own baseline; guessing an offset per label is how
-- bars drift out of alignment when one string changes size.
local function centre_y(top, height, size)
  return top + (height - size * 1.2) / 2
end

function M.render(snapshot)
  snapshot = snapshot or {}
  local surface = ui.surface(WIDTH, HEIGHT, ui.theme())

  surface:rect("ground", 0, 0, WIDTH, HEIGHT, "background", 1)
  surface:rect("edge", 0, HEIGHT - 1, WIDTH, 1, "border", 0.9)

  -- Left: workspaces. The pill is the target; its label is centred on measured
  -- width so a renamed workspace does not lean.
  local workspaces = snapshot.workspaces or {}
  local x = 8
  for index, workspace in ipairs(workspaces) do
    local active = workspace.focused and true or false
    surface:rect("ws-" .. index, x, PILL_TOP, PILL_W, PILL_H,
      active and "accent" or "surface", active and 1 or 0.85, 9)
    local label = tostring(workspace.id)
    local size = 12
    surface:text("ws-label-" .. index, label,
      x + (PILL_W - text_width(surface, label, size)) / 2,
      centre_y(PILL_TOP, PILL_H, size), size,
      active and "background" or "muted")
    x = x + PILL_W + PILL_GAP
  end

  -- Centre: what has focus. Two lines, both centred on the whole surface.
  local active = snapshot.active or {}
  local title, app = active.title, active.app
  if title and title ~= "" then
    surface:text("active-title", title, (WIDTH - text_width(surface, title, 13)) / 2, 7, 13, "text")
  end
  if app and app ~= "" then
    surface:text("active-app", app, (WIDTH - text_width(surface, app, 10)) / 2, 25, 10, "muted")
  end

  -- Right: ambient facts, right-aligned and packed from the edge so a missing
  -- battery closes the gap instead of leaving a hole where it used to be.
  local ambient = snapshot.ambient or {}
  local right = WIDTH - 10
  local function push(id, value, size, ink)
    if not value or value == "" then return end
    local width = text_width(surface, value, size)
    surface:text(id, value, right - width, centre_y(0, HEIGHT, size), size, ink)
    right = right - width - 12
  end
  push("clock", ambient.clock, 13, "text")
  if right < WIDTH - 10 then
    surface:rect("divider", right, PILL_TOP, 1, PILL_H, "border", 0.8)
    right = right - 11
  end
  push("battery", ambient.battery, 11, "muted")
  push("network", ambient.network, 11, "muted")

  return surface
end

return M
