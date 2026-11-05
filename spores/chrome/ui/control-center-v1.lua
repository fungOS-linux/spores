-- Control center: the shell's own state, as one panel.
--
-- Every value comes from the host -- the clock, the focused window, the surface
-- count -- so the panel cannot disagree with the bar about any of them. Layout and
-- spacing come from widget.tokens, so a theme change moves this with everything
-- else instead of leaving one hardcoded gap behind.
local i, t = module.inputs, module.theme
local w, h = math.max(i.width or 360, 1), math.max(i.height or 420, 1)
local p = widget.pixel_space(w, h)
local k = widget.tokens()
local pad = k.space.lg

widget.glass({
  id = "panel", space = p, x = 0, y = 0, width = w, height = h, z = -1,
  ink = "surface", opacity = 1.0, radius = t.radius, elevation = k.elevation.floating,
  shadow_opacity = 0.4, transition_ms = k.motion.base,
})

local function text(id, value, px, py, size, ink)
  widget.label({ id = id, text = value, x = p.x(px), y = p.y(py), size = size, ink = ink or "text" })
end

text("clock", i.clock or "--:--", pad, 12, k.type.display, "text")
text("date", i.date or "", pad + 2, 46, k.type.small, "muted")

local y = 84
widget.section({ id = "session", space = p, title = "SESSION", x = pad, y = y, width = w - pad * 2 })
y = y + 18

local sessions = {
  { "workspace", "Workspace", i.workspace or "workspace" },
  { "focused", "Focused", i.focused or "no window focused" },
  { "surfaces", "Windows", tostring(i.surfaces or 0) },
}
for _, row in ipairs(sessions) do
  widget.list_row({
    id = "row-" .. row[1], space = p, x = pad, y = y, width = w - pad * 2,
    title = row[2], value = row[3], height = k.density.row,
  })
  y = y + k.density.row
end

y = y + k.space.md
widget.section({ id = "system", space = p, title = "SYSTEM", x = pad, y = y, width = w - pad * 2 })
y = y + 22

text("battery-label", "Battery", pad, y, k.type.small, "muted")
local reading = i.battery or "—"
text("battery-value", reading, pad + 190 - #tostring(reading) * (k.type.small * 0.6), y, k.type.small, "text")
y = y + 30
widget.gauge({ id = "battery", space = p, x = pad, y = y, width = w - pad * 2, height = 6, fraction = 0.62, ink = "success" })

widget.button({
  id = "apps", label = "Applications", appearance = "quiet", align = "left",
  action = "launcher", body = { origin = "control-center" },
  x = p.x(pad), y = p.y(h - 92), width = p.w(w - pad * 2), height = p.h(k.density.control), enabled = true,
})
widget.button({
  id = "dismiss", label = "Close", appearance = "quiet", align = "left",
  action = "dismiss", body = { origin = "control-center" },
  x = p.x(pad), y = p.y(h - 50), width = p.w(w - pad * 2), height = p.h(k.density.control), enabled = true,
})
