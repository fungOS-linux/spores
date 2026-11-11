-- Headless check for ui.host_surface. Run from spores/gallery:
--   lua tests/host_surface_fixture.lua

local source = assert(io.open("../../sdk/ui/ui.lua", "rb"), "ui library missing")
local ui = assert(load(source:read("*a"), "ui"))()
source:close()
_G.ui = ui
ui.debug = true
_G.module = {
  inputs = { width = 1200, height = 1800 },
  theme = { title = 28, large = 22, body = 16, small = 13, micro = 10, radius = 10 },
}
local hosts = {}
_G.canvas = {
  rect = function() end,
  text = function() end,
  host_resource = function(spec) hosts[#hosts + 1] = spec end,
}
_G.widget = { button = function() end }

local invalid = pcall(function()
  ui.host_surface("", { h = 400 })
end)
assert(not invalid, "host_surface should reject an empty resource alias")

ui.render(ui.column {
  h = 500,
  children = {
    ui.text("GPU layer", { style = "title" }),
    ui.host_surface("gpu-chart", { id = "chart", h = 400 }),
  },
}, _G.module.theme)

assert(#hosts == 1, "expected one host_resource placement")
assert(hosts[1].id == "chart", "host surface id")
assert(hosts[1].resource == "gpu-chart", "host surface resource")
assert(hosts[1].scale[1] > 0 and hosts[1].scale[2] > 0, "host surface geometry")
print("host_surface: ok")
