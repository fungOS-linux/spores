-- Headless primitive check for the gallery. Run from spores/gallery:
--   lua tests/gallery_fixture.lua

local source = assert(io.open("../../sdk/ui/ui.lua", "rb"), "ui library missing")
local ui = assert(load(source:read("*a"), "ui"))()
source:close()
_G.ui = ui
ui.debug = true
_G.module = {
  inputs = { width = 1200, height = 1800 },
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
assert(count_buttons("select-range") == 1, "chart selection hit region missing")
print(string.format("gallery: ok (%d rects, %d texts, %d hit regions)",
  #drawn.rects, #drawn.texts, #drawn.buttons))
