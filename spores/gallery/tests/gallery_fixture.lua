-- Headless primitive check for the gallery. Run from spores/gallery:
--   lua tests/gallery_fixture.lua

local source = assert(io.open("../../sdk/ui/ui.lua", "rb"), "ui library missing")
local ui = assert(load(source:read("*a"), "ui"))()
source:close()
_G.ui = ui
ui.debug = true
_G.module = {
  inputs = { width = 1200, height = 2400 },
  theme = { title = 28, large = 22, body = 16, small = 13, micro = 10, radius = 10 },
}
local drawn = { rects = {}, texts = {}, buttons = {} }
_G.canvas = {
  rect = function(spec) drawn.rects[#drawn.rects + 1] = spec end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
}
_G.widget = { button = function(spec) drawn.buttons[#drawn.buttons + 1] = spec end }

local view = assert(loadfile("ui/gallery.lua"))()
view.render({})

local function has_text(value)
  for _, node in ipairs(drawn.texts) do
    if node.text == value then return true end
  end
  return false
end

assert(#drawn.rects > 100, "expected dashboard rects")
assert(#drawn.texts > 20, "expected dashboard text")
assert(has_text("Dashboard primitives"), "gallery title missing")
assert(has_text("Throughput"), "stat label missing")
assert(has_text("CPU pressure"), "gauge title missing")
assert(has_text("Services"), "table title missing")
assert(has_text("Request latency"), "chart title missing")
assert(has_text("Topology"), "graph title missing")
assert(has_text("canvas") and has_text("soil"), "graph node labels missing")
assert(has_text("p50") and has_text("p95") and has_text("p99"), "chart legend missing")

local function count_buttons(action)
  local count = 0
  for _, button in ipairs(drawn.buttons) do
    if button.action == action then count = count + 1 end
  end
  return count
end

assert(count_buttons("inspect-stat") == 3, "stat drill-in hit regions missing")
assert(count_buttons("inspect-gauge") == 1, "gauge hit region missing")
assert(count_buttons("open-service") == 4, "table row hit regions missing")
assert(count_buttons("toggle-series") == 3, "legend hit regions missing")
assert(count_buttons("inspect-node") == 7, "graph node hit regions missing")
assert(count_buttons("select-range") == 1, "chart selection hit region missing")

-- Dense series are decimated before they become bars. LTTB must keep the
-- endpoints and the dominant spike, not average it away.
local dense = {}
for index = 1, 5000 do
  local x = index / 5000
  dense[index] = { x = x, y = math.sin(x * 20) + (index == 2500 and 4 or 0) }
end
local reduced = ui.lttb(dense, 200)
assert(#reduced == 200, "lttb should honor the threshold")
assert(reduced[1] == dense[1] and reduced[#reduced] == dense[#dense], "lttb should keep endpoints")
local spike = false
for _, point in ipairs(reduced) do
  if point == dense[2500] then spike = true end
end
assert(spike, "lttb should keep a dominant spike")
assert(#ui.lttb(dense, 6000) == 5000, "lttb should leave small series alone")
assert(#ui.chart_window(dense, 100) == 100, "chart window should keep the newest points")
assert(ui.chart_window(dense, 100)[1] == dense[4901], "chart window should start at the right point")

-- The graph draws its edges as rotated rects; a gallery that lost them would
-- still draw every node and read as a scatter, which is the bug this catches.
local rotated = 0
for _, rect in ipairs(drawn.rects) do
  if rect.rotation then rotated = rotated + 1 end
end
assert(rotated >= 6, "graph edges are missing")

print(string.format("gallery: ok (%d rects, %d texts, %d hit regions, %d edges)",
  #drawn.rects, #drawn.texts, #drawn.buttons, rotated))
