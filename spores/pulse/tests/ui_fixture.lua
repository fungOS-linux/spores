-- Fixture for ui/console.lua. Run from spores/pulse:
--   FUNGOS_SDK=<fungos-sdk checkout> lua tests/ui_fixture.lua
--
-- The view is a function of the snapshot: it must paint every row it was given,
-- keep every node inside the surface, and make every control carry an intent.
-- The defects this guards are the ones a browser would hide until a reader
-- needed the page: an empty editor, an intent-less button, a row drawn past the
-- pane that contains it.
local sdk = os.getenv("FUNGOS_SDK") or ((os.getenv("HOME") or "") .. "/src/fungos-sdk")
local lib_path = sdk .. "/lua/fungos/ui.lua"
local lib_source = assert(io.open(lib_path, "rb"),
  "no fungos.ui at " .. lib_path .. "; set FUNGOS_SDK to a fungos-sdk checkout")
local lib_chunk = assert(load(lib_source:read("*a"), "@" .. lib_path))
lib_source:close()
package.preload["fungos.ui"] = function() return lib_chunk() end

local THEME = { micro = 15, small = 18, body = 24, large = 27 }
local view_size = { width = 1182, height = 1569 }
local drawn = { rects = {}, texts = {} }
local presses = {}

local function install_host()
  _G.module = { viewport = { width = view_size.width, height = view_size.height }, theme = THEME }
  _G.widget = { button = function(spec) presses[#presses + 1] = spec end }
  _G.canvas = {
    rect = function(spec) drawn.rects[#drawn.rects + 1] = spec end,
    text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
  }
end

local view = assert(loadfile("ui/console.lua"))()
local fixture = assert(loadfile("ui/snapshot.example.lua"))()

local function copy(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for key, item in pairs(value) do out[key] = copy(item) end
  return out
end

local function place(spec)
  return (spec.translation[1] + 4) / 8 * view_size.width,
    (4 - spec.translation[2]) / 8 * view_size.height
end

local function text_size(spec)
  return spec.scale[1] * THEME.body * 1.25
end

local function width_of(value, size)
  return #tostring(value) * size * 0.58
end

local function render(snapshot)
  drawn = { rects = {}, texts = {} }
  presses = {}
  install_host()
  assert(view.render(snapshot) == nil, "render paints through the host and returns nothing")
  local ids, problems = {}, {}
  for _, spec in ipairs(drawn.texts) do
    if ids[spec.id] then problems[#problems + 1] = "duplicate node id: " .. tostring(spec.id) end
    ids[spec.id] = true
    local x, y = place(spec)
    local size = text_size(spec)
    if x < -0.5 or y < -0.5 then
      problems[#problems + 1] = string.format("%s starts outside the surface", tostring(spec.id))
    end
    if x + width_of(spec.text, size) > view_size.width + 1 then
      problems[#problems + 1] = string.format("%s runs past the right edge", tostring(spec.id))
    end
    if y + size * 1.3 > view_size.height + 1 then
      problems[#problems + 1] = string.format("%s runs past the bottom edge", tostring(spec.id))
    end
  end
  for _, spec in ipairs(drawn.rects) do
    if ids[spec.id] then problems[#problems + 1] = "duplicate node id: " .. tostring(spec.id) end
    ids[spec.id] = true
    local x, y = place(spec)
    local w, h = spec.scale[1] / 8 * view_size.width, spec.scale[2] / 8 * view_size.height
    if x < -1 or y < -1 or x + w > view_size.width + 2 or y + h > view_size.height + 2 then
      problems[#problems + 1] = string.format("%s is not inside the surface", tostring(spec.id))
    end
  end
  assert(#problems == 0, table.concat(problems, "\n"))
  local total = #drawn.rects + #drawn.texts + #presses
  assert(total <= 256, string.format("the scene is %d nodes; Canvas rejects over 256", total))
  return total
end

local function text(id)
  for _, spec in ipairs(drawn.texts) do
    if spec.id == id then return tostring(spec.text) end
  end
  return nil
end

local function texts_containing(fragment)
  local found = {}
  for _, spec in ipairs(drawn.texts) do
    if tostring(spec.text):find(fragment, 1, true) then found[#found + 1] = spec.text end
  end
  return found
end

local function press_with(id)
  for _, spec in ipairs(presses) do
    if spec.id == id then return spec end
  end
  return nil
end

-- The default render: every cron is a row, the editor is the selected cron, and
-- the four sections a reader checks are all present.
local total = render(fixture)
assert(#drawn.texts > 0, "the view drew no text")
assert(#drawn.rects > 0, "the view drew no rects")
assert(text("pulse-title") == "PULSE", "the masthead is missing")
assert(text("pulse-cron-facts"):find("5 crons", 1, true), "the cron summary is missing")
assert(text("pulse-hook-facts"):find("4 hooks", 1, true), "the hook summary is missing")
for index, cron in ipairs(fixture.crons) do
  assert(text("pulse-cron-" .. index .. "-name") == cron.name, "cron row " .. index .. " is missing")
end
assert(text("pulse-detail-name") == "Nightly backup", "the selected cron is not in the editor")
assert(text("pulse-detail-schedule-value") == "0 3 * * *", "the schedule is not drawn")
assert(text("pulse-detail-description") == "every day at 03:00", "the description is not drawn")
assert(text("pulse-detail-next"):find("2026-01-03T03:00:00Z", 1, true), "the next fire is not drawn")
assert(text("pulse-detail-target-value"):find("dev.fungos.worklog#compact", 1, true),
  "the target is not drawn")
assert(#texts_containing("RECENT RUNS") == 1, "the run section is missing")
print(string.format("ui: rendered %d nodes", total))

-- Every control is an intent, and the vocabulary is closed. A button with no
-- action is a control the host cannot route; a kind outside this set is a
-- typo the reducer would reject at press time instead of at build time.
local ALLOWED = {
  tab = true, select = true, ["select-hook"] = true, preset = true, step = true,
  weekday = true, target = true, toggle = true, delete = true, schedule = true, save = true,
}
local press_ids = {}
for _, press in ipairs(presses) do
  assert(not press_ids[press.id], "duplicate press id: " .. tostring(press.id))
  press_ids[press.id] = true
  assert(type(press.action) == "table", "press without an intent: " .. tostring(press.id))
  assert(type(press.action.kind) == "string", "press without a kind: " .. tostring(press.id))
  assert(ALLOWED[press.action.kind], "unknown intent kind: " .. tostring(press.action.kind))
end
for _, id in ipairs({
  "pulse-tabs-crons", "pulse-tabs-ingress", "pulse-cron-3",
  "pulse-presets-daily", "pulse-weekdays-1", "pulse-targets-1",
  "pulse-actions-toggle", "pulse-actions-delete",
}) do
  assert(press_with(id), "missing control: " .. id)
end
assert(press_with("pulse-actions-toggle").action.id == "nightly-backup", "toggle targets the wrong cron")
assert(press_with("pulse-presets-daily").action.preset == "daily", "preset carries the wrong id")
print("ui: controls carry intents")

-- Selecting another cron changes the editor; this is what makes the list a list
-- rather than decoration.
local selected = copy(fixture)
selected.selected = "rake"
render(selected)
assert(text("pulse-detail-name") == "Search rake", "selection did not change the editor")
assert(text("pulse-detail-health") == "OVERDUE", "overdue is not called out")
assert(text("pulse-detail-schedule-value") == "0 * * * *", "the new schedule is not drawn")
print("ui: selection changes the editor")

-- The ingress pane is the other half of the console, and it has to say why a
-- delivery was refused, not only that one was.
local ingress = copy(fixture)
ingress.tab = "ingress"
ingress.selected_hook = "ci-edge"
render(ingress)
assert(text("pulse-hooks-label") == "WEBHOOKS  4", "the hook list is missing")
assert(text("pulse-ingress-name") == "standard-webhooks ci", "the hook is not in the editor")
assert(text("pulse-ingress-health") == "FAILING", "hook health is not called out")
assert(text("pulse-ingress-credential") == "credential ci-edge", "the credential name is not drawn")
assert(text("pulse-reasons-1-text") == "401: signature mismatch", "the rejection reason is not drawn")
assert(text("pulse-reasons-1-count") == "3×", "the rejection count is not drawn")
assert(#texts_containing("rejected") > 0, "deliveries do not show a rejection")
print("ui: ingress pane shows health and reasons")

-- An invalid schedule is a normal state after a hand-edited record: the editor
-- must still open, say what is wrong, and keep the presets reachable.
local invalid = copy(fixture)
invalid.crons[1].schedule = "nope"
invalid.crons[1].health = "invalid"
invalid.crons[1].health_text = "invalid schedule"
invalid.crons[1].problems = { { field = "schedule", text = "a schedule is five fields" } }
invalid.selected = invalid.crons[1].id
render(invalid)
assert(text("pulse-detail-health") == "INVALID SCHEDULE", "invalid health is not drawn")
assert(text("pulse-problem-1") == "schedule: a schedule is five fields", "the problem is not drawn")
assert(press_with("pulse-presets-hourly"), "presets must stay reachable on an invalid schedule")
print("ui: invalid schedules explain themselves")

-- Empty is a state, not a crash. Both panes have words for it.
render({ title = "PULSE", tab = "crons", summary = {}, crons = {}, hooks = {} })
assert(text("pulse-crons-empty-title") == "No crons", "the empty cron list says nothing")
assert(text("pulse-detail-empty-title") == "No cron selected", "the empty editor says nothing")
render({ title = "PULSE", tab = "ingress", summary = {}, crons = {}, hooks = {} })
assert(text("pulse-hooks-empty-title") == "No hooks", "the empty hook list says nothing")
assert(text("pulse-ingress-empty-title") == "No webhook selected", "the empty ingress editor says nothing")
print("ui: empty panes say so")

-- A short window still has to lay out: the editor is a column of fixed rows, so
-- the only thing that may change is how much room the list and detail get.
view_size = { width = 900, height = 700 }
render(fixture)
assert(text("pulse-title") == "PULSE", "the masthead is missing at 900x700")
print("ui: ok")
