-- Preview the minimal pad surface without Canvas.
--
--   FUNGOS_SDK=$HOME/src/fungOS/fungos-sdk lua tests/pads_fixture.lua

local root = os.getenv("FUNGOS_SDK") or (os.getenv("HOME") .. "/src/fungOS/fungos-sdk")
local path = root .. "/lua/fungos/ui.lua"
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

local pads = assert(loadfile("ui/pads.lua"))()
local snapshot = assert(loadfile("ui/board.example.lua"))()
local present, hits = ui.capture(1200, 900, ui.theme(), function()
  pads.render(snapshot)
end)

assert(#present.nodes > 20, "the pad surface painted almost nothing")
assert(#hits == #snapshot.pads, string.format("expected %d pad hits, got %d", #snapshot.pads, #hits))
local found = false
for _, node in ipairs(present.nodes) do
  if node.type == "text" and node.text == "SFX" then found = true end
end
assert(found, "the pad title is missing")
print(string.format("pads: ok (%d nodes, %d hits)", #present.nodes, #hits))
