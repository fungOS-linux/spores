-- Control center: the shell's own state, as one panel.
--
-- Every value comes from the host -- the clock, the focused window, the surface
-- count -- so the panel cannot disagree with the bar about any of them. Layout and
-- spacing come from widget.tokens, so a theme change moves this with everything
-- else instead of leaving one hardcoded gap behind.
local i, t = module.inputs, module.theme
local w, h = math.max(i.width or 380, 1), math.max(i.height or 520, 1)
local p = widget.pixel_space(w, h)
local k = widget.tokens()
local pad = k.space.lg
local inner = w - pad * 2

widget.glass({
  id = "panel", space = p, x = 0, y = 0, width = w, height = h, z = -1,
  ink = "surface", opacity = 1.0, radius = t.radius, elevation = k.elevation.floating,
  shadow_opacity = 0.4, transition_ms = k.motion.base,
})

local function text(id, value, px, py, size, ink)
  widget.label({ id = id, text = value, x = p.x(px), y = p.y(py), size = size, ink = ink or "text" })
end

-- The clock is the one thing here that is allowed to be large: everything below it
-- is a readout, and a panel of equally-sized lines has no entry point.
-- The label box is the type size, not the line box, so the date clears the clock
-- by a full line rather than by the difference between two guesses.
-- Bounded: a theme with a large display size should not push the date through the
-- clock, and the panel's upper block has a fixed budget either way.
local display = math.min(k.type.display * 1.3, 40)
text("clock", i.clock or "--:--", pad - 2, 8, display, "text")
text("date", i.date or "", pad, 14 + display * 1.1, k.type.small, "muted")

-- Quick actions before status, because a control center is something you act in,
-- not a dashboard you read. Three is the whole row: a fourth would need a grid,
-- and a grid in a 380 px panel is a phone home screen, not a desktop control.
local quick = {
  { id = "apps", label = "Applications", action = "apps" },
  { id = "projects", label = "Projects", action = "projects" },
  { id = "dismiss", label = "Close", action = "dismiss" },
}
local gap = k.space.sm
local pillw = (inner - gap * (#quick - 1)) / #quick
for index, item in ipairs(quick) do
  local left = pad + (index - 1) * (pillw + gap)
  -- A quiet button over a chip: the hit target stays the host's, and the chip is
  -- what makes three words in a row read as three things you can press.
  widget.panel({
    id = item.id .. "-chip", space = p, x = left, y = 76, width = pillw, height = 30,
    ink = "text", opacity = 0.07, radius = k.radius.md,
  })
  widget.button({
    id = item.id, label = item.label, appearance = "quiet", align = "center",
    action = item.action, body = { origin = "control-center" },
    x = p.x(left), y = p.y(76),
    width = p.w(pillw), height = p.h(30), enabled = true,
  })
end

local y = 120
local function section(id, title, top)
  widget.section({ id = id, space = p, title = title, x = pad, y = top, width = inner })
  return top + k.type.micro + k.space.md
end

y = section("session", "THIS SESSION", y)
local sessions = {
  { "workspace", "Workspace", i.workspace or "workspace" },
  { "focused", "Focused", i.focused or "no window focused" },
  { "surfaces", "Windows", tostring(i.surfaces or 0) },
}
for _, row in ipairs(sessions) do
  y = y + widget.list_row({
    id = "row-" .. row[1], space = p, x = pad, y = y, width = inner,
    title = row[2], value = row[3], height = k.density.row,
  })
end

-- Open windows, when the host knows them: a list of what is running is the one
-- thing a control center can offer that a bar cannot.
local windows = 0
local titles = {}
for n = 0, 4 do
  local title = i["window" .. n]
  if type(title) == "string" and title ~= "" then
    windows = windows + 1
    titles[#titles + 1] = title
  end
end
if windows > 0 then
  y = y + k.space.md
  y = section("running", "RUNNING", y)
  -- Only the rows that fit above the system block: a list that runs off the panel
  -- is a list that lost rows, not a list that is long.
  local room = math.max(0, math.floor((h - 150 - y) / k.density.row))
  for n, title in ipairs(titles) do
    if n > room then break end
    local short = title
    if #short > 26 then short = short:sub(1, 25) .. "…" end
    y = y + widget.list_row({
      id = "window-" .. n, space = p, x = pad, y = y, width = inner,
      title = short, height = k.density.row,
    })
  end
end

y = y + k.space.md
y = section("system", "SYSTEM", y)
local reading = tostring(i.battery or "")
local known = reading ~= "" and reading ~= "—" and reading ~= "-"
text("battery-label", "Battery", pad, y + 2, k.type.small, "muted")
text(
  "battery-value",
  known and reading or "no reading",
  pad + inner - #(known and reading or "no reading") * (k.type.small * 0.6),
  y + 2, k.type.small, known and "text" or "muted"
)
y = y + 24
if known then
  -- A gauge is drawn only when a fraction is real. A panel that fills a bar from
  -- a placeholder is lying at exactly the moment someone looks at it.
  local fraction = tonumber((reading:gsub("%%", ""))) or 0
  widget.gauge({
    id = "battery", space = p, x = pad, y = y, width = inner, height = 5,
    fraction = math.max(0, math.min(1, fraction / 100)), ink = "success",
  })
  y = y + 12
else
  widget.panel({
    id = "battery-unknown", space = p, x = pad, y = y, width = inner, height = 2,
    ink = "muted", opacity = 0.14, radius = 1,
  })
  y = y + 12
end
local sources = tonumber(i.sources) or 0
y = y + widget.list_row({
  id = "row-streams", space = p, x = pad, y = y, width = inner,
  title = "Live streams", value = tostring(sources), height = k.density.row,
})
