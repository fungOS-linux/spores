-- Fixture for the files view. Run from spores/files:
--   FUNGOS_SDK=<fungos-sdk checkout> lua tests/ui_fixture.lua
--
-- Canvas hands a module `canvas.rect` and `canvas.text`: no layout service, no
-- text measurement, no hit-testing. So the fixture stubs exactly those two
-- primitives and holds the view to the things the host cannot check for it —
-- every node inside the surface, one id per node, one node per line, a line that
-- fits the column it was measured for, and presses whose intent is the path the
-- reader is looking at.

-- The library is the SDK package, loaded the way a host loads it: through
-- `require`. `FUNGOS_SDK` points at a fungos-sdk checkout.
local sdk = os.getenv("FUNGOS_SDK") or ((os.getenv("HOME") or "") .. "/src/fungos-sdk")
local lib_path = sdk .. "/lua/fungos/ui.lua"
local lib_source = assert(io.open(lib_path, "rb"),
  "no fungos.ui at " .. lib_path .. "; set FUNGOS_SDK to a fungos-sdk checkout")
local lib_chunk = assert(load(lib_source:read("*a"), "@" .. lib_path))
lib_source:close()
package.preload["fungos.ui"] = function() return lib_chunk() end

-- The scale the wall publishes: a 1.5x theme in pixels. Canvas sizes text as
-- `scale * theme.body * depth(z)`, so `body` is what it draws with.
local THEME = { micro = 15, small = 18, body = 24, large = 27 }

local view = { width = 1182, height = 820 }
local drawn = { rects = {}, texts = {} }

-- The library's advance estimate, which is also what `surface:measure_text`
-- returns on a host with no `canvas.measure_text` — and Canvas has none.
local function width_of(value, size) return #tostring(value) * size * 0.58 end
local function text_size(spec) return spec.scale[1] * THEME.body * 1.25 end

-- Normalized scene units are 8 wide and 8 tall over the whole surface with the
-- origin at the top-left.
local function place(spec)
  return (spec.translation[1] + 4) / 8 * view.width,
    (4 - spec.translation[2]) / 8 * view.height
end

local files = assert(loadfile("ui/files.lua"))()

