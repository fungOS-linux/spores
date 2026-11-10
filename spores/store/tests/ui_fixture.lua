-- Headless check for ui/store.lua. Run from spores/store:
--   lua tests/ui_fixture.lua
--
-- The view is self-painting: it goes through `ui.present`, which emits Canvas
-- primitives (`canvas.rect`, `canvas.text`) and host-owned controls
-- (`widget.button`). This supplies a recording host and asserts on what was
-- *drawn*, so a change that silently drops a row or a control fails here rather
-- than on the wall.

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

local store = assert(loadfile("ui/store.lua"))()

local function render(view)
  reset()
  store.render(view)
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

-- ---- browse ------------------------------------------------------------

render(nil)
assert(drawn.rects > 25, "expected storefront surfaces, drew " .. drawn.rects)
assert(#drawn.texts > 30, "expected storefront text, drew " .. #drawn.texts)
assert(has_text("Spore Store"), "store title missing")
assert(has_text("All Spores"), "category heading missing")
assert(has_text("markdown") and has_text("terminal") and has_text("gallery"),
  "listing rows missing")
assert(has_text_matching("4.7 · 212"), "rating text missing")
assert(has_text_matching("Updates · 5"), "updates badge missing")
assert(has_text_matching("verified"), "source verification missing")
assert(count_buttons("store.category") == 16,
  "15 category controls plus the Updates button")
assert(count_buttons("store.select") >= 3, "row and card selection controls missing")

-- An empty snapshot still draws the shell rather than erroring: the host may
-- open the store before the first index fetch lands.
render({})
assert(has_text("Spore Store"), "empty snapshot should keep the shell")

-- ---- detail ------------------------------------------------------------

render({ route = "detail", selected_id = "dev.fungos.markdown" })
assert(has_text("markdown"), "detail title missing")
assert(has_text("About"), "description heading missing")
assert(has_text("What it can reach"), "capability heading missing")
assert(has_text_matching("soil:storage — open and save documents by DocId"),
  "grant reason missing")
assert(has_text_matching("verified install"), "review attestation missing")
assert(has_text("Editing a note with the view hot-reloaded"), "screenshot caption missing")

-- The histogram is 5..1 in the snapshot. Reversing it draws the five-star
-- bucket with the one-star count, which is the kind of error a screenshot
-- review misses and a reader never does.
local function five_star_count()
  for _, node in ipairs(drawn.texts) do
    if node.text == "★★★★★" and node.translation[1] > 0 then
      local best
      for _, other in ipairs(drawn.texts) do
        if math.abs(other.translation[2] - node.translation[2]) < 0.02
          and other.translation[1] > node.translation[1]
          and (not best or other.translation[1] < best.translation[1]) then
          best = other
        end
      end
      if best then return best.text end
    end
  end
end
assert(five_star_count() == "148", "histogram buckets are reversed: " .. tostring(five_star_count()))
assert(count_buttons("store.install") == 1, "install control missing")
assert(count_buttons("store.review") == 1, "review control missing")
assert(count_buttons("store.back") == 1, "back control missing")

-- ---- updates -----------------------------------------------------------

render({ route = "updates" })
assert(has_text("Updates"), "updates title missing")
assert(has_text("Waiting on you"), "pending banner missing")
assert(has_text("live") and has_text("held") and has_text("consent"),
  "update classes missing")
assert(has_text_matching("view only · applies now"), "class explanation missing")
assert(count_buttons("store.update") == 4,
  "every class except held offers an apply control")
assert(count_buttons("store.unpin") == 1, "held row should offer unpin")
assert(count_buttons("store.update_all") == 1, "update-all control missing")

print("store ui: ok")
