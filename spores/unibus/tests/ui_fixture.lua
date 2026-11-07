-- Fixture for the inbox view. Run from spores/unibus:
--   lua tests/ui_fixture.lua
--
-- The view delegates geometry to the host: it builds a node tree and hands it to
-- `canvas.layout`, which in Canvas is the Taffy layout server. A fixture has no
-- layout engine, so this file supplies a minimal host that flattens the tree and
-- gives every node a placeholder rect. The assertions are therefore about
-- structure and copy, never about geometry.

local source = assert(io.open("ui/lib/ui.lua", "rb"), "ui/lib/ui.lua missing")
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
  -- the draw pass reads (kind, style, text) and replacing only the geometry.
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

_G.widget = {
  hit = function(spec) drawn.hits = (drawn.hits or 0) + 1 end,
}

local inbox = assert(loadfile("ui/inbox.lua"))()
local snapshot = assert(loadfile("examples/inbox_mock.lua"))()

local function render(snap)
  drawn = { rects = {}, texts = {} }
  assert(inbox.render(snap) == nil, "render draws through the host and returns nothing")
  local joined = {}
  for _, spec in ipairs(drawn.texts) do joined[#joined + 1] = tostring(spec.text) end
  return table.concat(joined, "\n")
end

local copy = render(snapshot)

-- It drew something, through the host, and none of it is a picture: every text
-- node is a string Canvas shapes itself.
assert(#drawn.texts > 0, "the view drew no text")
assert(#drawn.rects > 0, "the view drew no rects")
for _, spec in ipairs(drawn.texts) do
  assert(type(spec.text) == "string", "text primitive without a string")
  assert(type(spec.ink) == "string", "text primitive without an ink role")
end

-- The header reads from the snapshot, not from a constant.
local account = snapshot.account and snapshot.account.email
assert(account and copy:find(account, 1, true), "account is not drawn")
local counts = snapshot.counts or {}
assert(copy:find(string.format("%d unread", counts.unread or 0), 1, true), "unread count is not drawn")

-- The list is paged copy, so the range and the sort are visible rather than
-- implied.
assert(copy:find("MAIL", 1, true), "list header is missing")
assert(copy:find("Newest first", 1, true), "sort order is missing")

-- The selected thread drives the detail pane.
local selected
for _, thread in ipairs(snapshot.threads or {}) do
  if thread.selected then selected = thread end
end
assert(selected and selected.id == "support/refund-441", "selected support thread")
assert(selected.draft and #selected.draft > 0, "draft is present")
assert(copy:find(selected.title, 1, true), "selected subject is not drawn")

-- Every detail action is a drawn label the host hit-tests.
for _, label in ipairs({ "Reply", "Reply all", "Assign", "Resolve", "Archive" }) do
  assert(copy:find(label, 1, true), "missing detail action: " .. label)
end

-- A view is a function of the snapshot: selecting another thread has to change
-- what is drawn, or the detail pane is decoration.
local other
for _, thread in ipairs(snapshot.threads or {}) do
  if thread.id ~= selected.id then other = thread break end
end
assert(other, "fixture needs a second thread")
for _, thread in ipairs(snapshot.threads) do thread.selected = thread.id == other.id end
local other_copy = render(snapshot)
assert(other_copy ~= copy, "selection did not change the view")
assert(other_copy:find(other.title, 1, true), "newly selected subject is not drawn")

print("ui: ok")

-- The real Canvas host provides no `widget.hit`: its `widget` is the theming
-- library (pixel_space, button, panel, label, badge, metric) and Soil exposes no
-- hit targets. Rendering there must not crash, and a hit spec must not vanish.
--
-- This exercises the guard directly rather than through the render path: the
-- fixture's placeholder layout never produces the row ids the hit loop looks
-- for, so a render-level assertion would pass without ever reaching the guard.
local themed_widget = _G.widget

_G.widget = {}
drawn = { rects = {}, texts = {} }
for index = #inbox.pending_hits, 1, -1 do inbox.pending_hits[index] = nil end

local rendered, render_error = pcall(inbox.render, snapshot)
assert(rendered, "render crashed without widget.hit: " .. tostring(render_error))
assert(#drawn.texts > 0, "the no-hit host drew nothing")

inbox.hit({ id = "probe", label = "hit", x = 0, y = 0, width = 1, height = 1 })
assert(#inbox.pending_hits == 1, "hit spec was dropped instead of kept for the host")
assert(inbox.pending_hits[1].id == "probe", "kept the wrong spec")

local delivered = nil
_G.widget = { hit = function(spec) delivered = spec end }
inbox.hit({ id = "probe2", label = "hit", x = 0, y = 0, width = 1, height = 1 })
assert(delivered and delivered.id == "probe2", "hit was not delivered to a host that provides it")
assert(#inbox.pending_hits == 1, "a delivered hit should not also be queued")

_G.widget = themed_widget

print("ui: no-hit host ok")
