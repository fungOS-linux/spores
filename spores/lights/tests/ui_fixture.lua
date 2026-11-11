-- Headless check for ui/lights.lua. Run from spores/lights:
--   lua tests/ui_fixture.lua

local function library()
  local source = assert(io.open("../../sdk/ui/ui.lua", "rb"),
    "../../sdk/ui/ui.lua is missing; the view reads the package this repo declares")
  local chunk = assert(load(source:read("*a"), "sdk/ui/ui.lua"))
  source:close()
  return chunk()
end

package.preload["fungos.ui"] = library

local drawn = { texts = {}, buttons = {}, rects = 0 }
_G.module = {
  inputs = { width = 1200, height = 820 },
  theme = { title = 26, large = 20, body = 15, small = 12, micro = 10, radius = 10 },
}
_G.canvas = {
  rect = function() drawn.rects = drawn.rects + 1 end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
}
_G.widget = {
  button = function(spec) drawn.buttons[#drawn.buttons + 1] = spec end,
}

local lights = assert(loadfile("ui/lights.lua"))()
local fixture = assert(loadfile("ui/snapshot.lua"))()

local function render(view)
  drawn = { texts = {}, buttons = {}, rects = 0 }
  lights.render(view)
end

local function has_text(value)
  for _, node in ipairs(drawn.texts) do
    if node.text == value then return true end
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

render(fixture)
assert(drawn.rects > 20, "expected room, scene, and light surfaces")
assert(has_text("Living room"), "selected room title missing")
assert(has_text("Window Lamp"), "living-room light missing")
assert(has_text("Relax"), "living-room scene missing")
assert(has_text("60%"), "room brightness readout missing")
assert(has_text("Hue Bridge · 7 lights · 3 on"), "header on-count missing")
assert(count_buttons("lights.room") == 7, "one room row per group")
assert(count_buttons("lights.room_power") == 1, "one room power control")
assert(count_buttons("lights.power") == 4, "four living-room light controls")
assert(count_buttons("lights.brightness") == 12, "three presets per light")
assert(count_buttons("lights.scene") == 3, "three living-room scenes")
assert(count_buttons("lights.all_off") == 1, "global off control missing")

local home = {}
for key, value in pairs(fixture) do home[key] = value end
home.selected = "84"
render(home)
assert(has_text("Buddha"), "home group light missing")
assert(count_buttons("lights.power") == 7, "home group should expose seven lights")
assert(count_buttons("lights.brightness") == 21, "three presets per home light")

render({})
assert(has_text("No lights in this group."), "an empty snapshot must say so")
assert(count_buttons("lights.all_off") == 1, "the global control survives an empty bridge")

print("lights ui: ok")
