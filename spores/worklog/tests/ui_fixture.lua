-- Fixture for the worklog view. Run from spores/worklog:
--   FUNGOS_SDK=<fungos-sdk checkout> lua tests/ui_fixture.lua
--
-- Canvas hands a module `canvas.rect` and `canvas.text` and nothing else: no
-- layout service, no text measurement, no hit targets. So the fixture stubs
-- exactly those two primitives and holds the view to the things the host cannot
-- check for it — every node inside the surface, one id per node, one node per
-- line, and a line no wider than the column it was measured for. The defects
-- this guards were all geometric: a wrapped label given one line's height, a
-- summary drawn over the field stacked below it, a row painted past the bottom
-- edge and still counted in "1–40 of 40".

-- The library is the SDK package, loaded the way a host loads it: through
-- `require`. `FUNGOS_SDK` points at a fungos-sdk checkout (`~/src/fungos-sdk` by
-- default), which is where `lua/fungos/ui.lua` lives.
local sdk = os.getenv("FUNGOS_SDK") or ((os.getenv("HOME") or "") .. "/src/fungos-sdk")
local lib_path = sdk .. "/lua/fungos/ui.lua"
local lib_source = assert(io.open(lib_path, "rb"),
  "no fungos.ui at " .. lib_path .. "; set FUNGOS_SDK to a fungos-sdk checkout")
local lib_chunk = assert(load(lib_source:read("*a"), "@" .. lib_path))
lib_source:close()
package.preload["fungos.ui"] = function() return lib_chunk() end

-- The scale the wall publishes: a 1.5x theme in pixels. Canvas sizes text as
-- `scale * theme.body * depth(z)`, so `body` is what it draws with, not a value
-- the view may round to its own defaults.
local THEME = { micro = 15, small = 18, body = 24, large = 27 }

-- The size the host granted this surface. The view cannot ask for it, so the
-- fixture has to hand it over and then hold the view to it.
local view = { width = 1182, height = 1569 }

local drawn = { rects = {}, texts = {} }

-- The library's advance estimate, which is also what `surface:measure_text`
-- returns on a host with no `canvas.measure_text` — and Canvas has none. Using
-- the same number here is the point: a view reserving with a different constant
-- reserves lines the host then paints past.
local function width_of(value, size) return #tostring(value) * size * 0.58 end

-- `surface:ts(size)` is `size / (theme.body * 1.25)`, so this is the inverse.
local function text_size(spec) return spec.scale[1] * THEME.body * 1.25 end

-- Normalized scene units are 8 wide and 8 tall over the whole surface with the
-- origin at the top-left, which is what `nx`/`ny` map into.
local function place(spec)
  return (spec.translation[1] + 4) / 8 * view.width,
    (4 - spec.translation[2]) / 8 * view.height
end

local worklog = assert(loadfile("ui/worklog.lua"))()
local chars_per_line = worklog.chars_per_line

