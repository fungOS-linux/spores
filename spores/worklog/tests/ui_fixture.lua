-- Fixture for the worklog view. Run from spores/worklog:
--   lua tests/ui_fixture.lua
--
-- The view builds a node tree and hands geometry to the host. Canvas has no
-- layout service, so it draws through the kit's Lua flow layout — which is what
-- this fixture runs, at the surface sizes the wall actually hands out. Geometry
-- is asserted, not stubbed: the defect this guards was geometric (a wrapped
-- label given one line's height while the host drew several, so the detail
-- column landed on top of itself), and a placeholder rect per node hides it.

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
local checked = { width = 0, height = 0 }
-- What each wrapped label was allowed to draw: the emit path must stay inside
-- it, which is the invariant a wall depends on. A box sized for one line while
-- the ink ran to eight is what put the detail column on top of itself.
local budget = {}
-- The view publishes the wrap budget; the fixture asks it rather than keeping a
-- second guess at how many characters fit.
local chars_per_line

local function check_geometry(laid, width, height)
  checked.width, checked.height = width, height
  local by_id, nodes = {}, {}
  for _, node in ipairs(laid.nodes or {}) do
    local rect = node.rect
    assert(rect, "laid-out node without a rect: " .. tostring(node.id))
    assert(rect.x >= -0.5 and rect.y >= -0.5,
      string.format("%s starts outside the surface (%.1f, %.1f)", tostring(node.id), rect.x, rect.y))
    assert(rect.x + rect.width <= width + 0.5,
      string.format("%s runs past the right edge (%.1f > %d)", tostring(node.id), rect.x + rect.width, width))
    assert(rect.y + rect.height <= height + 0.5,
      string.format("%s runs past the bottom edge (%.1f > %d)", tostring(node.id), rect.y + rect.height, height))
    by_id[node.id] = rect
    nodes[node.id] = node
  end
  -- Every wrapped label has to reserve the height its lines take. The host draws
  -- the lines and the layout only honours the box, so a box of one line's height
  -- does not clip the ink: it lets it run over whatever the column stacks next —
  -- which is what put the detail column on top of its own summary.
  for id, node in pairs(nodes) do
    local rect, style = by_id[id], node.style
    if rect and style and style.wrap and node.kind == "text" then
      local line_height = style.line_height or (style.size + 4)
      budget[id] = {
        per_line = chars_per_line and chars_per_line(rect.width, style.size) or 1,
        lines = math.max(1, math.floor(rect.height / line_height)),
      }
    end
  end
  -- And the column stacks: the field after the summary may not sit under it.
  local summary, below = by_id["worklog-detail-summary"], by_id["worklog-detail-when"]
  if summary and below then
    assert(below.y >= summary.y + summary.height - 0.5,
      string.format("the detail column overlaps its own summary (summary ends %.1f, worklog-detail-when starts %.1f)",
        summary.y + summary.height, below.y))
  end
  -- The two columns sit side by side, not on top of each other: a flexible list
  -- pane used to take the whole row and lay its text across the detail column.
  local list_pane, detail_pane = by_id["worklog-list-pane"], by_id["worklog-detail-pane"]
  if list_pane and detail_pane then
    assert(detail_pane.x >= list_pane.x + list_pane.width,
      string.format("the detail column starts inside the list column (list ends %.1f, detail starts %.1f)",
        list_pane.x + list_pane.width, detail_pane.x))
  end
end

