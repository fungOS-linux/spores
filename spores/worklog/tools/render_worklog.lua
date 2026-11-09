-- Draw the worklog view to an SVG, at the size the wall actually hands it.
--
--   lua tools/render_worklog.lua [snapshot.lua] [out.svg] [--width N] [--height N]
--   rsvg-convert -w 1182 out.svg -o out.png
--
-- `tend` needs a compositor and a Canvas that is willing to paint; this needs
-- neither. It runs the real view through the real Lua layout fallback, turns the
-- same `canvas.rect`/`canvas.text` calls the host receives into SVG, and prints a
-- picture of them. The point is geometry: whether the columns line up, whether a
-- row is tall enough for the ink in it, whether the page is full.
--
-- It is a review aid, so the two things it cannot reproduce are named rather than
-- faked: font metrics (Canvas draws roughly 0.8 em per character, so the SVG uses
-- a monospace face with letter-spacing that lands on the same advance) and the
-- host's own antialiasing. Colours are the dark theme's roles, not a theme dump.

local snapshot_path = arg[1] or "examples/worklog_mock.lua"
local out_path = arg[2] or "/tmp/worklog.svg"
local W, H, scroll = 1182, 1877, nil
for index = 1, #arg do
  if arg[index] == "--width" then W = tonumber(arg[index + 1]) or W end
  if arg[index] == "--height" then H = tonumber(arg[index + 1]) or H end
  if arg[index] == "--scroll" then scroll = tonumber(arg[index + 1]) end
end

local source = assert(io.open("../unibus/ui/lib/ui.lua", "rb"),
  "../unibus/ui/lib/ui.lua is missing; the worklog view composes the shared kit")
local ui = assert(load(source:read("*a"), "ui/lib/ui.lua"))()
source:close()
_G.ui = ui

-- `module.theme.body` is the size `ts()` divides by, so it has to be the host's:
-- a host that passes 16 draws the view's "12" as 12 px, which is the measurement
-- the wrap budget is built on.
_G.module = {
  viewport = { width = W, height = H },
  theme = { title = 0.34, large = 0.24, body = 16, small = 0.105, micro = 0.085, radius = 0.02 },
}
if scroll then _G.module.scroll = { offset = scroll } end

local drawn = { rects = {}, texts = {}, presses = {} }
_G.canvas = {
  rect = function(spec) drawn.rects[#drawn.rects + 1] = spec end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
}
_G.widget = { button = function(spec) drawn.presses[#drawn.presses + 1] = spec end }

local worklog = assert(loadfile("ui/worklog.lua"))()
local snapshot = assert(loadfile(snapshot_path))()
worklog.render(snapshot)

local INK = {
  background = "#0c0e0d", surface = "#171b1c", text = "#e8eae9",
  muted = "#8e9598", primary = "#8ab4f8", success = "#81c995",
  warning = "#fdd663", danger = "#f28b82", info = "#8ab4f8",
}
local function ink(name) return INK[name] or INK.text end

-- Normalized canvas units are 8 wide and 8 tall over the whole surface, with the
-- origin top-left at (-4, 4): `nx(px) = -4 + 8*px/W`, `ny(px) = 4 - 8*px/H`.
local function px(x) return (x + 4) / 8 * W end
local function py(y) return (4 - y) / 8 * H end

local out = {
  string.format('<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d" viewBox="0 0 %d %d">', W, H, W, H),
  string.format('<rect width="%d" height="%d" fill="%s"/>', W, H, INK.background),
}

for _, spec in ipairs(drawn.rects) do
  local x, y = px(spec.translation[1]), py(spec.translation[2])
  local w, h = spec.scale[1] / 8 * W, spec.scale[2] / 8 * H
  out[#out + 1] = string.format(
    '<rect x="%.2f" y="%.2f" width="%.2f" height="%.2f" rx="%.2f" fill="%s" fill-opacity="%.2f"/>',
    x, y, w, h, (spec.radius or 0) / 8 * W, ink(spec.ink), spec.opacity or 1)
end

for _, spec in ipairs(drawn.texts) do
  local x, y = px(spec.translation[1]), py(spec.translation[2])
  -- A text primitive carries a scale, not a font size: the host multiplies it by
  -- the theme's base and a 1.25 line factor, which is what makes the view's
  -- `size = 12` arrive as 12 px.
  local size = spec.scale[1] * 16 * 1.25
  out[#out + 1] = string.format(
    '<text x="%.2f" y="%.2f" font-family="Menlo, monospace" font-size="%.2f" letter-spacing="%.2f" fill="%s">%s</text>',
    x, y + size * 0.75, size, size * 0.2, ink(spec.ink), (spec.text:gsub("&", "&amp;"):gsub("<", "&lt;"):gsub(">", "&gt;")))
end

-- Buttons are host-owned pixels, so they are not in the scene; outline them to
-- show where a locator is pressable.
for _, spec in ipairs(drawn.presses) do
  local x, y = px(spec.x), py(spec.y)
  out[#out + 1] = string.format(
    '<rect x="%.2f" y="%.2f" width="%.2f" height="%.2f" fill="none" stroke="%s" stroke-opacity="0.5" stroke-dasharray="3 3"/>',
    x, y, spec.width / 8 * W, spec.height / 8 * H, INK.primary)
end
out[#out + 1] = "</svg>"

local file = assert(io.open(out_path, "wb"))
file:write(table.concat(out, "\n"))
file:close()
print(string.format("wrote %s (%dx%d): %d rects, %d text, %d pressable, %s rows at %d px pitch",
  out_path, W, H, #drawn.rects, #drawn.texts, #drawn.presses,
  tostring(worklog.rows_drawn), worklog.row_height))