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

local function text(id, value, x, y, size, ink, z)
  surface:text(id, value, x, y, size, ink, z)
end

local function clip(value, limit)
  value = tostring(value or "")
  if #value <= limit then return value end
  return value:sub(1, limit - 1) .. "…"
end

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

local function paint_row(row, cols, y, cw, ch, size, ox)
  local x = 0
  local runs = {}
  for index, run in ipairs(row.runs or {}) do
    local text = tostring(run[1] or "")
    local fg = run[2]
    local bg = run[3]
    local flags = tonumber(run[4]) or 0

    -- A row is one line; a run is how part of it is painted. The line is
    -- truncated at the column the grid ends on, because the runs are now
    -- concatenated into one node and nothing else would stop them overflowing.
    local span = math.min(#text, cols - x)
    local piece = span > 0 and text:sub(1, span) or ""
    local run_w = math.max(cw, span * cw)

    if bg and span > 0 then
      surface:rect("cell-bg-" .. tostring(row.y) .. "-" .. index,
        ox + x * cw, y, run_w, ch, ink(bg, "surface"), 1, 0)
    end

    local fg_ink = ink(fg, "text")
    local bg_ink = bg and ink(bg, "surface") or nil
    if flags % (INVERSE * 2) >= INVERSE then
      -- Inverse is a swap, not a colour: the program asked for "whatever is
      -- behind me, in front". Without a known background the theme's surface is
      -- the honest answer.
      fg_ink = bg_ink or ink(nil, "background")
      if bg_ink == nil and span > 0 then
        surface:rect("cell-inv-" .. tostring(row.y) .. "-" .. index,
          ox + x * cw, y, run_w, ch, ink(fg, "text"), 1, 0)
      end
    end
    if flags % (FAINT * 2) >= FAINT then
      -- Faint text is still text; the scene has no alpha per glyph, so the
      -- muted role is the closest thing that stays readable.
      fg_ink = ink(fg, "muted")
    end

    -- Spaces stay in the run list: the runs are concatenated into one node, so
    -- a blank run is what keeps the run after it in its column.
    if piece ~= "" then
      runs[#runs + 1] = { text = piece, ink = fg_ink }
    end

    if flags % (UNDERLINE * 2) >= UNDERLINE and span > 0 then
      surface:rect("cell-ul-" .. tostring(row.y) .. "-" .. index,
        ox + x * cw, y + ch - 1, run_w, 1, fg_ink, 1, 0)
    end

    x = x + #text
    if x >= cols then break end
  end

  -- One node for the line. A row that changes colour ten times is still one
  -- line, and painting it as ten nodes made a frame cost a node per run.
  if #runs > 0 then
    surface:text_runs("row-" .. tostring(row.y), runs, ox, y, size, nil)
  end
end

-- The navigator is part of the terminal, not of the page that shows it: a Spore
-- view that can only draw a grid needs a second UI beside it to pick a device,
-- and then the picker is HTML again. This draws the fleet and the session table
-- in the Spore's own scene and publishes where each row landed, so a click is a
-- lookup rather than a second copy of the layout.
local SIDEBAR = 232
local TAB = 34
local ROW = 26
local GROUP = { { "shell", "shells" }, { "ssh", "hosts" }, { "fleet", "fleet" }, { "mush", "desktops" } }

local function sidebar_rows(frame)
  local rows = {}
  for _, group in ipairs(GROUP) do
    local kind, heading = group[1], group[2]
    local targets = {}
    for _, target in ipairs(frame.fleet or {}) do
      if target.kind == kind then targets[#targets + 1] = target end
    end
    if #targets > 0 then
      rows[#rows + 1] = { heading = heading, kind = kind }
      for _, target in ipairs(targets) do
        rows[#rows + 1] = { kind = kind, target = target }
      end
    end
  end
  return rows
end

local function paint_sidebar(frame, height, top, offset)
  local rows = sidebar_rows(frame)
  local total = 0
  for _, row in ipairs(rows) do
    if not row.heading then total = total + 1 end
  end
  -- The offset is a window over the device list, clamped to what can actually
  -- scroll: an offset past the end would draw an empty panel and look like the
  -- fleet went away.
  local capacity = math.max(1, math.floor((height - top - 10) / ROW))
  offset = math.min(math.max(0, offset), math.max(0, total - capacity))

  local items, devices, shown = {}, 0, 0
  local y = top + 10
  local heading = nil
  surface:rect("nav-bg", 0, top, SIDEBAR, height - top, "surface", 0.22, 0, 0.05)

  for _, row in ipairs(rows) do
    if row.heading then
      -- A heading names the rows under it, so it scrolls with them rather than
      -- pinning to the top: a label that stays while its list moves is a lie
      -- about what is on screen.
      heading = row
    else
      devices = devices + 1
      if devices > offset then
        if heading then
          if shown > 0 then y = y + 8 end
          if y + 18 + ROW > height then break end
          text("nav-group-" .. heading.kind, string.upper(heading.heading), 12, y, 9, "muted")
          y = y + 18
          heading = nil
        end
        if y + ROW > height then break end
        local target = row.target
        text("nav-target-" .. row.kind .. "-" .. devices, clip(target.label or target.name, 24), 12, y, 10, "text")
        items[#items + 1] = {
          kind = "target",
          name = target.name,
          y = y - 3,
          height = ROW - 4,
        }
        y = y + ROW
        shown = shown + 1
      end
    end
  end

  M.meta = M.meta or {}
  M.meta.sidebar = {
    width = SIDEBAR,
    items = items,
    -- The offset the view actually used, not the one it was asked for: the page
    -- clamps its own wheel against this, so it has to see the clamp.
    offset = offset,
    total = total,
    visible = shown,
  }
end

--- How far the host has scrolled the device list.
---
--- The terminal's scrollback and its device list are two different lists, so
--- they are two different offsets: `module.scroll` belongs to the producer's
--- history, `module.sidebar` to the panel this view draws. Without a host the
--- list starts at the top.
local function sidebar_offset()
  local host = rawget(_G, "module")
  if type(host) ~= "table" or type(host.sidebar) ~= "table" then return 0 end
  return math.max(0, math.floor(tonumber(host.sidebar.offset) or 0))
end

function M.render(frame)
  frame = frame or {}
  local width, height = ui.viewport(1200, 900)
  local cols = math.max(1, tonumber(frame.cols) or 80)
  local rows = math.max(1, tonumber(frame.rows) or 24)
  local sidebar = (frame.fleet and #frame.fleet > 0) and SIDEBAR or 0
  local ox = sidebar > 0 and sidebar or 0
  local tabs = frame.sessions or {}
  local tab_height = #tabs > 0 and TAB or 0
  local cw, ch, size = M.cell(cols, rows, width - ox, height - tab_height)

  local theme = ui.theme()
  surface = ui.surface(width, height, theme)
  M.meta = {}
  surface:rect("terminal-bg", 0, 0, width, height, "background", 1, 0)
  if tab_height > 0 then
    -- Sessions are a tab strip, not a list: a session is a place you are, and
    -- the thing you are in should be visible without opening a menu.
    local strip = {}
    for index, session in ipairs(tabs) do
      strip[#strip + 1] = {
        id = tostring(session.id),
        label = clip(session.label or session.target, 24),
        selected = tonumber(session.id) == tonumber(frame.active),
        muted = session.exited == true,
        action = "session:" .. tostring(session.id),
      }
    end
    M.meta.tabs = ui.paint_tabs(surface, 0, 0, width - ox, tab_height, strip, { size = 11, z = 0.07 })
    surface:rect("tab-strip-rule", 0, tab_height - 1, width, 1, "muted", 0.18, 0, 0.06)
  end
  if sidebar > 0 then paint_sidebar(frame, height, tab_height, sidebar_offset()) end

  for _, row in ipairs(frame.lines or {}) do
    local y = tab_height + (tonumber(row.y) or 0) * ch
    if y + ch <= height + 0.5 then
      paint_row(row, cols, y, cw, ch, size, ox)
    end
  end

  -- The page converts a click back into a cell and a resize into a new grid, so
  -- it needs the numbers the view drew with, not a second copy of the arithmetic.
  -- What the frame is, not what it draws: the page converts a click into a cell
  -- and a resize into a new grid, so it needs the numbers this view drew with.
  M.meta.cols = cols
  M.meta.rows = rows
  M.meta.cell_width = cw
  M.meta.cell_height = ch
  -- The base the surface normalised against, so a renderer can turn a node's
  -- scale back into pixels: `scale * base * 1.25` is the size the view asked for.
  M.meta.base = theme.body or 16
  -- Empty means the shell set no title (OSC 0); the page keeps its own name
  -- rather than replacing it with a blank one.
  M.meta.title = type(frame.title) == "string" and frame.title or ""
  M.meta.scroll = frame.scroll and tonumber(frame.scroll.offset) or 0

  local cursor = frame.cursor
  if type(cursor) == "table" and cursor.visible ~= false then
    surface:rect("terminal-cursor", ox + (tonumber(cursor.x) or 0) * cw, tab_height + (tonumber(cursor.y) or 0) * ch,
      math.max(2, cw), ch, "primary", 0.55, 0)
  end
end

return M