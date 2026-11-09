-- files: a VFS browser and storage census.
--
-- The snapshot arrives already scanned and already ordered: totals, a census by
-- media class, the largest files, and one directory listing. This file paints
-- it. It computes no total and re-sorts nothing — a view with a second opinion
-- about size is how two panes start disagreeing about the same disk.
--
-- Geometry: this view owns it, because Canvas has no layout service and a flow
-- engine carried inside one Spore would be a second layout implementation with
-- one caller. A masthead, a rail, a list and an inspector are arithmetic.
--
-- The library arrives through `require`, which is what the SDK package is for:
-- the composer hands this view a `require` that resolves to `fungos.ui`, so the
-- view names the library instead of receiving it as an accidental upvalue.
local ui = require("fungos.ui")

local M = {}
M.name = "files"
M.version = "0.1.0"

-- The surface size a host does not tell us. This matches the authoring size the
-- other views use; a host that passes `module.viewport` overrides it.
local W, H = 1200, 900
-- Explicit draw layers. Canvas sorts scene nodes by z and breaks ties by
-- insertion order, so "I appended it later" is how a band ends up over its text.
local Z = 0
local Z_PANEL = Z + 0.05
local Z_INK = Z + 0.10
local Z_TEXT = Z + 0.20
-- A hairline is not a density; it is the only way this view separates anything.
-- The host has no border primitive, so a separator is a one-pixel muted rect.
local HAIRLINE = 0.14
-- Canvas caps a module scene at a combined node budget. A row costs three nodes
-- (name, size, date), so the list stops at a count that leaves the chrome room.
local MAX_ROWS = 22
local MAX_ROOTS = 5
local MAX_LEGEND = 4

local surface
local TYPE = { micro = 10, caption = 12, body = 16, title = 22 }
local GAP = { xs = 4, sm = 8, md = 16 }

-- One type scale for the whole view, read from the host's own tokens. The
-- numbers in the table above are the fallback for a host that publishes none.
local function token(value, fallback)
  local number = tonumber(value)
  if number and number >= 4 and number <= 200 then return number end
  return fallback
end

local function use_type(scale)
  TYPE.micro = token(scale.micro, TYPE.micro)
  TYPE.caption = token(scale.small, TYPE.caption)
  TYPE.body = token(scale.body, TYPE.body)
  TYPE.title = token(scale.large, TYPE.title)
  GAP.xs = math.max(4, math.floor(TYPE.body / 4))
  GAP.sm = math.max(8, math.floor(TYPE.body / 2))
  GAP.md = math.max(16, math.floor(TYPE.body))
end

-- The host publishes the scale it draws with; `ui.theme()` returns the SDK's
-- profile defaults, which are a fallback, not the wall. Reading the host's own
-- tokens here is what makes a 24 px body mean 24 px: `ui.surface` divides a
-- requested size by `theme.body`, and Canvas multiplies by the module's body. A
-- view that let those two disagree draws every label at the wrong size and
-- aligns none of its right edges.
local function host_scale()
  local theme = ui.theme()
  local host = _G.module and _G.module.theme
  if type(host) == "table" then
    for _, key in ipairs({ "micro", "small", "body", "large" }) do
      local value = tonumber(host[key])
      if value and value >= 4 and value <= 200 then theme[key] = value end
    end
  end
  return theme
end

local function use_viewport() W, H = ui.viewport(W, H) end

local function surfaces()
  local profile = (ui.profile and ui.profile.surfaces) or {}
  return {
    panel = tonumber(profile.panel) or 0.36,
    selected = tonumber(profile.selected) or 0.16,
  }
end

local function line_height(size) return math.floor(size * 1.35) end

local function rect(id, x, y, w, h, ink, opacity, radius, layer)
  surface:rect(id, x, y, w, h, ink or "surface", opacity, radius or 0, layer or Z)
end

local function text(id, value, x, y, size, ink, layer)
  surface:text(id, value, x, y, size, ink or "text", layer or Z_TEXT)
end

local function measure(value, size)
  return surface:measure_text(value, size).width
end

local function clip(value, limit)
  value = tostring(value or "")
  if value == "" then return "" end
  if ui.clip then return ui.clip(value, limit) end
  if #value <= limit then return value end
  return value:sub(1, math.max(1, limit - 1)) .. "\u{2026}"
