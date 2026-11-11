-- Headless check for ui/home.lua. Run from spores/home:
--   lua tests/ui_fixture.lua
--
-- The view is self-painting: it goes through `ui.present`, which emits Canvas
-- primitives (`canvas.rect`, `canvas.text`) and host-owned controls
-- (`widget.button`). This supplies a recording host and asserts on what was
-- drawn, so a change that silently drops a tile or a widget fails here.

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
  inputs = { width = 1440, height = 1600 },
  theme = { title = 30, large = 22, body = 15, small = 12, micro = 10, radius = 10 },
}
_G.canvas = {
  rect = function() drawn.rects = drawn.rects + 1 end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
}
_G.widget = {
  button = function(spec) drawn.buttons[#drawn.buttons + 1] = spec end,
}

local home = assert(loadfile("ui/home.lua"))()

local function render(view)
  reset()
  home.render(view)
end

local function has_text(value)
  for _, node in ipairs(drawn.texts) do
    if node.text == value then return true end
  end
  return false
end

local function has_text_matching(pattern)
  for _, node in ipairs(drawn.texts) do
    if tostring(node.text):find(pattern, 1, true) then return true end
  end
  return false
end

local function count_buttons(action)
  local count = 0
  for _, button in ipairs(drawn.buttons) do
    if button.action == action then count = count + 1 end
  end
  return count
end

-- ---- the launcher ------------------------------------------------------

render(nil)
assert(drawn.rects > 20, "expected launcher surfaces, drew " .. drawn.rects)
assert(#drawn.texts > 40, "expected launcher text, drew " .. #drawn.texts)
assert(has_text("TODAY"), "today widget missing")
assert(has_text("THIS MACHINE"), "machine widget missing")
assert(has_text("STORE"), "store widget missing")
assert(has_text("SYSTEM"), "system widget missing")
assert(has_text("SERVICES"), "services rail missing")
assert(has_text("16 Spores"), "catalog size missing")
assert(has_text("down: webhook, sfx"), "down services missing")
assert(has_text("terminal") and has_text("worklog") and has_text("gallery"),
  "app tiles missing")
assert(has_text("averys-mac-studio"), "host name missing")

-- Every app is openable, and the dock repeats three of them.
assert(count_buttons("home-open") == 19,
  "16 grid tiles plus 3 dock tiles, drew " .. count_buttons("home-open"))

-- ---- partial and empty snapshots --------------------------------------

render({})
assert(has_text("TODAY"), "an empty snapshot should keep the shell")

render({ apps = {}, services = {}, dock = {} })
assert(has_text("SERVICES"), "an app-less snapshot should still draw the rail")
assert(count_buttons("home-open") == 0, "no apps means no open controls")

print("home ui: ok")