local function install_host()
  _G.module = { viewport = { width = view.width, height = view.height }, theme = THEME }
  _G.widget = {}
  _G.canvas = {
    rect = function(spec) drawn.rects[#drawn.rects + 1] = spec end,
    text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
  }
end

local function render(snapshot)
  drawn = { rects = {}, texts = {} }
  -- A module re-executes for every render, so its pending controls are per-render
  -- too. Leaving them to accumulate counts the same chip once per repaint and
  -- makes the node budget below fail on a scene that is actually inside it.
  for index = #worklog.pending_presses, 1, -1 do worklog.pending_presses[index] = nil end
  for index = #worklog.pending_hits, 1, -1 do worklog.pending_hits[index] = nil end
  assert(worklog.render(snapshot) == nil, "render draws through the host and returns nothing")

  local ids, lines = {}, {}
  for _, spec in ipairs(drawn.texts) do
    lines[#lines + 1] = tostring(spec.text)
    assert(not ids[spec.id], "duplicate node id: " .. tostring(spec.id))
    ids[spec.id] = true
    local x, y = place(spec)
    local size = text_size(spec)
    assert(x >= -0.5 and y >= -0.5, "text starts outside the surface: " .. tostring(spec.id))
    -- The host neither wraps nor clips a `canvas.text`, so a line past the edge is
    -- a line drawn over whatever is beside it. One node is one line here, which
    -- is why the view breaks text itself.
    assert(x + width_of(spec.text, size) <= view.width + 0.5,
      string.format("%s runs past the right edge", tostring(spec.id)))
    assert(y + size * 1.3 <= view.height + 0.5,
      string.format("%s runs past the bottom edge", tostring(spec.id)))
  end
  for _, spec in ipairs(drawn.rects) do
    assert(not ids[spec.id], "duplicate node id: " .. tostring(spec.id))
    ids[spec.id] = true
    local x, y = place(spec)
    local w, h = spec.scale[1] / 8 * view.width, spec.scale[2] / 8 * view.height
    assert(x >= -0.5 and y >= -0.5 and x + w <= view.width + 0.5 and y + h <= view.height + 0.5,
      string.format("%s is not inside the surface (%.1f,%.1f %.1fx%.1f)",
        tostring(spec.id), x, y, w, h))
  end
  -- Canvas rejects a scene over its node budget, and a rejected scene is an error
  -- card where the surface should be. Rects, text and controls all count toward
  -- it. The view plans against a stricter number than this: the installed host's
  -- observed headroom, not this tree's default.
  local total = #drawn.rects + #drawn.texts + #worklog.pending_presses
  assert(total <= 256, string.format("the scene is %d nodes; Canvas rejects anything over 256", total))
  return table.concat(lines, "\n")
end

local function rect_of(id)
  for _, spec in ipairs(drawn.rects) do
    if spec.id == id then
      local x, y = place(spec)
      return { x = x, y = y, w = spec.scale[1] / 8 * view.width, h = spec.scale[2] / 8 * view.height }
    end
  end
end

local function text_at(id)
  for _, spec in ipairs(drawn.texts) do
    if spec.id == id then
      local x, y = place(spec)
      return { x = x, y = y, text = spec.text, size = text_size(spec) }
    end
  end
end

local function ids_drawn()
  local ids = {}
  for _, spec in ipairs(drawn.texts) do ids[tostring(spec.id)] = true end
  for _, spec in ipairs(drawn.rects) do ids[tostring(spec.id)] = true end
  return ids
end

install_host()
local snapshot = assert(loadfile("examples/worklog_mock.lua"))()
local copy = render(snapshot)

assert(#drawn.texts > 0, "the view drew no text")
assert(#drawn.rects > 0, "the view drew no rects")
for _, spec in ipairs(drawn.texts) do
  assert(type(spec.text) == "string", "text primitive without a string")
  assert(type(spec.ink) == "string", "text primitive without an ink role")
end

-- The view draws the size it was granted, not the size it would have preferred:
-- the backdrop covers the viewport and the two panes sit side by side inside it.
local backdrop = assert(rect_of("worklog-bg"), "the view draws no backdrop")
assert(math.abs(backdrop.w - view.width) < 0.5 and math.abs(backdrop.h - view.height) < 0.5,
  string.format("the backdrop is %.0fx%.0f on a %dx%d surface", backdrop.w, backdrop.h, view.width, view.height))
local list_pane = assert(rect_of("worklog-list-bg"), "the view draws no list pane")
local detail_pane = assert(rect_of("worklog-detail-bg"), "the view draws no detail pane")
assert(detail_pane.x >= list_pane.x + list_pane.w,
  string.format("the detail column starts inside the list column (list ends %.1f, detail starts %.1f)",
    list_pane.x + list_pane.w, detail_pane.x))

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
  assert(spec.action == "worklog-open-locator", "a press without the declared action id")
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

-- A summary long enough to wrap: Marbles' detail runs to hundreds of characters,
-- and a column that reserves one line's height lays the rest of itself over the
-- field stacked below.
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

local summary_lines, last_line = 0, nil
for _, spec in ipairs(drawn.texts) do
  local index = tostring(spec.id):match("^worklog%-detail%-summary%-l(%d+)$")
  if index then
    summary_lines = math.max(summary_lines, tonumber(index))
    if tonumber(index) == summary_lines then last_line = spec end
  end
end
assert(summary_lines >= 2, "the long summary was not wrapped into lines")
assert(summary_lines <= worklog.summary_lines,
  "the detail summary drew more lines than the column reserved: " .. summary_lines)

-- The invariant that actually matters: the field after the summary starts below
-- the last line of it. A column that reserves two lines for an eight-line
-- summary leaves a hole; one that reserves eight for a two-line summary draws
-- the next field over the text.
local _, summary_top = place(last_line)
local summary_end = summary_top + text_size(last_line) * 1.3
local when = assert(text_at("worklog-detail-when"), "the detail column has no timestamp line")
assert(when.y >= summary_end - 0.5,
  string.format("the detail column overlaps its own summary (summary ends %.1f, when starts %.1f)",
    summary_end, when.y))
-- And every detail label stays inside the pane it was measured for.
for _, spec in ipairs(drawn.texts) do
  if tostring(spec.id):find("^worklog%-detail") and not tostring(spec.id):find("%-bg$") then
    local x_px = place(spec)
    assert(x_px + width_of(spec.text, text_size(spec)) <= detail_pane.x + detail_pane.w + 0.5,
      tostring(spec.id) .. " runs out of the detail pane")
    assert(x_px >= detail_pane.x - 0.5, tostring(spec.id) .. " starts left of the detail pane")
  end
end

-- Scrolling: the host owns the offset, the view windows the log to it. A wheel
-- notch is pixels, and a view that ignored them would render the same first row
-- no matter how far the reader scrolled.
local many = { title = "WORKLOG", counts = { total = 30, today = 3 },
  sources = { { id = "marbles", label = "Marbles", count = 30 } }, events = {} }
for index = 1, 30 do
  local template = snapshot.events[(index - 1) % #snapshot.events + 1]
  many.events[index] = {
    id = "e" .. index, kind = template.kind, source = template.source,
    actor = template.actor, summary = "event " .. index .. ": " .. template.summary,
    subject = template.subject, age = template.age, at_text = template.at_text,
    selected = index == 1, links = template.links,
  }
end

-- The pitch is derived per page, so it has to be read from a render of *this*
-- snapshot: using the previous page's pitch would scroll to the wrong row.
_G.module.scroll = nil
local pitched = render(many)
assert(pitched:find("event 1:", 1, true), "the first page does not start at the newest event")
local pitch = worklog.row_height
_G.module.scroll = { offset = pitch * 3 }
render(many)
local scrolled = ids_drawn()
assert(scrolled["event-4-summary"], "a scrolled window does not start at row 4")
assert(not scrolled["event-1-summary"], "the first row is still drawn after scrolling")
_G.module.scroll = { offset = pitch * 1000 }
render(many)
assert(ids_drawn()["event-30-summary"], "a scroll past the end does not clamp to the last page")
_G.module.scroll = nil

-- How much log fits is as much a part of the view as whether it fits. The node
-- budget used to be estimated rather than counted, and the estimate put 17 rows
-- on a page that could hold twice that. The pitch is now derived from counted
-- costs, so a row is a row — and if a page ever has to drop a row to stay inside
-- the host's ceiling, this says so instead of hiding it in the range label.
render(many)
assert(worklog.rows_drawn == worklog.rows_planned,
  string.format("rows dropped at draw time on a page that had room: %d of %d",
    worklog.rows_drawn, worklog.rows_planned))
assert(worklog.row_height <= worklog.max_row_height,
  string.format("the pitch is %d px, past the view's own cap of %d",
    worklog.row_height, worklog.max_row_height))
assert(worklog.rows_drawn >= 16,
  string.format("a 1,569 px surface fits only %d rows", worklog.rows_drawn))

-- Hover: the id the host hit is the row under the pointer, so it is the row the
-- view lights — no geometry guess of its own.
_G.module.hover = "event-2"
render(snapshot)
assert(ids_drawn()["event-hover-2"], "the hovered row is not lit")
_G.module.hover = nil

-- A short surface: the view cannot ask for a size, so it has to survive one.
-- Rows and the detail column are both computed from the height it was handed.
view = { width = 1182, height = 797 }
install_host()
local short = render(snapshot)
assert(short:find("WORKLOG", 1, true), "the title is missing at surface size")
local short_backdrop = assert(rect_of("worklog-bg"), "no backdrop at surface size")
assert(math.abs(short_backdrop.h - 797) < 0.5,
  string.format("the view laid out for %.0f px when it was handed 797", short_backdrop.h))
view = { width = 1182, height = 1569 }
install_host()

print("ui: ok")

-- Canvas provides no `widget.hit`: its `widget` is the theming library and Soil
-- exposes no hit targets. Rendering there must not crash, and the spec must not
-- vanish.
for index = #worklog.pending_hits, 1, -1 do worklog.pending_hits[index] = nil end
drawn = { rects = {}, texts = {} }
local rendered, render_error = pcall(worklog.render, snapshot)
assert(rendered, "render crashed without widget.hit: " .. tostring(render_error))
assert(#drawn.texts > 0, "the no-hit host drew nothing")
assert(#worklog.pending_hits == worklog.rows_drawn, "every drawn row should stay addressable")

local delivered = nil
_G.widget = { hit = function(spec) delivered = spec end }
worklog.hit({ id = "probe", label = "hit", x = 0, y = 0, width = 1, height = 1 })
assert(delivered and delivered.id == "probe", "hit was not delivered to a host that provides it")
_G.widget = {}
print("ui: no-hit host ok")
