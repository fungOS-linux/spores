-- Fixture for the new declarative inbox UI. Run from spores/unibus:
--   lua tests/ui_fixture.lua
local source = assert(io.open("ui/lib/ui.lua", "rb"), "ui/lib/ui.lua missing")
local ui = assert(load(source:read("*a"), "ui/lib/ui.lua"))()
source:close()
_G.ui = ui
_G.module = {
  theme = {
    title = 0.34, large = 0.24, body = 0.145,
    small = 0.105, micro = 0.085, radius = 0.02,
  },
}

local inbox = assert(loadfile("ui/inbox.lua"))()
local snapshot = assert(loadfile("examples/inbox_mock.lua"))()
local tree = inbox.tree(snapshot)
assert(tree.kind == "column", "root is a column")
assert(#tree.children == 4, "header, summary, main, footer")

-- The selected thread is the support refund, and it carries a draft.
local selected = nil
for _, thread in ipairs(snapshot.threads) do
  if thread.selected then selected = thread end
end
assert(selected and selected.id == "support/refund-441", "selected support thread")
assert(selected.draft and #selected.draft > 0, "draft is present")

local function values(node, out)
  out = out or {}
  if node.kind == "text" then out[#out + 1] = node.value end
  if node.kind == "button" then out[#out + 1] = node.label end
  if node.kind == "icon" or node.kind == "icon_button" then out[#out + 1] = node.glyph end
  for _, child in ipairs(node.children or {}) do values(child, out) end
  return out
end
local text_values = table.concat(values(tree), "\n")
assert(text_values:find("MAIL · 1–6 of 9", 1, true), "paged range is visible")
assert(text_values:find("↓ 3 more", 1, true), "remaining count is visible")

snapshot.layout.list_offset = 3
local second = inbox.tree(snapshot)
assert(table.concat(values(second), "\n"):find("MAIL · 4–9 of 9", 1, true), "offset page")
snapshot.layout.list_offset = 0

-- Narrow mode removes the rail and shows one pane.
snapshot.layout.width_class = "narrow"
snapshot.layout.focus = "list"
local narrow_list = table.concat(values(inbox.tree(snapshot)), "\n")
assert(narrow_list:find("MAIL · 1–6 of 9", 1, true), "narrow list")
assert(not narrow_list:find("FILTERS", 1, true), "narrow hides rail")
snapshot.layout.focus = "detail"
local narrow_detail = table.concat(values(inbox.tree(snapshot)), "\n")
assert(narrow_detail:find("AGENT BRIEF", 1, true), "narrow detail")
assert(narrow_detail:find("← Back", 1, true), "narrow back")
snapshot.layout.width_class = "wide"
snapshot.layout.focus = "list"

for mode, needle in pairs({
  empty = "Inbox zero",
  loading = "Syncing",
  offline = "Offline",
  error = "Could not sync",
}) do
  snapshot.state = { mode = mode, message = "provider said no" }
  local shown = table.concat(values(inbox.tree(snapshot)), "\n")
  assert(shown:find(needle, 1, true), mode .. " state")
  assert(not shown:find("MAIL ·", 1, true), mode .. " hides list")
end
snapshot.state = { mode = "ready" }

-- Exercise the Canvas path with a recorder. The preview only uses tree().
local rects, texts = 0, 0
_G.canvas = {
  rect = function() rects = rects + 1 end,
  text = function() texts = texts + 1 end,
}
inbox.render(snapshot)
assert(rects > 10, "rects emitted")
assert(texts > 10, "text emitted")
print("ui: ok")
