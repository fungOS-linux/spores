-- Headless check for the fleet view. Run from spores/fleet:
--   lua tests/ui_fixture.lua
--
-- The snapshot is the committed fixture, so this fails when the projection
-- changes shape or the view starts drawing outside the surface.

local source = assert(io.open("../../sdk/ui/ui.lua", "rb"), "ui library missing")
local ui = assert(load(source:read("*a"), "ui"))()
source:close()
package.preload["fungos.ui"] = function() return ui end
ui.debug = true

_G.module = {
  viewport = { width = 1600, height = 1000 },
  inputs = { width = 1600, height = 1000 },
  theme = { title = 28, large = 22, body = 16, small = 13, micro = 15, radius = 10 },
}

local drawn = { rects = {}, texts = {}, buttons = {} }
_G.canvas = {
  rect = function(spec) drawn.rects[#drawn.rects + 1] = spec end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
}
_G.widget = { button = function(spec) drawn.buttons[#drawn.buttons + 1] = spec end }

local view = assert(loadfile("ui/fleet.lua"))()
local snapshot = assert(loadfile("examples/fleet_snapshot.lua"))()
view.render(snapshot)

local function has_text(value)
  for _, node in ipairs(drawn.texts) do
    if node.text == value then return true end
  end
  return false
end

local function count_actions(action)
  local count = 0
  for _, button in ipairs(drawn.buttons) do
    if button.action == action then count = count + 1 end
  end
  return count
end

assert(#drawn.rects > 80, "expected panels, marks and edges")
assert(#drawn.texts > 80, "expected labels and table cells")
assert(has_text("FLEET"), "masthead missing")
assert(has_text("mycelium · fungOS"), "subtitle missing")
assert(has_text("Mesh topology"), "graph card missing")
assert(has_text("Attention"), "attention table missing")
assert(has_text("Sites"), "sites table missing")
assert(has_text("Release drift"), "release gauge missing")
assert(has_text("wagar-house"), "a site label is missing")
assert(has_text("pris"), "a live host label is missing")

-- Edges are rotated rects. A view that lost them would still draw every dot,
-- so the count is what proves the graph is a graph and not a scatter.
local rotated = 0
for _, rect in ipairs(drawn.rects) do
  if rect.rotation then rotated = rotated + 1 end
end
assert(rotated >= 20, "expected rotated site-to-host edges, got " .. rotated)

-- The node budget the wall enforces. 256 primitives is the stock Canvas cap;
-- a view above it composes fine and then fails on the wall.
local total = #drawn.rects + #drawn.texts
assert(total <= 256, "fleet view exceeds the 256-primitive wall budget: " .. total)

assert(count_actions("fleet-inspect-count") == 4, "stat drill-ins missing")
assert(count_actions("fleet-inspect-peer") > 20, "graph node hit regions missing")
assert(count_actions("fleet-filter-site") == #snapshot.sites.rows, "site rows are not pressable")
assert(count_actions("fleet-inspect-release") == 1, "gauge hit region missing")

-- Every primitive is in the 8-unit surface. Off-surface text is the failure a
-- preview cannot show: the wall simply clips it and reads as a missing row.
for _, list in ipairs({ drawn.rects, drawn.texts }) do
  for _, node in ipairs(list) do
    local x, y = node.translation[1], node.translation[2]
    assert(x >= -4.01 and x <= 4.01, "primitive outside the surface x: " .. x)
    assert(y >= -4.01 and y <= 4.01, "primitive outside the surface y: " .. y)
  end
end

print(string.format("fleet: ok (%d rects, %d texts, %d edges, %d hit regions, %d primitives)",
  #drawn.rects, #drawn.texts, rotated, #drawn.buttons, total))
