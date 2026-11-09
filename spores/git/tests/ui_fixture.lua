-- Fixture for ui/review.lua. Run from spores/git:
--   lua tests/ui_fixture.lua
--
-- The view is self-painting: it calls `ui.render`, which emits Canvas
-- primitives through `canvas.*` and `widget.button`. So this fixture supplies a
-- recording host and asserts on what was *drawn*, not on a tree that some other
-- component is supposed to interpret. That is the whole reason the view moved to
-- `sdk/ui`: a tree library can be checked for shape, a self-painting one can be
-- checked for output.
--
-- `fungos.ui` is stubbed through `package.preload` so this runs with no SDK
-- checkout. The stub is not a simulation of the layout: `lay` and the token
-- tables are the package's, loaded from `sdk/ui/ui.lua` in this repo.

local function library()
  local source = assert(io.open("../../sdk/ui/ui.lua", "rb"),
    "../../sdk/ui/ui.lua is missing; the view reads the package this repo declares")
  local chunk = assert(load(source:read("*a"), "sdk/ui/ui.lua"))
  source:close()
  return chunk()
end

package.preload["fungos.ui"] = library

-- ---- a recording host --------------------------------------------------

local drawn = { texts = {}, buttons = {}, rects = 0 }

local function reset()
  drawn = { texts = {}, buttons = {}, rects = 0 }
end

_G.module = {
  inputs = { width = 1200, height = 1600 },
  theme = { title = 34, large = 24, body = 14, small = 10, micro = 8, radius = 0.02 },
}
_G.canvas = {
  rect = function() drawn.rects = drawn.rects + 1 end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
}
_G.widget = { button = function(spec) drawn.buttons[#drawn.buttons + 1] = spec end }

local review = assert(loadfile("ui/review.lua"))()
local view = assert(loadfile("ui/view.example.lua"))()

local function render(snapshot)
  reset()
  review.render(snapshot)
  local copy, signature = {}, {}
  for _, spec in ipairs(drawn.texts) do
    copy[#copy + 1] = tostring(spec.text)
    -- Selection is carried in ink, not in the string, so a copy-only signature
    -- would miss it entirely.
    signature[#signature + 1] = tostring(spec.text) .. ":" .. tostring(spec.ink)
  end
  local labels = {}
  for _, spec in ipairs(drawn.buttons) do labels[#labels + 1] = tostring(spec.label) end
  return table.concat(copy, "\n"), table.concat(labels, "\n"), table.concat(signature, "\n")
end

local copy, labels, signature = render(view)

-- It painted through the host, and every primitive is a string Canvas shapes.
assert(#drawn.texts > 10, "the view painted almost nothing")
assert(drawn.rects > 0, "the view painted no rects")
for _, spec in ipairs(drawn.texts) do
  assert(type(spec.text) == "string", "a text primitive has no string")
  assert(type(spec.ink) == "string", "a text primitive has no ink role")
end

-- Copy comes from the snapshot, not from constants.
assert(copy:find("READY TO LAND", 1, true), "the section label is missing")
assert(copy:find(view.rows[1].title, 1, true), "the first title is not drawn")
assert(copy:find("FuturePresentLabs/marbles#26", 1, true), "the PR coordinate is not drawn")
assert(copy:find("no pull request", 1, true), "a row without a PR must say so")
assert(copy:find("7 waiting", 1, true), "the waiting count is not drawn")

-- The one judgement the queue exists for: merged offers Land, anything else
-- offers Review, and a row with no PR offers neither.
assert(labels:find("Land", 1, true), "the merged row offers no Land button")
assert(labels:find("Review", 1, true), "open rows offer no Review button")

-- Count the decision buttons: every row with a PR offers exactly one.
local with_pr = 0
for _, item in ipairs(view.rows) do if item.pr then with_pr = with_pr + 1 end end
local decisions = 0
for _, spec in ipairs(drawn.buttons) do
  if spec.label == "Land" or spec.label == "Review" then decisions = decisions + 1 end
end
assert(decisions == with_pr, string.format("expected %d decision buttons, drew %d", with_pr, decisions))

-- Intents are emitted, never executed.
local intents = {}
for _, spec in ipairs(drawn.buttons) do
  local action = spec.action
  if type(action) == "table" and action.action then intents[action.action] = true end
end
for _, expected in ipairs({ "git.land", "git.open_marble", "git.filter" }) do
  assert(intents[expected], "missing intent: " .. expected)
end

-- A view is a function of the snapshot: another selection paints differently.
view.selected_id = "motion-guard-noise"
local _, _, after = render(view)
assert(after ~= signature, "selection did not change what was painted")
view.selected_id = "federation-f1-assertion-cache"

-- An empty queue is a normal state, not a blank window.
local empty = render({ title = "REVIEW", counts = {}, rows = {} })
assert(empty:find("Nothing is waiting", 1, true), "an empty queue must say so")

print(string.format("ui: ok (%d text, %d buttons)", #drawn.texts, #drawn.buttons))
