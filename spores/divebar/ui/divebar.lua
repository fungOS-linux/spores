-- Divebar: the system bar as an SDK view.
--
-- Authored at 1200x44 and authority-free: everything it draws comes from the
-- snapshot. The host decides the extent it grants and says so in
-- `module.viewport`; the fallback below is only what a host that says nothing
-- gets. The manifest declares the region's height, so a second copy here would
-- be a fact with two owners.

local ui = require("fungos.ui")

local M = { name = "dev.fungos.divebar", version = "0.1.0" }

local FALLBACK_WIDTH, FALLBACK_HEIGHT = 1200, 44
local PILL_W, PILL_H, PILL_GAP = 34, 28, 8

local function extent()
  local viewport = module and module.viewport or {}
  return tonumber(viewport.width) or FALLBACK_WIDTH,
    tonumber(viewport.height) or FALLBACK_HEIGHT
end

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

-- Cut a run to the width it is allowed, with an ellipsis. A bar that lets a
-- title run under the clock looks like a clock that cannot be read.
local function fit(surface, value, size, limit)
  value = tostring(value or "")
  if text_width(surface, value, size) <= limit then return value end
  local text = value
  while #text > 1 and text_width(surface, text .. "…", size) > limit do
    text = text:sub(1, -2)
  end
  return text .. "…"
end

function M.render(snapshot)
  snapshot = snapshot or {}
  local WIDTH, HEIGHT = extent()
  local surface = ui.surface(WIDTH, HEIGHT, ui.theme())

  surface:rect("ground", 0, 0, WIDTH, HEIGHT, "background", 1)
  surface:rect("edge", 0, HEIGHT - 1, WIDTH, 1, "border", 0.9)

  -- Left: workspaces. The pill is the target; its label is centred on measured
  -- width so a renamed workspace does not lean.
  local workspaces = snapshot.workspaces or {}
  local pill_top = math.floor((HEIGHT - PILL_H) / 2)
  local x = 8
  for index, workspace in ipairs(workspaces) do
    local active = workspace.focused and true or false
    local label = tostring(workspace.id)
    local size = 12
    surface:rect("ws-" .. index, x, pill_top, PILL_W, PILL_H,
      active and "accent" or "surface", active and 1 or 0.85, 9)
    surface:text("ws-label-" .. index, label,
      x + (PILL_W - text_width(surface, label, size)) / 2,
      centre_y(pill_top, PILL_H, size), size,
      active and "background" or "muted")
    x = x + PILL_W + PILL_GAP
  end
  local left_free = x + 8

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
    surface:rect("divider", right, pill_top, 1, PILL_H, "border", 0.8)
    right = right - 11
  end
  push("battery", ambient.battery, 11, "muted")
  push("network", ambient.network, 11, "muted")

  -- Centre: what has focus, in the space the two sides actually left. Both
  -- lines are centred as one block, so a missing app leaves the title centred
  -- rather than sitting above a gap.
  local active = snapshot.active or {}
  local gap = right - left_free
  if gap > 40 then
    local title = fit(surface, active.title, 13, gap)
    local app = fit(surface, active.app, 10, gap)
    local block = 0
    if title ~= "" then block = block + 13 * 1.2 end
    if app ~= "" then block = block + 10 * 1.2 end
    local top = (HEIGHT - block) / 2
    if title ~= "" then
      surface:text("active-title", title,
        left_free + (gap - text_width(surface, title, 13)) / 2, top, 13, "text")
      top = top + 13 * 1.2
    end
    if app ~= "" then
      surface:text("active-app", app,
        left_free + (gap - text_width(surface, app, 10)) / 2, top, 10, "muted")
    end
  end

  return surface
end

return M