end

local function chars_per_line(width, size)
  local sample = "the quick brown fox jumps"
  local per_char = measure(sample, size) / #sample
  if per_char <= 0 then return 8 end
  return math.max(8, math.floor(width / per_char))
end
M.chars_per_line = chars_per_line

-- Units are presentation, so they live here and not in the snapshot. The
-- snapshot carries bytes, which is the only thing that cannot disagree.
local UNITS = { "B", "KB", "MB", "GB", "TB", "PB" }
local function format_bytes(bytes)
  local value = tonumber(bytes) or 0
  if value < 0 then value = 0 end
  if value < 1024 then return string.format("%d B", math.floor(value)) end
  local unit = 1
  while value >= 1024 and unit < #UNITS do
    value = value / 1024
    unit = unit + 1
  end
  if value >= 100 then return string.format("%.0f %s", value, UNITS[unit]) end
  if value >= 10 then return string.format("%.1f %s", value, UNITS[unit]) end
  return string.format("%.2f %s", value, UNITS[unit])
end
M.format_bytes = format_bytes

local function format_count(value)
  local number = tonumber(value) or 0
  local out = tostring(math.floor(number))
  local grouped = out:reverse():gsub("(%d%d%d)", "%1,"):reverse()
  return (grouped:gsub("^,", ""))
end
M.format_count = format_count

-- The snapshot formats dates, because a view has no clock and the `os`
-- library is ambient authority. An empty date is a host that did not report one.
local function date(value)
  value = tostring(value or "")
  if value == "" then return "\u{2014}" end
  return value
end

