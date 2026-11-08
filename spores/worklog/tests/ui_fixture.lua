-- Fixture for the worklog view. Run from spores/worklog:
--   lua tests/ui_fixture.lua
--
-- The view builds a node tree and hands it to `canvas.layout`, which in Canvas
-- is the Taffy layout server. A fixture has no layout engine, so this supplies a
-- minimal host that flattens the tree and gives every node a placeholder rect.
-- The assertions are about structure and copy, never about geometry.

-- The layout kit lives in the inbox Spore: one kit, composed into every view
-- rather than copied per Spore. Only `ui.layout` is used here.
local source = assert(io.open("../unibus/ui/lib/ui.lua", "rb"),
  "../unibus/ui/lib/ui.lua is missing; the worklog view composes the shared kit")
local ui = assert(load(source:read("*a"), "ui/lib/ui.lua"))()
source:close()
_G.ui = ui

_G.module = {
  viewport = { width = 1086, height = 1569 },
  theme = {
    title = 0.34, large = 0.24, body = 0.145,
    small = 0.105, micro = 0.085, radius = 0.02,
  },
}

local drawn = { rects = {}, texts = {} }

_G.canvas = {
  -- Flatten the node graph the way the layout server does, keeping every field
  -- the draw pass reads and replacing only the geometry.
  layout = function(root, width)
    local nodes = {}
    local function walk(node)
      local copy = {}
      for key, value in pairs(node) do
        if key ~= "children" then copy[key] = value end
      end
      copy.rect = { x = 0, y = 0, width = width, height = 24 }
      nodes[#nodes + 1] = copy
      for _, child in ipairs(node.children or {}) do walk(child) end
    end
    walk(root)
    return { nodes = nodes }
  end,
  rect = function(spec) drawn.rects[#drawn.rects + 1] = spec end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
}

local worklog = assert(loadfile("ui/worklog.lua"))()
local snapshot = assert(loadfile("examples/worklog_mock.lua"))()

local function render(snap)
  drawn = { rects = {}, texts = {} }
  assert(worklog.render(snap) == nil, "render draws through the host and returns nothing")
  local lines = {}
  for _, spec in ipairs(drawn.texts) do lines[#lines + 1] = tostring(spec.text) end
  return table.concat(lines, "\n")
end

local copy = render(snapshot)

assert(#drawn.texts > 0, "the view drew no text")
assert(#drawn.rects > 0, "the view drew no rects")
for _, spec in ipairs(drawn.texts) do
  assert(type(spec.text) == "string", "text primitive without a string")
  assert(type(spec.ink) == "string", "text primitive without an ink role")
end

-- The timeline reads from the snapshot, not from constants.
assert(copy:find("WORKLOG", 1, true), "title is missing")
assert(copy:find("Newest first", 1, true), "sort order is missing")
assert(copy:find(string.format("%d today", snapshot.counts.today), 1, true), "today count is not drawn")
assert(copy:find("Marbles 3", 1, true), "source counts are not drawn")

-- Every event is a row, and the kind leads it in a fixed, upper-case word.
for index, event in ipairs(snapshot.events) do
  assert(copy:find(event.summary, 1, true), "event " .. index .. " summary is not drawn")
end
assert(copy:find("MERGED", 1, true), "the kind is not drawn as a word")

-- Newest first is the producer's decision and the view must not reorder it: the
-- first event's row ids come before the second's.
local first_row, second_row = nil, nil
for index, spec in ipairs(drawn.texts) do
  if spec.id == "event-1-summary" then first_row = index end
  if spec.id == "event-2-summary" then second_row = index end
end
assert(first_row and second_row and first_row < second_row, "rows are not painted in snapshot order")

-- The selected event drives the detail pane.
local selected = snapshot.events[1]
assert(copy:find(selected.subject, 1, true), "the selected event's locator is not drawn")

-- A view is a function of the snapshot: selecting another event has to change
-- what is drawn, or the detail pane is decoration.
for _, event in ipairs(snapshot.events) do event.selected = event == snapshot.events[3] end
local other_copy = render(snapshot)
assert(other_copy ~= copy, "selection did not change the view")
assert(other_copy:find(snapshot.events[3].summary, 1, true), "newly selected summary is not drawn")
for index, event in ipairs(snapshot.events) do event.selected = index == 1 end

-- An empty log is a normal state, not a crash.
local empty = render({ title = "WORKLOG", counts = { total = 0, today = 0 }, events = {} })
assert(empty:find("Nothing has arrived yet", 1, true), "an empty log should say so")

print("ui: ok")

-- Canvas provides no `widget.hit`: its `widget` is the theming library and Soil
-- exposes no hit targets. Rendering there must not crash, and the spec must not
-- vanish.
local themed_widget = _G.widget
_G.widget = {}
for index = #worklog.pending_hits, 1, -1 do worklog.pending_hits[index] = nil end

drawn = { rects = {}, texts = {} }
local rendered, render_error = pcall(worklog.render, snapshot)
assert(rendered, "render crashed without widget.hit: " .. tostring(render_error))
assert(#drawn.texts > 0, "the no-hit host drew nothing")
assert(#worklog.pending_hits == #snapshot.events, "every row should stay addressable")

local delivered = nil
_G.widget = { hit = function(spec) delivered = spec end }
worklog.hit({ id = "probe", label = "hit", x = 0, y = 0, width = 1, height = 1 })
assert(delivered and delivered.id == "probe", "hit was not delivered to a host that provides it")

_G.widget = themed_widget
print("ui: no-hit host ok")

-- Canvas has no layout service either, so the view must run without
-- `canvas.layout` and draw the same copy through the Lua fallback.
local real_layout = _G.canvas.layout
_G.canvas.layout = nil
drawn = { rects = {}, texts = {} }

local laid_ok, laid_error = pcall(worklog.render, snapshot)
_G.canvas.layout = real_layout

assert(laid_ok, "render failed without canvas.layout: " .. tostring(laid_error))
assert(#drawn.texts > 0, "the layout fallback drew nothing")
assert(#drawn.rects > 0, "the layout fallback drew no rects")

local laid = {}
for _, spec in ipairs(drawn.texts) do laid[#laid + 1] = tostring(spec.text) end
local laid_text = table.concat(laid, "\n")
assert(laid_text:find("WORKLOG", 1, true), "the layout fallback lost the title")
assert(laid_text:find(snapshot.events[1].summary, 1, true), "the layout fallback lost the newest event")

print("ui: layout fallback ok")
