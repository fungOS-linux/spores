-- Fixture for the search UI. Run from spores/search:
--   lua tests/ui_fixture.lua
--
-- Canvas has no viewport in the snapshot, so the fixture supplies one and a
-- mock canvas that lays the tree out flat (every node gets a rect) so the view
-- can place backgrounds, dividers and hit targets exactly as it will on-device.

local source = assert(io.open("ui/search.lua", "rb"), "ui/search.lua missing")
local view = assert(load(source:read("*a"), "ui/search.lua"))()
source:close()

local W, H = 1024, 704

local function mock_canvas(collect)
  local nodes = {}
  local cursor = 0
  local function walk(node)
    local h = (node.layout and node.layout.height) or 12
    node.rect = { x = 0, y = cursor, width = W, height = h }
    cursor = cursor + h
    table.insert(nodes, node)
    if node.children then
      for _, child in ipairs(node.children) do walk(child) end
    end
  end
  return {
    layout = function(tree) walk(tree) return { nodes = nodes } end,
    rect = function(spec) collect.rects = collect.rects + 1; collect.ids[spec.id] = true end,
    text = function(spec)
      collect.texts = collect.texts + 1
      collect.ids[spec.id] = true
      collect.text[spec.id] = spec.text
    end,
  }
end

local function mock_widget(collect)
  return { hit = function(spec) collect.hits[spec.id] = true end }
end

local function fresh(collect)
  collect.rects = 0
  collect.texts = 0
  collect.ids = {}
  collect.text = {}
  collect.hits = {}
end

-- Wide viewport: three panes render.
_G.module = {
  theme = { body = 16 },
  viewport = { width = W, height = H },
}
local snapshot = assert(loadfile("examples/search_mock.lua"))()
local collect = {}
local selected_key
_G.module.select = function(key) selected_key = key end
_G.canvas = mock_canvas(collect)
_G.widget = mock_widget(collect)

fresh(collect)
view.render(snapshot)
assert(collect.rects >= 4, "rects emitted")
assert(collect.texts > 5, "text emitted")
assert(collect.ids["search-title"], "sidebar title present")
assert(collect.text["search-query"] and collect.text["search-query"]:find("greece", 1, true), "query is shown")
assert(collect.hits["result-1"], "result hit targets are routed")
assert(collect.text["detail-name"] and collect.text["detail-name"]:find("greece-01.jpg", 1, true),
  "selected entry is in the detail pane")
assert(collect.text["detail-preview"] and collect.text["detail-preview"]:find("image/jpeg", 1, true),
  "detail preview is shown")

-- Activating a different result moves the selection.
_G.module.event = { kind = "activate", target_id = "result-1" }
local moved = {}
_G.canvas = mock_canvas(moved)
_G.widget = mock_widget(moved)
fresh(moved)
view.render(snapshot)
assert(moved.text["detail-name"] and moved.text["detail-name"]:find("greece-notes.txt", 1, true),
  "activating result-1 moves the selection to its detail")
assert(selected_key and selected_key:find("greece%-notes%.txt"), "selection is remembered by the host")

-- A fresh render restores the host-owned selection instead of resetting to hit 1.
local restored = assert(loadfile("examples/search_mock.lua"))()
_G.module.event = nil
_G.module.view = { selected = selected_key }
local remembered = {}
_G.canvas = mock_canvas(remembered)
_G.widget = mock_widget(remembered)
fresh(remembered)
view.render(restored)
assert(remembered.text["detail-name"] and remembered.text["detail-name"]:find("greece-notes.txt", 1, true),
  "selection survives a fresh render")

-- An empty query browses; the UI still renders a coherent pane.
snapshot.query = ""
snapshot.mode = "browse"
local browsed = {}
_G.canvas = mock_canvas(browsed)
_G.widget = mock_widget(browsed)
fresh(browsed)
view.render(snapshot)
assert(browsed.rects >= 4, "browse still renders")
assert(browsed.text["search-query"]:find("Search files", 1, true), "empty query shows the placeholder")

print("ui: ok")
