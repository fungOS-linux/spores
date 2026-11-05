-- Check a Spore view the way the host will: lay it out, then look for the
-- mistakes a single-frame screenshot hides.
--
--   lua tools/check-layout.lua
--
-- Reports text that overlaps other text, nodes drawn outside the surface, and
-- (because the library validates on construction) any token, style, or ink name
-- the view got wrong.

local theme = { title = 64, large = 40, body = 26, small = 22, micro = 18 }

local function read(path)
  local file = assert(io.open(path, "rb"), "missing " .. path)
  local text = file:read("*a")
  file:close()
  return text
end

local composed = table.concat({
  "local ui = (function()\n", read("ui/lib/ui.lua"), "\nend)()\n", read("ui/editor.lua"), "\n",
}, "\n")

local prims = {}
local record = function(kind)
  return function(props) props._kind = kind; prims[#prims + 1] = props end
end

local env = {
  module = { theme = theme, inputs = { width = 1200, height = 1800 } },
  canvas = { rect = record("rect"), text = record("text") },
  utf8 = utf8, print = print, math = math, string = string, table = table,
  pairs = pairs, ipairs = ipairs, type = type, tostring = tostring, error = error,
}

local chunk, err = load(composed, "spore-view", "t", env)
if not chunk then
  io.stderr:write("view does not compile: " .. tostring(err) .. "\n")
  os.exit(1)
end

local ok, run_err = pcall(chunk)
if not ok then
  io.stderr:write("view does not run: " .. tostring(run_err) .. "\n")
  os.exit(1)
end

-- Text boxes: the number is a font size, so the drawn glyph height is
-- size / surface-height and the advance is the scale squared times the body size.
local boxes = {}
for _, prim in ipairs(prims) do
  if prim._kind == "text" and prim.text and #prim.text > 0 then
    local size = prim.scale[1] * prim.scale[1] * theme.body
    local height = size / 215
    local chars = utf8 and utf8.len(prim.text) or #prim.text
    local width = math.max(1, chars or #prim.text) * size * 0.62 / 215
    boxes[#boxes + 1] = {
      text = prim.text,
      x = prim.translation[1], y = prim.translation[2],
      w = width, h = height,
    }
  end
end

local problems = 0
local function report(message)
  problems = problems + 1
  io.write(string.format("  %s\n", message))
end

for i = 1, #boxes do
  local a = boxes[i]
  if a.x < -4.01 or a.x + a.w > 4.01 or a.y > 4.01 or a.y - a.h < -4.01 then
    report(string.format("outside surface: %q at (%.2f, %.2f)", a.text, a.x, a.y))
  end
  for j = i + 1, #boxes do
    local b = boxes[j]
    local ox = math.min(a.x + a.w, b.x + b.w) - math.max(a.x, b.x)
    local oy = math.min(a.y, b.y) - math.max(a.y - a.h, b.y - b.h)
    if ox > 0.02 and oy > a.h * 0.25 then
      report(string.format("overlap: %q (%.2f) and %q (%.2f) %.3f x %.3f", a.text, a.x, b.text, b.x, ox, oy))
    end
  end
end

print(string.format("%d primitives, %d text boxes, %d problems", #prims, #boxes, problems))
os.exit(problems == 0 and 0 or 2)