local function install_host(viewport, theme)
  view = viewport
  _G.module = { viewport = { width = view.width, height = view.height }, theme = theme or THEME }
  _G.widget = {}
  _G.canvas = {
    rect = function(spec) drawn.rects[#drawn.rects + 1] = spec end,
    text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
  }
end

local function render(snapshot)
  drawn = { rects = {}, texts = {} }
  for index = #files.pending_presses, 1, -1 do files.pending_presses[index] = nil end
  assert(files.render(snapshot) == nil, "render draws through the host and returns nothing")

  local ids, lines = {}, {}
  for _, spec in ipairs(drawn.texts) do
    lines[#lines + 1] = tostring(spec.text)
    assert(not ids[spec.id], "duplicate node id: " .. tostring(spec.id))
    ids[spec.id] = true
    local x, y = place(spec)
    local size = text_size(spec)
    assert(x >= -0.5 and y >= -0.5, "text starts outside the surface: " .. tostring(spec.id))
    assert(x + width_of(spec.text, size) <= view.width + 0.5,
      string.format("%s runs past the right edge (x=%.1f w=%.1f surface=%.1f text=%q)",
        tostring(spec.id), x, width_of(spec.text, size), view.width, tostring(spec.text)))
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
  local total = #drawn.rects + #drawn.texts + #files.pending_presses
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

local function has_id(id)
  for _, spec in ipairs(drawn.texts) do if spec.id == id then return true end end
  for _, spec in ipairs(drawn.rects) do if spec.id == id then return true end end
  return false
end

install_host({ width = 1182, height = 820 })
local snapshot = assert(loadfile("examples/files_mock.lua"))()
local copy = render(snapshot)

assert(#drawn.texts > 0, "the view drew no text")
assert(#drawn.rects > 0, "the view drew no rects")
for _, spec in ipairs(drawn.texts) do
  assert(type(spec.text) == "string", "text primitive without a string")
  assert(type(spec.ink) == "string", "text primitive without an ink role")
end

-- The view draws the size it was granted, not the size it would have preferred.
local backdrop = assert(rect_of("files-bg"), "the view draws no backdrop")
assert(math.abs(backdrop.w - view.width) < 0.5 and math.abs(backdrop.h - view.height) < 0.5,
  string.format("the backdrop is %.0fx%.0f on a %dx%d surface", backdrop.w, backdrop.h, view.width, view.height))
local rail = assert(rect_of("rail-panel"), "the view draws no locations rail")

-- The three panes sit side by side. The rule between the rail and the list is a
-- real node, and the list starts after it.
local rule = assert(rect_of("rail-rule"), "the view draws no rail rule")
assert(rule.x > rail.x + rail.w - 1 and rule.x < rail.x + rail.w + 16, "the rail rule is not at the rail edge")
local crumb = assert(text_at("crumb"), "the view draws no breadcrumb")
assert(crumb.x > rule.x, "the list starts inside the rail")

-- The numbers on the page are the snapshot's numbers.
assert(copy:find("files", 1, true), "masthead is missing")
assert(copy:find("68.4 GB", 1, true), "the total is not drawn")
assert(copy:find("18,422 files", 1, true), "the file count is not drawn")
assert(copy:find("Studio", 1, true) and copy:find("Archive", 1, true), "locations are not drawn")
-- Each location answers "how big" and "how many files" without being opened.
assert(has_id("root-1-files") and has_id("root-2-files"), "a location's file count is not drawn")
assert(copy:find("Studio  /  Projects  /  orbit", 1, true), "the breadcrumb is not the path")
assert(copy:find("assets/", 1, true), "a directory row is not drawn with its separator")
assert(copy:find("README.md", 1, true), "a file row is not drawn")
assert(copy:find("2026-11-08", 1, true), "a modification date is not drawn")
assert(copy:find("Images", 1, true) and copy:find("Video", 1, true), "the census legend is not drawn")
assert(copy:find("orbit-session.mov", 1, true), "the largest-files list is not drawn")
assert(copy:find("scanned 20,326", 1, true), "the scan count is not drawn")
assert(copy:find("1 folder unreadable", 1, true), "a failed subtree is not disclosed")

-- The stack is width-proportional: Images is ~41% of the total, so its band is
-- the widest. A bar that ignores bytes is a bar that lies.
local track = assert(rect_of("kind-bar-track"), "the view draws no census bar")
local bands = {}
for _, spec in ipairs(drawn.rects) do
  if tostring(spec.id):match("^kind%-band%-") then bands[#bands + 1] = spec end
end
assert(#bands >= 3, "the census bar has too few bands")
table.sort(bands, function(a, b) return a.scale[1] > b.scale[1] end)
assert(bands[1].id == "kind-band-1", "the largest class is not the widest band, got " .. tostring(bands[1].id))
local widest = bands[1].scale[1] / 8 * view.width
assert(math.abs(widest / track.w - 0.41) < 0.03,
  "the Images band does not match its share of the total")

-- Rows are planned against the surface, never painted past the bottom edge, and
-- the count the view publishes is the count it drew.
assert(files.rows_drawn > 0, "no rows were drawn")
assert(files.rows_drawn <= files.rows_planned, "the draw exceeded its own plan")
for index = 1, files.rows_drawn do
  local row = assert(rect_of("row-" .. index .. "-hover") or true)
  assert(has_id("row-" .. index .. "-name"), "row " .. index .. " has no name")
  local name = assert(text_at("row-" .. index .. "-name"))
  assert(row and name.y + name.size * 1.3 <= view.height + 0.5, "row " .. index .. " is past the bottom edge")
end

-- Every press names an intent and carries the path it was made on. A directory
-- row is pressable; a file row is not, because this Spore opens nothing yet.
local open_src, up = false, false
for _, spec in ipairs(files.pending_presses) do
  assert(spec.action, "a press without an action id")
  assert(spec.body and type(spec.body) == "table", "a press without a body")
  assert(spec.x >= -4 and spec.x <= 4 and spec.y >= -4 and spec.y <= 4, "a press outside the surface")
  if spec.action == "files-open" and spec.body.path == "Projects/orbit/src" then open_src = true end
  if spec.action == "files-up" and spec.body.path == "Projects" then up = true end
end
assert(open_src, "the src directory row is not pressable with its path")
assert(up, "the breadcrumb is not a way up")

-- A view is a function of the snapshot: another folder changes what is drawn.
snapshot.path = ""
snapshot.crumbs = { { name = "Studio", path = "" } }
snapshot.listing = {}
snapshot.listing_total = 0
local empty_copy = render(snapshot)
assert(empty_copy:find("This folder is empty", 1, true), "an empty folder should say so")
assert(not empty_copy:find("README.md", 1, true), "a folder change did not change the listing")
assert(empty_copy ~= copy, "the view did not change with the snapshot")

-- A narrow window keeps the rail and the list and drops the inspector rather
-- than drawing three columns on top of each other.
install_host({ width = 760, height = 700 })
local narrow = render(assert(loadfile("examples/files_mock.lua"))())
assert(not has_id("inspect-total"), "the inspector should be gone at 760 px")
assert(has_id("crumb"), "the list should survive at 760 px")
assert(narrow:find("files", 1, true), "the masthead should survive at 760 px")

-- A location whose first scan is still running says so instead of claiming to
-- be empty.
install_host({ width = 1182, height = 820 })
render({
  title = "files", workspace = "Studio", root = "studio", path = "",
  crumbs = { { name = "Studio", path = "" } },
  roots = {
    { id = "studio", name = "Studio", bytes = 100, files = 2, folders = 1, current = true, available = true },
    { id = "archive", name = "Archive", bytes = 0, files = 0, folders = 0, current = false, available = false },
  },
  totals = { bytes = 100, files = 2, folders = 1, scanned = 3, errors = 0, truncated = false },
  kinds = { { id = "text", label = "Text", bytes = 100, files = 2 } },
  listing = {}, listing_total = 0, listing_truncated = false, largest = {},
})
local pending_files = assert(text_at("root-2-files"), "the pending location has no status line")
assert(pending_files.text == "scanning", "a pending location does not say scanning: " .. pending_files.text)
local pending_size = assert(text_at("root-2-size"), "the pending location has no size line")
assert(pending_size.text == "\u{2014}", "a pending location should not claim a size")

-- A host error is a state to draw, not a crash and not an empty folder.
install_host({ width = 1182, height = 820 })
local failed = render({
  title = "files",
  workspace = "Studio",
  root = "studio",
  path = "gone",
  crumbs = { { name = "Studio", path = "" }, { name = "gone", path = "gone" } },
  roots = { { id = "studio", name = "Studio", bytes = 0, files = 0, folders = 0, current = true } },
  totals = { bytes = 0, files = 0, folders = 0, scanned = 0, errors = 1, truncated = false },
  kinds = {},
  listing = {},
  listing_total = 0,
  largest = {},
  error = "gone: not found",
})
assert(failed:find("not found", 1, true), "a host error is not drawn")

-- Units are presentation, so the view owns them; the numbers they wrap are not.
assert(files.format_bytes(0) == "0 B")
assert(files.format_bytes(512) == "512 B")
assert(files.format_bytes(1024) == "1.00 KB")
assert(files.format_bytes(1536) == "1.50 KB")
assert(files.format_bytes(10 * 1024 * 1024) == "10.0 MB")
assert(files.format_bytes(73400320000) == "68.4 GB")
assert(files.format_count(0) == "0")
assert(files.format_count(100) == "100")
assert(files.format_count(1000) == "1,000")
assert(files.format_count(18422) == "18,422")

print("ui: ok")
