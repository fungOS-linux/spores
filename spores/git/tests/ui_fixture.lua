-- Fixture for ui/review.lua. Run from spores/git:
--   lua tests/ui_fixture.lua
--
-- The view needs `fungos.ui` and `fungos.action`, which live in the SDK rather
-- than in this repo. Both are pure tree builders, so the stubs below reproduce
-- their contract — same names, same shapes — and the assertions are about the
-- tree the view builds, not about the SDK. A view that calls a function this
-- stub does not have fails here, which is the point: the names are the API.

local function node(kind, id, children, extra)
  local out = { kind = kind, id = id, children = children or {} }
  for key, value in pairs(extra or {}) do out[key] = value end
  return out
end

local stub_ui = {}
function stub_ui.text(id, value, opts) return node("text", id, {}, { text = value, opts = opts or {} }) end
function stub_ui.row(id, layout, children) return node("row", id, children) end
function stub_ui.column(id, layout, children) return node("column", id, children) end
function stub_ui.badge(id, value, opts) return node("badge", id, { stub_ui.text(id .. "-text", value) }) end
function stub_ui.button(id, label, opts) return node("button", id, { stub_ui.text(id .. "-label", label) }, { action = opts and opts.action }) end
function stub_ui.list_row(id, opts, children) return node("list_row", id, children, { action = opts and opts.action, selected = opts and opts.selected }) end
function stub_ui.sidebar(id, opts, children) return node("sidebar", id, children) end
function stub_ui.sidebar_group(id, label, items) return node("sidebar_group", id, items) end
function stub_ui.command_bar(id, opts) return node("command_bar", id) end
function stub_ui.section_label(id, value) return stub_ui.text(id, value) end
function stub_ui.empty_state(id, title, detail)
  return node("empty_state", id, { stub_ui.text(id .. "-title", title), stub_ui.text(id .. "-detail", detail) })
end
function stub_ui.push(children, child) children[#children + 1] = child end

local stub_action = {}
function stub_action.intent(id, payload) return { id = id, payload = payload } end

package.preload["fungos.ui"] = function() return stub_ui end
package.preload["fungos.action"] = function() return stub_action end

local review = assert(loadfile("ui/review.lua"))()
local view = assert(loadfile("ui/view.example.lua"))()

local function walk(root)
  local ids, texts, actions = {}, {}, {}
  local function visit(current)
    if type(current) ~= "table" then return end
    if current.id then ids[current.id] = current end
    if current.text then texts[#texts + 1] = tostring(current.text) end
    if current.action and current.action.id then actions[#actions + 1] = current.action.id end
    for _, child in ipairs(current.children or {}) do visit(child) end
  end
  visit(root)
  return ids, texts, actions
end

assert(type(review.render) == "function", "the view must export render")

local tree = review.render({}, view)
assert(tree and tree.id == "git-root", "render must return the root")

local ids, texts, actions = walk(tree)
local copy = table.concat(texts, "\n")

-- Every row is present, in the order the producer set.
for index, item in ipairs(view.rows) do
  assert(ids["git-row-" .. index], "row " .. index .. " is missing")
end
assert(#view.rows == 0 or ids["git-row-" .. (#view.rows + 1)] == nil, "rows must not be invented")

-- Titles and the PR coordinate are drawn, so a row is identifiable without
-- opening it.
assert(copy:find(view.rows[1].title, 1, true), "the first title is not drawn")
assert(copy:find("FuturePresentLabs/marbles#26", 1, true), "the PR coordinate is not drawn")
assert(copy:find("no pull request", 1, true), "a row without a PR must say so")

-- A merged PR reads as landable; an open one reads as review. This is the whole
-- distinction the queue exists to make.
assert(copy:find("merged", 1, true), "a merged row is not labelled")
assert(ids["git-row-1-land"], "the merged row offers no action")
assert(ids["git-row-1-land-label"].text == "Land", "a merged row must offer Land, not Review")
assert(ids["git-row-2-land-label"].text == "Review", "an open row must offer Review")

-- A row with no PR offers no land button: there is nothing to land.
assert(ids["git-row-8-land"] == nil, "a row without a PR must not offer Land")
assert(ids["git-row-8-marble"], "a row without a PR should still open its Marble")

-- Intents are emitted, never executed.
assert(#actions > 0, "the view emitted no intents")
local seen = {}
for _, id in ipairs(actions) do seen[id] = true end
for _, expected in ipairs({ "git.select", "git.land", "git.open_marble", "git.filter" }) do
  assert(seen[expected], "missing intent: " .. expected)
end

-- Counts are drawn from the snapshot, not computed here.
assert(copy:find("7 waiting", 1, true), "the waiting count is not drawn")

-- Selection is data: picking another row moves it.
view.selected_id = "motion-guard-noise"
local picked, _ = walk(review.render({}, view))
assert(picked["git-row-5"].selected, "the selected row is not marked")
assert(not picked["git-row-1"].selected, "the previous selection was not cleared")

-- An empty queue is a normal state, not a blank window.
local empty_ids, empty_texts = walk(review.render({}, { title = "REVIEW", counts = {}, rows = {} }))
assert(empty_ids["git-empty"], "an empty queue must render the empty state")
assert(table.concat(empty_texts, "\n"):find("Nothing is waiting", 1, true), "the empty state has no copy")

print("ui: ok")
