-- Fixture for ui/board.lua. Run from spores/sfx:
--   lua tests/ui_fixture.lua
--
-- The view is self-painting: it calls `ui.render`, which emits Canvas primitives
-- and host-owned `widget.button` controls. So this asserts on what was drawn,
-- not on a tree some other component is supposed to interpret.

local function library()
  local source = assert(io.open("../../sdk/ui/ui.lua", "rb"),
    "../../sdk/ui/ui.lua is missing; the view reads the package this repo declares")
  local chunk = assert(load(source:read("*a"), "sdk/ui/ui.lua"))
  source:close()
  return chunk()
end

package.preload["fungos.ui"] = library

local drawn = { texts = {}, buttons = {}, rects = 0 }

local function reset()
  drawn = { texts = {}, buttons = {}, rects = 0 }
end

_G.module = {
  inputs = { width = 1200, height = 900 },
  theme = { title = 28, large = 22, body = 15, small = 12, micro = 9, radius = 9 },
}
_G.canvas = {
  rect = function() drawn.rects = drawn.rects + 1 end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
}
_G.widget = { button = function(spec) drawn.buttons[#drawn.buttons + 1] = spec end }

local board = assert(loadfile("ui/board.lua"))()
local view = assert(loadfile("ui/board.example.lua"))()

local function render(snapshot)
  reset()
  board.render(snapshot)
  local copy, signature = {}, {}
  for _, spec in ipairs(drawn.texts) do
    copy[#copy + 1] = tostring(spec.text)
    signature[#signature + 1] = tostring(spec.text) .. ":" .. tostring(spec.ink)
  end
  return table.concat(copy, "\n"), table.concat(signature, "\n")
end

local copy, signature = render(view)
local full_text, full_buttons = #drawn.texts, #drawn.buttons

assert(#drawn.texts > 20, "the view painted almost nothing")
assert(drawn.rects > 0, "the view painted no rects")
for _, spec in ipairs(drawn.texts) do
  assert(type(spec.text) == "string", "a text primitive has no string")
  assert(type(spec.ink) == "string", "a text primitive has no ink role")
end

assert(copy:find("EVENT MAPPING", 1, true), "the mapping section is missing")
assert(copy:find("PADS", 1, true), "the pad section is missing")
assert(copy:find("Home Pi", 1, true), "the live room is not drawn")
assert(copy:find("missing blake3 content id", 1, true), "an unavailable pad does not say why")
assert(copy:find("room: home-pi", 1, true), "the opened room is not drawn")
assert(copy:find("48 kHz stereo", 1, true), "the output format is not drawn")

-- Every pad is a control, including the unavailable one (disabled, so the host
-- can explain it rather than silently omitting it).
local triggers = 0
for _, spec in ipairs(drawn.buttons) do
  if spec.action == "sfx-trigger" and spec.body and spec.body.action == "sfx.trigger" then
    triggers = triggers + 1
  end
end
assert(triggers == #view.pads, string.format("expected %d pad controls, drew %d", #view.pads, triggers))

-- Rules carry the four intents the board exists to emit.
local intents = {}
for _, spec in ipairs(drawn.buttons) do
  if type(spec.action) == "string" and spec.body and spec.body.action then
    intents[spec.body.action] = true
  end
end
for _, expected in ipairs({ "sfx.trigger", "sfx.bind", "sfx.test", "sfx.enabled", "sfx.room", "sfx.stop" }) do
  assert(intents[expected], "missing intent: " .. expected)
end

-- Selection is carried in ink, not only in a table field the library ignores.
view.status.selected = "chime"
for _, pad in ipairs(view.pads) do pad.selected = pad.id == "chime" end
local _, selected_signature = render(view)
assert(selected_signature ~= signature, "selection did not change what was painted")
view.status.selected = "pop"
for _, pad in ipairs(view.pads) do pad.selected = pad.id == "pop" end

-- Arming a rule is a visible mode, not a hidden state.
view.status.bind = "email.delta.v1"
local armed_copy = render(view)
assert(armed_copy:find("BINDING email.delta.v1", 1, true), "binding mode is not visible")
assert(armed_copy:find("press a pad", 1, true), "binding mode does not say what to do")
view.status.bind = nil

-- An empty mapping is a normal state, not a blank window.
local empty = {
  title = "SFX", counts = { pads = 0, rules = 0, enabled = 0, live_rooms = 0, issues = 0 },
  mapping = { scope = "memory", gain = 0.85 }, status = { output = "48 kHz stereo" },
  rooms = {}, pads = {}, rules = {}, issues = {},
}
local empty_copy = render(empty)
assert(empty_copy:find("No events are mapped", 1, true), "an empty mapping must say so")
assert(empty_copy:find("No pads", 1, true), "an empty board must say so")
assert(empty_copy:find("No rooms reported", 1, true), "an empty room list must say so")

print(string.format("ui: ok (%d text, %d buttons)", full_text, full_buttons))
