-- Fixture for the DragShift ring. Run from spores/dragshift:
--   lua tests/ui_fixture.lua
--
-- The SDK lives at the repository root and is composed into a Spore's UI at
-- build time, so the fixture reads it from there, exactly as tend.lua would.
-- Canvas supplies `module`, `widget` and `canvas`; this file does the same.

local sdk = "../../sdk/ui/ui.lua"
local source = assert(io.open(sdk, "rb"), "missing " .. sdk)
local ui = assert(load(source:read("*a"), sdk))()
source:close()
_G.ui = ui

local drawn = { rects = {}, texts = {}, buttons = {} }

_G.module = {
  inputs = { width = 900, height = 900, source = "photo.jpg", ring = "convert", selected = 3 },
  theme = {
    title = 0.34, large = 0.24, body = 0.145,
    small = 0.12, micro = 0.085, radius = 0.02,
  },
}

_G.canvas = {
  rect = function(spec)
    assert(type(spec.id) == "string", "rect without an id")
    assert(type(spec.scale) == "table" and #spec.scale == 3, "rect without a scale")
    drawn.rects[#drawn.rects + 1] = spec
  end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
}

_G.widget = {
  button = function(spec) drawn.buttons[#drawn.buttons + 1] = spec end,
}

local chunk = assert(loadfile("ui/radial.lua"))
chunk()

local function find_text(value)
  for _, node in ipairs(drawn.texts) do
    if node.text == value then return node end
  end
  return nil
end

local function find_button(id)
  for _, button in ipairs(drawn.buttons) do
    if button.id == id then return button end
  end
  return nil
end

-- The header reads the drop, not a constant.
assert(find_text("photo.jpg"), "the source name is not drawn")
assert(find_text("Convert to"), "the ring name is not drawn")

-- Every slice of the convert ring is a slice, and each one is a real control:
-- the host hit-tests these regions, so a slice without an action is a dead
-- pixel in the middle of a menu.
local expected = { "png", "jpeg", "webp", "tiff", "bmp", "ico", "pdf", "avif", "heic" }
for _, id in ipairs(expected) do
  local button = find_button(id)
  assert(button, "slice " .. id .. " has no hit region")
  assert(button.action == "pick", "slice " .. id .. " has no action")
  assert(type(button.width) == "number" and button.width > 0, "slice " .. id .. " is zero-width")
end
assert(#drawn.buttons == #expected, "extra or missing hit regions")

-- A format this build cannot honour is drawn, but not pressable, and it says so
-- rather than looking like the rest.
assert(find_button("heic") and find_button("heic").enabled == false, "HEIC should be unavailable")
assert(find_button("heic") and find_button("heic").enabled == false, "HEIC should be unavailable")
assert(find_button("png").enabled == true, "PNG should be available")

-- The ring is geometry: slices carry rotation, so a future renderer cannot
-- quietly reduce a ring to a row of boxes.
local rotated = 0
for _, rect in ipairs(drawn.rects) do
  if rect.rotation then
    rotated = rotated + 1
    assert(#rect.rotation == 4, "rotation is not a quaternion")
    assert(math.abs(rect.rotation[1]) < 1e-9 and math.abs(rect.rotation[2]) < 1e-9, "rotation left the surface plane")
  end
end
assert(rotated > 0, "the ring drew no rotated slices")

print("ui: ok")