-- Pressable controls go through `widget.button`, the primitive a host
-- implements. Where there is no host, the spec is kept so the row stays
-- addressable instead of crashing the window.
local pending_presses = {}
local function press(spec)
  if type(widget) == "table" and type(widget.button) == "function" then
    widget.button(spec)
  else
    pending_presses[#pending_presses + 1] = spec
  end
end
M.pending_presses = pending_presses
M.press = press

local function hover_id()
  local hover = _G.module and _G.module.hover
  return type(hover) == "string" and hover or nil
end

-- The class palette is roles, never colours; a theme decides what they are. The
-- order is stable so the same class keeps the same band between renders.
local BAND_INK = { "primary", "info", "success", "warning", "danger" }

local function parent_of(path)
  path = tostring(path or "")
  return path:match("^(.*)/[^/]*$") or ""
end

local function crumb_text(snapshot)
  local parts = {}
  for _, crumb in ipairs(snapshot.crumbs or {}) do
    parts[#parts + 1] = tostring(crumb.name or "")
  end
  if #parts == 0 then return tostring(snapshot.workspace or "Files") end
  return table.concat(parts, "  /  ")
end

-- Where everything goes. Three panes at 980 px, the rail alone below that, and
-- a summary line instead of the inspector when there is no room for it.
local function geometry()
  local pad = GAP.md
  local mast_h = math.floor(TYPE.title * 1.7)
  local top = pad + mast_h + GAP.sm
  local bottom = H - pad
  local body_h = math.max(0, bottom - top)
  local wide = W >= 980
  local rail_w = W >= 760 and math.max(176, math.floor(W * 0.20)) or 0
  local inspect_w = wide and math.max(288, math.floor(W * 0.27)) or 0
  local rail = rail_w > 0 and { x = pad, y = top, w = rail_w, h = body_h } or nil
  local main_x = rail and (pad + rail_w + GAP.md) or pad
  local main_right = wide and (W - pad - inspect_w - GAP.md) or (W - pad)
  local box = {
    mast = { x = pad, y = pad, w = W - pad * 2, h = mast_h },
    rail = rail,
    main = { x = main_x, y = top, w = math.max(0, main_right - main_x), h = body_h },
    inspect = wide and { x = W - pad - inspect_w, y = top, w = inspect_w, h = body_h } or nil,
    wide = wide,
  }
  box.crumb = { x = box.main.x, y = box.main.y, w = box.main.w, h = line_height(TYPE.body) }
  box.head = {
    x = box.main.x,
    y = box.crumb.y + box.crumb.h + GAP.sm,
    w = box.main.w,
    h = line_height(TYPE.micro),
  }
  box.rows = { x = box.main.x, y = box.head.y + box.head.h + GAP.xs, w = box.main.w }
  if box.rail then
    box.rail_inner = {
      x = box.rail.x + GAP.sm,
      y = box.rail.y + GAP.sm,
      w = box.rail.w - GAP.sm * 2,
    }
  end
  if box.inspect then
    box.inspect_inner = {
      x = box.inspect.x + GAP.md,
      y = box.inspect.y + GAP.sm,
      w = box.inspect.w - GAP.md * 2,
    }
  end
  return box
end

local function row_pitch()
  return math.max(34, math.floor(TYPE.body * 2.0))
end
M.row_height = row_pitch()

-- How many rows the window holds. The pitch is the view's own, so the fixture
-- holds it to this number rather than a second guess.
local function plan_rows(box)
  local row_h = row_pitch()
  M.row_height = row_h
  local space = box.main.y + box.main.h - box.rows.y
  return math.max(0, math.min(MAX_ROWS, math.floor(space / row_h)))
end

local function columns(box)
  local size_w = measure("000.0 MB", TYPE.caption) + GAP.sm
  local date_w = measure("0000-00-00", TYPE.micro) + GAP.md
  local name_w = math.max(60, box.main.w - size_w - date_w)
  -- Left edges, not right edges: the last column starts where the size column
  -- ends, so `x + w` is the list's right edge exactly. Adding the date width a
  -- second time put the header 103 px past the window at 760 px wide.
  return {
    name = { x = box.main.x, w = name_w },
    size = { x = box.main.x + name_w, w = size_w },
    date = { x = box.main.x + name_w + size_w, w = date_w },
  }
end

local function right(id, value, right_x, y, size, ink)
  text(id, value, right_x - measure(value, size), y, size, ink)
end

local function draw_masthead(snapshot, box, totals)
  text("mast-title", tostring(snapshot.title or "files"), box.mast.x,
    box.mast.y + math.floor((box.mast.h - TYPE.title) / 2), TYPE.title, "primary")
  local summary = string.format("%s   %s files   %s folders",
    format_bytes(totals.bytes), format_count(totals.files), format_count(totals.folders))
  if box.wide then
    -- The summary's right edge is the inspector's text edge, not the window's:
    -- two right-aligned columns of numbers that end 16 px apart read as a
    -- mistake.
    right("mast-summary", summary, box.mast.x + box.mast.w - GAP.md,
      box.mast.y + math.floor((box.mast.h - TYPE.caption) / 2), TYPE.caption, "muted")
  end
  rect("mast-rule", box.mast.x, box.mast.y + box.mast.h, box.mast.w, 1, "muted",
    HAIRLINE, 0, Z_PANEL)
end

local function draw_rail(snapshot, box, tone)
  if not box.rail then return end
  rect("rail-panel", box.rail.x, box.rail.y, box.rail.w, box.rail.h, "surface",
    tone.panel, 8, Z_PANEL)
  text("rail-label", "LOCATIONS", box.rail_inner.x, box.rail_inner.y, TYPE.micro, "muted")
  -- Three lines per location: the name, its size, and how many files that size
  -- is made of. "How big is this shared desktop" and "how many files is that"
  -- are the two questions the rail exists to answer without opening anything.
  local row_h = math.max(56, math.floor(TYPE.body * 3.4))
  local y = box.rail_inner.y + line_height(TYPE.micro) + GAP.sm
  local hovered = hover_id()
  for index, root in ipairs(snapshot.roots or {}) do
    if index > MAX_ROOTS then break end
    local row = { x = box.rail_inner.x, y = y, w = box.rail_inner.w, h = row_h }
    local selected = root.current == true
    if selected then
      rect("root-" .. index .. "-band", row.x, row.y, row.w, row.h, "primary",
        tone.selected, 6, Z_PANEL)
    elseif hovered == "root-" .. index then
      rect("root-" .. index .. "-hover", row.x, row.y, row.w, row.h, "primary",
        tone.selected * 0.5, 6, Z_PANEL)
    end
    local room = chars_per_line(row.w - GAP.md, TYPE.caption)
    text("root-" .. index .. "-name", clip(root.name, room), row.x + GAP.sm, row.y + GAP.sm,
      TYPE.caption, selected and "primary" or "text")
    -- A location whose first scan is still running is not an empty location.
    -- Say which one it is; "0 B" is a claim the host has not made.
    local available = root.available ~= false
    text("root-" .. index .. "-size",
      clip(available and format_bytes(root.bytes) or "\u{2014}", room), row.x + GAP.sm,
      row.y + GAP.sm + line_height(TYPE.caption), TYPE.micro, "muted")
    -- A count is a claim. Say when a location is only partly counted, and when
    -- it could not be read at all — a denied folder is not an empty one.
    local errors = tonumber(root.errors) or 0
    local files_label = "scanning"
    local files_ink = "muted"
    if available then
      files_label = string.format("%s files", format_count(root.files))
      if errors > 0 or root.truncated then
        if (tonumber(root.files) or 0) == 0 and errors > 0 then
          files_label = "unreadable"
        else
          files_label = string.format("%s files \u{00b7} partial", format_count(root.files))
        end
        files_ink = "warning"
      end
    end
    text("root-" .. index .. "-files", clip(files_label, room), row.x + GAP.sm,
      row.y + GAP.sm + line_height(TYPE.caption) + line_height(TYPE.micro), TYPE.micro, files_ink)
    press {
      id = "root-" .. index,
      label = tostring(root.name or ""),
      action = "files-open",
      body = { root = tostring(root.id or ""), path = "" },
      x = surface:nx(row.x), y = surface:ny(row.y),
      width = surface:sw(row.w), height = surface:sh(row.h),
    }
    y = y + row_h + GAP.xs
  end
  text("rail-foot", "files.myc", box.rail_inner.x,
    box.rail.y + box.rail.h - line_height(TYPE.micro) - GAP.sm, TYPE.micro, "muted")
end

local function draw_crumbs(snapshot, box)
  local crumbs = crumb_text(snapshot)
  local room = chars_per_line(box.crumb.w, TYPE.body)
  text("crumb", clip(crumbs, room), box.crumb.x, box.crumb.y, TYPE.body, "text")
  local path = tostring(snapshot.path or "")
  if path ~= "" then
    -- The whole crumb line is the way up: a breadcrumb that cannot be pressed is
    -- decoration, and an invisible hit region costs no node.
    press {
      id = "crumb-up",
      label = "up",
      action = "files-up",
      body = { root = tostring(snapshot.root or ""), path = parent_of(path) },
      x = surface:nx(box.crumb.x), y = surface:ny(box.crumb.y),
      width = surface:sw(math.min(box.crumb.w, measure(crumbs, TYPE.body))),
      height = surface:sh(box.crumb.h),
    }
  end
end

local function draw_columns(box)
  local col = columns(box)
  text("col-name", "NAME", col.name.x, box.head.y, TYPE.micro, "muted")
  right("col-size", "SIZE", col.size.x + col.size.w - GAP.sm, box.head.y, TYPE.micro, "muted")
  right("col-date", "MODIFIED", col.date.x + col.date.w, box.head.y, TYPE.micro, "muted")
end

local function draw_rows(snapshot, box, shown, tone)
  local listing = snapshot.listing or {}
  local col = columns(box)
  local row_h = row_pitch()
  local y = box.rows.y
  local hovered = hover_id()
  local drawn = 0
  local name_room = chars_per_line(col.name.w - GAP.sm, TYPE.caption)
  for index = 1, math.min(#listing, shown) do
    local entry = listing[index]
    local row = { x = box.rows.x, y = y, w = box.rows.w, h = row_h }
    if row.y + row.h <= box.main.y + box.main.h + 0.5 then
      if hovered == "row-" .. index then
        rect("row-" .. index .. "-hover", row.x, row.y, row.w, row.h, "primary",
          tone.selected * 0.5, 5, Z_PANEL)
      end
      local directory = entry.kind == "directory"
      local label = tostring(entry.name or "")
      if directory then label = label .. "/" end
      text("row-" .. index .. "-name", clip(label, name_room), col.name.x,
        row.y + math.floor((row.h - TYPE.caption) / 2), TYPE.caption,
        directory and "primary" or "text")
      local size = directory and "\u{2014}" or format_bytes(entry.bytes)
      right("row-" .. index .. "-size", clip(size, 12), col.size.x + col.size.w - GAP.sm,
        row.y + math.floor((row.h - TYPE.micro) / 2), TYPE.micro, "muted")
      right("row-" .. index .. "-date", clip(date(entry.modified), 12), col.date.x + col.date.w,
        row.y + math.floor((row.h - TYPE.micro) / 2), TYPE.micro, "muted")
      if directory then
        press {
          id = "row-" .. index,
          label = tostring(entry.path or ""),
          action = "files-open",
          body = { root = tostring(snapshot.root or ""), path = tostring(entry.path or "") },
          x = surface:nx(row.x), y = surface:ny(row.y),
          width = surface:sw(row.w), height = surface:sh(row.h),
        }
      end
      drawn = drawn + 1
      y = y + row_h
    end
  end
  M.rows_drawn = drawn
  if #listing == 0 then
    text("empty", "This folder is empty.", box.rows.x, box.rows.y, TYPE.body, "muted")
  elseif snapshot.listing_truncated or drawn < #listing then
    local total = tonumber(snapshot.listing_total) or #listing
    text("rows-note", string.format("showing %s of %s", format_count(drawn), format_count(total)),
      box.rows.x, box.rows.y + drawn * row_h + GAP.xs, TYPE.micro, "muted")
  end
end

local function draw_bands(snapshot, box, totals, tone)
  local inner = box.inspect_inner
  local y = inner.y
  text("inspect-label", "STORAGE", inner.x, y, TYPE.micro, "muted")
  y = y + line_height(TYPE.micro) + GAP.sm
  text("inspect-total", format_bytes(totals.bytes), inner.x, y, TYPE.title, "primary")
  y = y + line_height(TYPE.title) + GAP.xs
  text("inspect-counts", string.format("%s files", format_count(totals.files)), inner.x, y,
    TYPE.caption, "muted")
  text("inspect-folders", string.format("%s folders", format_count(totals.folders)), inner.x,
    y + line_height(TYPE.caption), TYPE.caption, "muted")
  y = y + line_height(TYPE.caption) * 2 + GAP.md

  local kinds = snapshot.kinds or {}
  if #kinds > 0 and (tonumber(totals.bytes) or 0) > 0 then
    -- One stacked bar is the whole census in a glance; the legend is the same
    -- numbers spelled out. A band is only drawn when it has bytes, so an empty
    -- class never steals width from a real one.
    local bar_h = math.max(8, math.floor(TYPE.micro * 0.8))
    rect("kind-bar-track", inner.x, y, inner.w, bar_h, "surface", 0.55, bar_h / 2, Z_PANEL)
    local x = inner.x
    local total = tonumber(totals.bytes) or 0
    for index, kind in ipairs(kinds) do
      local bytes = tonumber(kind.bytes) or 0
      if bytes > 0 then
        local share = bytes / total
        local width = math.max(2, math.floor(inner.w * share))
        -- The last class takes the rounding remainder so the strip spans the
        -- track exactly; a bar with a dark sliver at its end reads as a class
        -- that was left out.
        if index == #kinds or x + width > inner.x + inner.w then
          width = math.max(2, inner.x + inner.w - x)
        end
        rect("kind-band-" .. index, x, y, width, bar_h, BAND_INK[(index - 1) % #BAND_INK + 1] or "muted",
          0.9, math.min(bar_h / 2, 3), Z_INK)
        x = x + width
      end
    end
    y = y + bar_h + GAP.md

    local legend_y = y
    local shown = 0
    for index, kind in ipairs(kinds) do
      if shown >= MAX_LEGEND then break end
      local bytes = tonumber(kind.bytes) or 0
      if bytes > 0 then
        local ink = BAND_INK[shown + 1] or "muted"
        rect("legend-" .. index .. "-swatch", inner.x, legend_y + math.floor(line_height(TYPE.micro) / 2) - 4,
          8, 8, ink, 0.9, 2, Z_INK)
        text("legend-" .. index, clip(tostring(kind.label or ""), 16), inner.x + GAP.md, legend_y,
          TYPE.caption, "text")
        right("legend-" .. index .. "-size", format_bytes(bytes), inner.x + inner.w, legend_y,
          TYPE.micro, "muted")
        legend_y = legend_y + line_height(TYPE.caption) + GAP.xs
        shown = shown + 1
      end
    end
    y = legend_y + GAP.sm
  end

  local largest = snapshot.largest or {}
  if #largest > 0 then
    text("largest-label", "LARGEST FILES", inner.x, y, TYPE.micro, "muted")
    y = y + line_height(TYPE.micro) + GAP.sm
    local size_w = measure("000.0 MB", TYPE.micro) + GAP.sm
    local name_room = chars_per_line(inner.w - size_w, TYPE.caption)
    for index, file in ipairs(largest) do
      if index > 3 then break end
      text("largest-" .. index, clip(tostring(file.name or ""), name_room), inner.x, y,
        TYPE.caption, "text")
      right("largest-" .. index .. "-size", format_bytes(file.bytes), inner.x + inner.w, y,
        TYPE.micro, "muted")
      y = y + line_height(TYPE.caption) + GAP.xs
    end
    y = y + GAP.sm
  end

  -- Two facts, two lines: a partial count and a failed subtree are different
  -- statements, and a footer that picks one of them hides the other.
  local errors = tonumber(totals.errors) or 0
  local foot_y = box.inspect.y + box.inspect.h - line_height(TYPE.micro) - GAP.sm
  local note
  if snapshot.error then
    note = clip(tostring(snapshot.error), chars_per_line(inner.w, TYPE.micro))
  elseif totals.truncated then
    note = string.format("partial: stopped at %s entries", format_count(totals.scanned))
  else
    note = string.format("scanned %s entries", format_count(totals.scanned))
  end
  text("inspect-foot", note, inner.x, foot_y - (errors > 0 and line_height(TYPE.micro) or 0),
    TYPE.micro, (snapshot.error or totals.truncated) and "warning" or "muted")
  if errors > 0 and not snapshot.error then
    text("inspect-errors", string.format("%s %s unreadable", format_count(errors),
      errors == 1 and "folder" or "folders"), inner.x, foot_y, TYPE.micro, "warning")
  end
end

local function draw_narrow_summary(snapshot, box, totals)
  if box.wide then return end
  local text_ = string.format("%s   %s files   %s folders", format_bytes(totals.bytes),
    format_count(totals.files), format_count(totals.folders))
  right("mast-summary", clip(text_, chars_per_line(box.mast.w * 0.5, TYPE.caption)),
    box.mast.x + box.mast.w, box.mast.y + math.floor((box.mast.h - TYPE.caption) / 2),
    TYPE.caption, "muted")
end

function M.render(snapshot)
  use_viewport()
  local scale = host_scale()
  surface = ui.surface(W, H, scale)
  use_type(scale)
  snapshot = snapshot or {}
  local tone = surfaces()
  local totals = snapshot.totals or {}
  local box = geometry()
  local shown = plan_rows(box)

  rect("files-bg", 0, 0, W, H, "background", 1.0, 0)
  draw_masthead(snapshot, box, totals)
  draw_narrow_summary(snapshot, box, totals)
  draw_rail(snapshot, box, tone)
  if box.rail then
    rect("rail-rule", box.rail.x + box.rail.w + math.floor(GAP.md / 2) - 0.5, box.main.y,
      1, box.main.h, "muted", HAIRLINE, 0, Z_PANEL)
  end
  if box.inspect then
    rect("inspect-rule", box.inspect.x - math.floor(GAP.md / 2), box.inspect.y,
      1, box.inspect.h, "muted", HAIRLINE, 0, Z_PANEL)
  end

  draw_crumbs(snapshot, box)
  draw_columns(box)
  if snapshot.error then
    text("error", clip(tostring(snapshot.error), chars_per_line(box.main.w, TYPE.body)),
      box.rows.x, box.rows.y, TYPE.body, "danger")
    M.rows_drawn = 0
  else
    draw_rows(snapshot, box, shown, tone)
  end
  if box.inspect then
    draw_bands(snapshot, box, totals, tone)
  end

  M.wide = box.wide
  M.rows_planned = shown
end

return M