_G.canvas = {
  -- The kit's own flow layout: the same code Canvas falls back to when the host
  -- offers no `canvas.layout`.
  layout = function(root, width, height)
    local laid = ui.layout(root, width, height)
    check_geometry(laid, width, height)
    return laid
  end,
  rect = function(spec) drawn.rects[#drawn.rects + 1] = spec end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
}

local worklog = assert(loadfile("ui/worklog.lua"))()
chars_per_line = worklog.chars_per_line
local snapshot = assert(loadfile("examples/worklog_mock.lua"))()

local function render(snap)
  drawn = { rects = {}, texts = {} }
  assert(worklog.render(snap) == nil, "render draws through the host and returns nothing")
  local lines, ids = {}, {}
  for _, spec in ipairs(drawn.texts) do
    lines[#lines + 1] = tostring(spec.text)
    local x, y = spec.translation[1], spec.translation[2]
    assert(x >= -4.5 and x <= 4.5 and y >= -4.5 and y <= 4.5,
      "text drawn outside the surface: " .. tostring(spec.id))
    assert(not ids[spec.id], "duplicate text id: " .. tostring(spec.id))
    ids[spec.id] = true
  end
  for _, spec in ipairs(drawn.rects) do
    assert(not ids[spec.id], "duplicate node id: " .. tostring(spec.id))
    ids[spec.id] = true
  end
  -- The host rejects a scene over 256 nodes, and a rejected scene is an error
  -- card where the surface should be. Width x height is not the only bound.
  local total_nodes = #drawn.rects + #drawn.texts + #worklog.pending_presses
  assert(total_nodes <= 256,
    string.format("the scene is %d nodes; Canvas rejects anything over 256", total_nodes))
  -- Every wrapped line has to fit the box it was given: a line past the budget
  -- is a line drawn over whatever the column stacked next.
  for _, spec in ipairs(drawn.texts) do
    local base, index = tostring(spec.id):match("^(.-)%-l(%d+)$")
    local allowed = base and budget[base]
    if allowed then
      assert(tonumber(index) <= allowed.lines,
        string.format("%s drew line %s of %d", base, index, allowed.lines))
      assert(#tostring(spec.text) <= allowed.per_line,
        string.format("%s line %s is %d chars, budget %d", base, index, #tostring(spec.text), allowed.per_line))
    end
  end
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
-- Wrapped labels are split into lines, so compare the opening words rather than
-- the whole string: the row summary is allowed to break.
for index, event in ipairs(snapshot.events) do
  assert(copy:find(event.summary:sub(1, 20), 1, true), "event " .. index .. " summary is not drawn")
end
assert(copy:find("MERGED", 1, true), "the kind is not drawn as a word")

-- Newest first is the producer's decision and the view must not reorder it: the
-- first event's row ids come before the second's.
local first_row, second_row = nil, nil
for index, spec in ipairs(drawn.texts) do
  if tostring(spec.id):match("^event%-1%-summary") then first_row = first_row or index end
  if tostring(spec.id):match("^event%-2%-summary") then second_row = second_row or index end
end
assert(first_row and second_row and first_row < second_row, "rows are not painted in snapshot order")

-- The selected event drives the detail pane, and its locator is a pressable
-- chip: the label is clipped to the row, but the *intent* carries the whole
-- locator, so that is what a test can hold the view to.
local selected = snapshot.events[1]
local pressable = false
for _, spec in ipairs(worklog.pending_presses or {}) do
  if spec.body and spec.body.locator == selected.subject then pressable = true end
end
assert(pressable, "the selected event's locator is not pressable")

-- A view is a function of the snapshot: selecting another event has to change
-- what is drawn, or the detail pane is decoration.
for _, event in ipairs(snapshot.events) do event.selected = event == snapshot.events[3] end
local other_copy = render(snapshot)
assert(other_copy ~= copy, "selection did not change the view")
assert(other_copy:find(snapshot.events[3].summary:sub(1, 24), 1, true), "newly selected summary is not drawn")
for index, event in ipairs(snapshot.events) do event.selected = index == 1 end

-- An empty log is a normal state, not a crash.
local empty = render({ title = "WORKLOG", counts = { total = 0, today = 0 }, events = {} })
assert(empty:find("Nothing has arrived yet", 1, true), "an empty log should say so")

-- A summary long enough to wrap: Marbles' detail runs to hundreds of
-- characters, and a column that reserves one line's height for it lays the rest
-- of itself under the paragraph.
local long_summary = string.rep("ack:superseded by scan recognition ", 11) .. "end"
local long = {
  id = "long", kind = "closed", source = "marbles", actor = "ajmwagar",
  summary = long_summary, subject = "marbles://local/long",
  at_text = "2026-11-09T00:57:01Z", age = "37m", selected = true,
  links = { { rel = "subject", locator = "marbles://local/long" } },
}
local wrapped_copy = render({
  title = "WORKLOG", counts = { total = 1, today = 1 },
  sources = { { id = "marbles", label = "Marbles", count = 1 } },
  events = { long },
})
assert(wrapped_copy:find("superseded", 1, true), "the long summary is not drawn")

-- A wrapped label is one node per line, so the host cannot draw a line the
-- layout never reserved. The pane is 268 px wide at 16 px type: 32 characters.
local summary_lines = 0
for _, spec in ipairs(drawn.texts) do
  local index = tostring(spec.id):match("^worklog%-detail%-summary%-l(%d+)$")
  if index then
    summary_lines = math.max(summary_lines, tonumber(index))
    assert(#tostring(spec.text) <= chars_per_line(268, 16),
      "a detail summary line is wider than its pane: " .. tostring(spec.text))
  end
end
assert(summary_lines >= 2, "the long summary was not wrapped into lines")
assert(summary_lines <= 8, "the detail summary drew more lines than the column reserved: " .. summary_lines)

-- The size the wall actually hands this surface. The view cannot ask for it, so
-- it has to survive it: rows and the detail column are both computed from H.
_G.module.viewport = { width = 1182, height = 797 }
local at_surface = render(snapshot)
assert(checked.width == 1182 and checked.height == 797, "the view laid out for a size it was not given")
assert(at_surface:find("WORKLOG", 1, true), "the title is missing at surface size")
_G.module.viewport = { width = 1086, height = 1569 }

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
