-- Control center: the shell's own status, as one panel.
--
-- It reads only what the host already knows -- the clock, the focused surface,
-- how many windows are open -- so it cannot disagree with the bar about any of
-- them. Toggling things belongs here later; reading them is the part that has to
-- be honest first.
local i, t = module.inputs, module.theme
local w, h = math.max(i.width or 360, 1), math.max(i.height or 420, 1)
local p = widget.pixel_space(w, h)
local pad = 18

widget.panel({ id = "ground", x = -4, y = 4, width = 8, height = 8,
  ink = "surface", opacity = 1.0, radius = t.radius, z = -1 })

local function text(id, value, px, py, size, ink)
  widget.label({ id = id, text = value, x = p.x(px), y = p.y(py), size = size or t.body, ink = ink or "text" })
end

text("clock", i.clock or "--:--", pad, 14, t.title, "text")
text("date", i.date or "", pad, 52, t.small, "muted")

widget.panel({ id = "rule", x = p.x(pad), y = p.y(80), width = p.w(w - pad * 2), height = p.h(1),
  ink = "muted", opacity = 0.16, radius = 0 })

local rows = {
  { "workspace", i.workspace or "workspace", "text" },
  { "focused", i.focused or "no window focused", "text" },
  { "surfaces", tostring(i.surfaces or 0) .. " surfaces", "muted" },
  { "battery", i.battery or "—", "muted" },
}
for index, row in ipairs(rows) do
  local y = 96 + (index - 1) * 38
  widget.label({ id = "row-" .. row[1], text = row[1], x = p.x(pad), y = p.y(y), size = t.small, ink = "muted" })
  widget.label({ id = "value-" .. row[1], text = row[2], x = p.x(pad + 110), y = p.y(y), size = t.small, ink = row[3] })
end

widget.button({ id = "apps", label = "Applications", appearance = "quiet", align = "left",
  action = "launcher", body = { origin = "control-center" },
  x = p.x(pad), y = p.y(h - 96), width = p.w(w - pad * 2), height = p.h(34), enabled = true })
widget.button({ id = "dismiss", label = "Close", appearance = "quiet", align = "left",
  action = "dismiss", body = { origin = "control-center" },
  x = p.x(pad), y = p.y(h - 54), width = p.w(w - pad * 2), height = p.h(34), enabled = true })
