-- Window chrome. Canvas owns the window, drag/focus/resize and actions; Lua owns
-- how the titlebar reads. Buttons are local hits mapped back to ChromeAction.
local i, t = module.inputs, module.theme
local w, h = math.max(i.width or 720, 1), math.max(i.height or 32, 1)
local p = widget.pixel_space(w, h)
local focused = i.focused == true
local title = i.title or ""

widget.panel {
  id = "ground", x = -4, y = 4, width = 8, height = 8,
  ink = "surface", opacity = 1.0, radius = 0,
}

local controls = {
  { id = "close", label = "Close", ink = "danger", x = 10 },
  { id = "minimize", label = "Minimize", ink = "warning", x = 26 },
  { id = "maximize", label = "Maximize", ink = "success", x = 42 },
}
for _, control in ipairs(controls) do
  widget.panel {
    id = control.id .. "-mark", x = p.x(control.x), y = p.y(12),
    width = p.w(10), height = p.h(10), ink = control.ink,
    opacity = focused and 0.92 or 0.48, radius = 5,
  }
  widget.button {
    id = control.id, label = control.label, appearance = "hit", local = true,
    x = p.x(control.x - 4), y = 4, width = p.w(18), height = 8,
    enabled = i[control.id] ~= false,
  }
end

local left = 64
local room = math.max(40, w - left - 18)
local chars = math.max(4, math.floor(room / ((t.small or 11) * 0.58)))
if utf8.len(title) > chars then
  local cut = utf8.offset(title, chars + 1)
  title = cut and (title:sub(1, cut - 1) .. "…") or title
end
widget.label {
  id = "title", text = title,
  x = p.x(left), y = p.y((h - (t.small or 11)) / 2 - 1),
  size = t.small or 11, ink = focused and "text" or "muted",
}
