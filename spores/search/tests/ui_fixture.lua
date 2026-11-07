-- Fixture for the quick-search view. Run from spores/search:
--   lua tests/ui_fixture.lua

local source = assert(io.open("../../sdk/ui/ui.lua", "rb"), "sdk missing")
local ui = assert(load(source:read("*a"), "ui"))()
source:close()
_G.ui = ui

_G.module = {
  inputs = {
    width = 800,
    height = 600,
    query = "safari",
    hits = {
      { id = "1", title = "Safari", subtitle = "Web browser", source = "apps", kind = "application" },
      { id = "2", title = "safari-log.txt", subtitle = "", source = "vfs", kind = "file" },
      { id = "3", title = "Terminal", subtitle = "Utilities", source = "apps", kind = "application", icon = ">" },
    },
  },
  theme = {
    title = 0.34, large = 0.24, body = 0.145, small = 0.12,
    micro = 0.085, radius = 0.02,
  },
}

local drawn = { rects = {}, texts = {} }
_G.canvas = {
  rect = function(s) drawn.rects[#drawn.rects + 1] = s end,
  text = function(s) drawn.texts[#drawn.texts + 1] = s end,
}

_G.widget = { button = function() end }

local search = assert(loadfile("ui/launcher.lua"))()
local snapshot = assert(_G.module.inputs, "module inputs missing")

search.render(snapshot)

local function find_text(value)
  for _, node in ipairs(drawn.texts) do
    if node.text == value then return node end
  end
  return nil
end

-- It drew something, through the host.
assert(#drawn.rects > 0, "no rects")
assert(#drawn.texts > 0, "no text")

-- The header reads the source, not a constant.
assert(find_text("Safari"), "hit title not drawn")
assert(find_text("Web browser"), "hit subtitle not drawn")
assert(find_text("safari-log.txt"), "file hit not drawn")

-- A row says `App`, never the protocol's `application`.
assert(find_text("App"), "kind drawn with its short label")
assert(find_text("File"), "file kind drawn with its short label")
assert(not find_text("application"), "kind label is the raw protocol word")

-- A host-nominated mark is drawn as given; absent, the title supplies one.
assert(find_text(">"), "host icon not drawn")
assert(find_text("S"), "monogram not drawn")

-- Empty query yields an empty state.
local empty_snapshot = { width = 800, height = 600, query = "", hits = {} }
_G.module.inputs = empty_snapshot
search.render(empty_snapshot)
assert(find_text("No results"), "empty state missing")

print("ui: ok")
