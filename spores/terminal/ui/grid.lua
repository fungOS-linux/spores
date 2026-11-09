-- Terminal grid view. Paints the host's cell grid; owns no VT state, no PTY, no
-- keys. The host runs the session and hands over one frame:
--
--   { cols, rows, title, cursor = {x,y,visible}, scroll = {offset,total,len},
--     rows = { { y, runs = { { text, fg, bg, flags } } } } }
--
-- `fg`/`bg` are `#rrggbb` or nil (the terminal default). This is the one view
-- in the tree that names literal colour: sixteen ANSI colours are not sixteen
-- theme roles, and a terminal that ignores the program's colour is not the
-- program's terminal. The theme still supplies the surface it sits on.
--
-- Geometry: a grid is the one layout that must not be negotiated. The cell size
-- is the surface divided by the grid, so the same frame draws at 80x24 and at
-- 240x72 without the host measuring anything.
local ui = require("fungos.ui")

local M = { name = "dev.fungos.terminal.grid", version = "0.1.0" }

local BOLD, ITALIC, UNDERLINE, FAINT, INVERSE = 1, 2, 4, 8, 16

-- Monospace advance, as a fraction of the font size. Every terminal font worth
-- the name is near 0.6em; the page sets the same number in CSS so a run lands in
-- the column the view placed it in.
local ADVANCE = 0.6
local LEADING = 1.2

local surface

--- The cell size this grid draws at. Exported because a click is a cell, and the
--- page has to convert pixels back with the same arithmetic the view used.
function M.cell(cols, rows, width, height)
  cols = math.max(1, tonumber(cols) or 80)
  rows = math.max(1, tonumber(rows) or 24)
  local cw = width / cols
  local size = cw / ADVANCE
  local ch = size * LEADING
  if ch * rows > height then
    ch = height / rows
    size = ch / LEADING
    cw = size * ADVANCE
  end
  return cw, ch, size
end

local function rgb(value)
  if type(value) ~= "string" then return nil end
  local r, g, b = value:match("^#(%x%x)(%x%x)(%x%x)$")
  if not r then return nil end
  return { tonumber(r, 16), tonumber(g, 16), tonumber(b, 16) }
end

local function ink(colour, fallback)
  local value = rgb(colour)
  if not value then return fallback end
  return { ink = fallback, rgb = value }
end

local function paint_row(row, cols, y, cw, ch, size)
  local x = 0
  for index, run in ipairs(row.runs or {}) do
    local text = tostring(run[1] or "")
    local fg = run[2]
    local bg = run[3]
    local flags = tonumber(run[4]) or 0

    -- A run of spaces is not a glyph: the cell is already the background. It
    -- still advances the column, so the runs after it land where they belong.
    local blank = text:match("^%s*$") ~= nil

    if bg then
      surface:rect("cell-bg-" .. tostring(row.y) .. "-" .. index,
        x * cw, y, math.max(cw, #text * cw), ch, ink(bg, "surface"), 1, 0)
    end

    local fg_ink = ink(fg, "text")
    local bg_ink = bg and ink(bg, "surface") or nil
    if flags % (INVERSE * 2) >= INVERSE then
      -- Inverse is a swap, not a colour: the program asked for "whatever is
      -- behind me, in front". Without a known background the theme's surface is
      -- the honest answer.
      fg_ink = bg_ink or ink(nil, "background")
      if bg_ink == nil then
        surface:rect("cell-inv-" .. tostring(row.y) .. "-" .. index,
          x * cw, y, math.max(cw, #text * cw), ch, ink(fg, "text"), 1, 0)
      end
    end
    if flags % (FAINT * 2) >= FAINT then
      -- Faint text is still text; the scene has no alpha per glyph, so the
      -- muted role is the closest thing that stays readable.
      fg_ink = ink(fg, "muted")
    end

    if not blank then
      surface:text("cell-" .. tostring(row.y) .. "-" .. index, text, x * cw, y, size, fg_ink)
    end

    if flags % (UNDERLINE * 2) >= UNDERLINE then
      surface:rect("cell-ul-" .. tostring(row.y) .. "-" .. index,
        x * cw, y + ch - 1, math.max(cw, #text * cw), 1, fg_ink, 1, 0)
    end

    x = x + #text
    if x >= cols then break end
  end
end

function M.render(frame)
  frame = frame or {}
  local width, height = ui.viewport(1200, 900)
  local cols = math.max(1, tonumber(frame.cols) or 80)
  local rows = math.max(1, tonumber(frame.rows) or 24)
  local cw, ch, size = M.cell(cols, rows, width, height)

  surface = ui.surface(width, height, ui.theme())
  surface:rect("terminal-bg", 0, 0, width, height, "background", 1, 0)

  for _, row in ipairs(frame.rows or {}) do
    local y = (tonumber(row.y) or 0) * ch
    if y + ch <= height + 0.5 then
      paint_row(row, cols, y, cw, ch, size)
    end
  end

  -- The page converts a click back into a cell and a resize into a new grid, so
  -- it needs the numbers the view drew with, not a second copy of the arithmetic.
  M.meta = {
    cols = cols,
    rows = rows,
    cell_width = cw,
    cell_height = ch,
    -- Empty means the shell set no title (OSC 0); the page keeps its own name
    -- rather than replacing it with a blank one.
    title = type(frame.title) == "string" and frame.title or "",
    scroll = frame.scroll and tonumber(frame.scroll.offset) or 0,
  }

  local cursor = frame.cursor
  if type(cursor) == "table" and cursor.visible ~= false then
    surface:rect("terminal-cursor", (tonumber(cursor.x) or 0) * cw, (tonumber(cursor.y) or 0) * ch,
      math.max(2, cw), ch, "primary", 0.55, 0)
  end
end

return M