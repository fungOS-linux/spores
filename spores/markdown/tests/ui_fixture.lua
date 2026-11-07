-- Fixture for the Lua UI contract. Run with `lua tests/ui_fixture.lua` from the
-- spore directory.
--
-- Canvas runs the *composed* artifact, not the two source files, so this fixture
-- composes them the way tools/compose.lua does and then supplies the globals a
-- host provides: `module` (inputs and theme), `widget` and `canvas`. There is no
-- JSON bridge: the host materializes the input table, and so does this file.
--
-- The assertions are about the contract, not the pixels: the view must register
-- every control it draws as a hit region with an action, and it must emit a
-- scene. A view that threw away its snapshot and drew a picture would pass a
-- "did it render" check and fail these.

local function read(path)
  local file = assert(io.open(path, "rb"), "missing " .. path)
  local text = file:read("*a")
  file:close()
  return text
end

local library = read("ui/lib/ui.lua")
local view = read("ui/editor.lua")

-- Record what the view asks the host to draw, so the assertions describe the
-- view rather than this stub.
local scene = { buttons = {}, rects = {}, texts = {} }

widget = {
  button = function(spec) scene.buttons[#scene.buttons + 1] = spec end,
}
canvas = {
  rect = function(spec) scene.rects[#scene.rects + 1] = spec end,
  text = function(spec) scene.texts[#scene.texts + 1] = spec end,
}

-- Inputs drive the view. Mode and section are surface state the host hands back
-- on the next render, which is why they are asserted below rather than assumed.
-- The theme is the documented type scale: STYLE in ui/lib/ui.lua maps every style
-- name onto title, large, body, small or micro, and a host that omits one hands
-- the view a nil size.
module = {
  inputs = { width = 1200, height = 900, mode = "edit", section = "Code" },
  theme = { title = 0.34, large = 0.24, body = 0.145, small = 0.12, micro = 0.085, radius = 0.02 },
}

-- Exactly the composition tools/compose.lua performs. If the library stops
-- returning its table this fails here instead of silently in Canvas.
local composed = table.concat({
  "local ui = (function()",
  library,
  "end)()",
  view,
}, "\n")

local chunk = assert(load(composed, "composed"))
chunk()

local function find_button(action)
  for _, button in ipairs(scene.buttons) do
    if button.action == action then return button end
  end
  return nil
end

local function has_text(value)
  for _, node in ipairs(scene.texts) do
    if node.text == value then return true end
  end
  return false
end

assert(#scene.buttons > 0, "the view registered no controls")
assert(#scene.rects > 0, "the view emitted no rects")
assert(#scene.texts > 0, "the view emitted no text")

-- Every control is a real hit region with an action and a body: Canvas hit-tests
-- the region and forwards the action, so a button without one is a dead pixel.
for _, button in ipairs(scene.buttons) do
  assert(type(button.id) == "string" and button.id ~= "", "control has no id")
  assert(type(button.action) == "string", "control " .. button.id .. " has no action")
  assert(type(button.body) == "table", "control " .. button.id .. " has no body")
end

-- Mode is an input, not a constant: editing must be the lit mode.
assert(find_button("mode-edit"), "no edit control")
assert(find_button("mode-read"), "no read control")
assert(find_button("share"), "no share control")
assert(has_text("editing · unsaved"), "edit mode is not reflected in the view")

-- Rail entries are controls keyed by the section they select, and the selected
-- section is drawn as a heading. Both come from module.inputs.
assert(find_button("section-code"), "no rail control for Code")
assert(find_button("section-open-questions"), "rail labels are not slugged")
assert(has_text("Code"), "the selected section is not drawn")

print("ui: ok")
