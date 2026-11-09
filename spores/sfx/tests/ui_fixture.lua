-- Fixture for the SFX view against the latest fungos.ui API.
--
-- Run from spores/sfx:
--   FUNGOS_SDK=$HOME/src/fungOS/fungos-sdk lua tests/ui_fixture.lua
--
-- The view draws a portable scene; this asserts the scene and its hit targets,
-- not a private layout representation.

local function sdk_root()
  return os.getenv("FUNGOS_SDK") or (os.getenv("HOME") .. "/src/fungOS/fungos-sdk")
end

local path = sdk_root() .. "/lua/fungos/ui.lua"
local file = io.open(path, "rb")
if not file then
  io.stderr:write("skipping: no fungos.ui at " .. path .. "\n")
  os.exit(0)
end
local source = file:read("*a")
file:close()

local chunk = assert(load(source, path))
local ui = chunk()
package.preload["fungos.ui"] = function() return ui end
package.loaded["fungos.ui"] = ui

local board = assert(loadfile("ui/board.lua"))()
local snapshot = assert(loadfile("ui/board.example.lua"))()

local present, hits = ui.capture(1200, 900, ui.theme(), function()
  board.render(snapshot)
end)

assert(type(present) == "table" and present.kind == "scene", "view did not produce a scene")
assert(#present.nodes > 100, "the board painted too few nodes: " .. #present.nodes)
assert(#hits == #snapshot.pads + #snapshot.rules, string.format("expected %d hits, got %d", #snapshot.pads + #snapshot.rules, #hits))

local ids = {}
for _, hit in ipairs(hits) do ids[hit.id] = true end
for _, pad in ipairs(snapshot.pads) do
  assert(ids["sfx-pad-" .. pad.id], "missing hit for pad " .. pad.id)
end

local text = {}
for _, node in ipairs(present.nodes) do
  if node.type == "text" then text[#text + 1] = node.text end
end
local copy = table.concat(text, "\n")
for _, expected in ipairs({ "SFX", "EVENT MAPPING", "PADS" }) do
  assert(copy:find(expected, 1, true), "missing text: " .. expected)
end

print(string.format("ui: ok (%d nodes, %d hits)", #present.nodes, #hits))
