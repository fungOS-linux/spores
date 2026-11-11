-- Headless check for the Lua soil document backend. Run from spores/gallery:
--   lua tests/soil_fixture.lua

local source = assert(io.open("../../sdk/ui/ui.lua", "rb"), "ui library missing")
local ui = assert(load(source:read("*a"), "ui"))()
source:close()
_G.ui = ui
ui.debug = true
_G.module = {
  inputs = { width = 1200, height = 1800 },
  theme = { title = 28, large = 22, body = 16, small = 13, micro = 10, radius = 10 },
}

local drawn = { rects = {}, texts = {}, buttons = {}, hosts = {} }
local presented
_G.canvas = {
  rect = function(spec) drawn.rects[#drawn.rects + 1] = spec end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
  host_resource = function(spec) drawn.hosts[#drawn.hosts + 1] = spec end,
  present = function(document)
    presented = document
    return { generation = 9 }
  end,
}
_G.widget = { button = function(spec) drawn.buttons[#drawn.buttons + 1] = spec end }

local tree = ui.column {
  children = {
    ui.text("Soil title", { style = "title" }),
    ui.stat { label = "Throughput", value = 42, unit = "%" },
    ui.button("Inspect", { action = "inspect" }),
  },
}

local document = ui.document(tree, _G.module.theme)
assert(document.kind == "scene", "document kind")
assert(document.schema == 1, "document schema")
assert(type(document.nodes) == "table" and #document.nodes > 5, "document nodes")
assert(document.nodes[1].type == "rect", "background is a rect")
assert(document.nodes[1].ink == "background", "background ink")
assert(document.nodes[1].transform.scale[1] == 8 and document.nodes[1].transform.scale[2] == 8,
  "background covers the surface")

local has_text = false
for _, node in ipairs(document.nodes) do
  assert(node.id and node.transform and node.type, "every node has id/transform/type")
  if node.type == "text" and node.text == "Soil title" then has_text = true end
end
assert(has_text, "title text node missing")
assert(#drawn.rects == 0 and #drawn.texts == 0, "ui.document must not call the legacy canvas API")

local state = ui.present(tree, _G.module.theme)
assert(state.generation == 9, "present forwards the host state")
assert(presented and presented.kind == "scene", "present sends a soil scene document")
assert(#presented.nodes == #document.nodes, "document and present encode the same scene")
assert(#drawn.rects == 0 and #drawn.texts == 0, "soil present must not draw legacy primitives")
assert(#drawn.buttons == 1 and drawn.buttons[1].action == "inspect", "controls stay host-owned")

local direct = { kind = "scene", schema = 1, nodes = {} }
presented = nil
local direct_state = ui.present_document(direct)
assert(presented == direct and direct_state.generation == 9, "present_document passes a document through")

_G.canvas.present = nil
ui.present(tree, _G.module.theme)
assert(#drawn.rects > 0 and #drawn.texts > 0, "an older host falls back to canvas.rect/text")
print("soil_document: ok")
