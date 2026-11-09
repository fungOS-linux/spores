-- Plain-text render of the worklog view.
--
-- A review aid, not a host: it runs the real view through the real Lua layout
-- fallback and prints the copy in reading order, so structure and wording can be
-- checked without a compositor. Geometry is approximate; widths are not.
--
--   lua tools/preview_worklog.lua [snapshot.lua]

local snapshot_path = arg[1] or "examples/worklog_mock.lua"

local source = assert(io.open("../unibus/ui/lib/ui.lua", "rb"),
  "../unibus/ui/lib/ui.lua is missing; the worklog view composes the shared kit")
local ui = assert(load(source:read("*a"), "ui/lib/ui.lua"))()
source:close()
_G.ui = ui

_G.module = {
  viewport = { width = 1086, height = 1569 },
  theme = { micro = 10, small = 12, body = 16, large = 18 },
}

_G.widget = {}

local drawn = {}
_G.canvas = {
  -- No layout stub: the view falls back to the shared Lua layout, which is what
  -- Canvas runs. A stub here would preview a host that does not exist.
  rect = function(spec) drawn[#drawn + 1] = { primitive = "rect", spec = spec } end,
  text = function(spec) drawn[#drawn + 1] = { primitive = "text", spec = spec } end,
}

local worklog = assert(loadfile("ui/worklog.lua"))()
local snapshot = assert(loadfile(snapshot_path))()
worklog.render(snapshot)

-- `ny(px) = 4 - 8 * px / H`, so a larger normalized y is higher on screen. Band
-- by y and order by x within a band: that is one visual line.
--
-- The list and the detail pane share the same bands, so the cells are split by
-- column first. Printing them interleaved would show two panes on one line and
-- read as a rendering bug.
local SPLIT = "worklog%-detail"
local columns = { left = {}, right = {} }
local texts, rects = 0, 0
for _, item in ipairs(drawn) do
  if item.primitive == "rect" then
    rects = rects + 1
  else
    texts = texts + 1
    local y = item.spec.translation[2]
    local key = string.format("%.2f", y)
    -- Split on the documented id prefix, not on x: the toolbar's right-aligned
    -- counters sit further right than the detail pane starts.
    local column = item.spec.id:find(SPLIT) and columns.right or columns.left
    column[key] = column[key] or { y = y, cells = {} }
    table.insert(column[key].cells, { x = item.spec.translation[1], text = item.spec.text })
  end
end

local function ordered(column)
  local rows = {}
  for _, band in pairs(column) do rows[#rows + 1] = band end
  table.sort(rows, function(left, right) return left.y > right.y end)
  return rows
end

local function print_column(column, heading)
  local rows = ordered(column)
  if #rows == 0 then return end
  print("-- " .. heading)
  for _, band in ipairs(rows) do
    table.sort(band.cells, function(left, right) return left.x < right.x end)
    local parts = {}
    for _, cell in ipairs(band.cells) do parts[#parts + 1] = cell.text end
    print(table.concat(parts, "  \u{00b7}  "))
  end
  print("")
end

print_column(columns.left, "timeline")
print_column(columns.right, "detail")
print(string.format("%d text nodes, %d rects", texts, rects))

