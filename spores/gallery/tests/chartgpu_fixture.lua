-- Headless check for the ChartGPU fixture. Run from spores/gallery:
--   lua tests/chartgpu_fixture.lua

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

local view = assert(loadfile("ui/chartgpu.lua"))()
view.render({})

local function has_text(value)
  for _, node in ipairs(drawn.texts) do
    if node.text == value then return true end
  end
  return false
end

assert(has_text("ChartGPU · decimated"), "fixture title missing")
for _, title in ipairs({ "Response latency", "Throughput", "CPU / memory", "Errors", "Connections" }) do
  assert(has_text(title), "panel missing: " .. title)
end
assert(#drawn.rects > 40, "expected chart marks")
assert(#drawn.rects + #drawn.texts <= 256, "fixture exceeds the default Canvas node budget")
print(string.format("chartgpu: ok (%d rects, %d texts, %d buttons)",
  #drawn.rects, #drawn.texts, #drawn.buttons))
