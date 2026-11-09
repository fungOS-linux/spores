-- Preview the minimal pad surface without Canvas.
--
--   lua tests/pads_fixture.lua

local function library()
  local source = assert(io.open("../../sdk/ui/ui.lua", "rb"),
    "../../sdk/ui/ui.lua is missing; run from spores/sfx")
  local chunk = assert(load(source:read("*a"), "sdk/ui/ui.lua"))
  source:close()
  return chunk()
end

package.preload["fungos.ui"] = library

local drawn = { texts = {}, buttons = {}, rects = 0 }
_G.module = { inputs = { width = 1200, height = 900 } }
_G.canvas = {
  rect = function() drawn.rects = drawn.rects + 1 end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
}
_G.widget = { button = function(spec) drawn.buttons[#drawn.buttons + 1] = spec end }

local pads = assert(loadfile("ui/pads.lua"))()
local snapshot = assert(loadfile("ui/board.example.lua"))()
pads.render(snapshot)

assert(#drawn.texts > 10, "the pad surface painted almost nothing")
assert(drawn.rects > 0, "the pad surface painted no tiles")
local found = false
for _, spec in ipairs(drawn.texts) do
  assert(type(spec.text) == "string")
  assert(type(spec.ink) == "string")
  if spec.text == "SFX" then found = true end
end
assert(found, "the pad title is missing")
print(string.format("pads: ok (%d text, %d buttons)", #drawn.texts, #drawn.buttons))
