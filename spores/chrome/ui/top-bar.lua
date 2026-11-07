-- Presentation only. The native shell owns these four allowlisted actions.
--
-- The bar is a row, not a toolbar: identity, one workspace control, whatever is
-- running, and the clock. Anything that would need a second row belongs in the
-- control center, which is where a person looks when they want more.
local i, t = module.inputs, module.theme
local w, h = math.max(i.width, 1), math.max(i.height, 1)
local p = widget.pixel_space(w, h)
local k = widget.tokens()

-- Labels are centred on the row, not stacked from the top: the bar's height is now
-- smaller than the sum of the pieces a naive layout would stack, and centring is
-- what keeps a 36 px row from looking like a clipped 44 px one.
local cap = (h - k.type.small) * 0.5
local function label(id, value, px, ink, size)
  widget.label({
    id = id, text = value, x = p.x(px), y = p.y(cap),
    size = size or k.type.small, ink = ink or "muted",
  })
end
local function control(id, value, px, width, enabled)
  widget.button({
    id = id, label = value, appearance = "quiet", action = id,
    body = { origin = "desktop-shell" }, enabled = enabled ~= false,
    x = p.x(px), y = 0, width = p.w(width), height = 8,
  })
end

widget.panel {
  id = "ground", x = -4, y = 4, width = 8, height = 8,
  ink = "surface", opacity = 1, radius = 0,
}
-- A hairline instead of a border: at 36 px a full stroke reads as a divider
-- between two bars rather than the edge of one.
widget.panel {
  id = "rule", x = -4, y = p.y(h - 1), width = 8, height = p.h(1),
  ink = "muted", opacity = 0.14, radius = 0,
}

local pad = i.padding or k.space.md
local left = pad
if w >= 560 then
  -- The identity gets a column, not a position: a label that grows into the next
  -- control is the classic hardcoded-offset bug.
  local identity = 74
  label("identity", "fungOS", left, "primary")
  control("back", "‹", left + identity, 22, i.back)
  control("forward", "›", left + identity + 26, 22, i.forward)
  left = left + identity + 54
end

-- The workspace control is the only thing on the row that carries a chevron: it
-- opens a menu, and everything else here is a button or a readout.
local clock_width = w >= 320 and 60 or 0
local project_width = math.max(28, math.min(190, w - left - clock_width - pad))
local project = tostring(i.workspace or "workspace")
if #project > 18 then project = project:sub(1, 17) .. "…" end
control("launcher", project .. " ▾", left, project_width, true)
left = left + project_width + pad

if w >= 900 and i.sources and i.sources > 0 then
  label("sources", tostring(i.sources) .. " live", w - 176, "muted")
end
-- The clock is right-aligned by measurement, not by a magic offset: it is the one
-- readout whose width changes every minute.
if clock_width > 0 then
  local clock = tostring(i.clock or "--:--"):sub(1, 5)
  local width = #clock * k.type.small * 0.62
  label("clock", clock, w - pad - width, "text", k.type.small)
end
