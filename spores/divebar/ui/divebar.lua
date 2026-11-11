-- Divebar: the system bar as an SDK view.
--
-- Authored at 1200x44 and authority-free: everything it draws comes from the
-- snapshot. The host decides the extent it grants and says so in
-- `module.viewport`; the fallback below is only what a host that says nothing
-- gets. The manifest declares the region's height, so a second copy here would
-- be a fact with two owners.

local ui = require("fungos.ui")

local M = { name = "dev.fungos.divebar", version = "0.1.0" }

local FALLBACK_WIDTH, FALLBACK_HEIGHT = 1200, 44
local PILL_W, PILL_H, PILL_GAP = 34, 28, 8

local function extent()
  local viewport = module and module.viewport or {}
  return tonumber(viewport.width) or FALLBACK_WIDTH,
    tonumber(viewport.height) or FALLBACK_HEIGHT
end

local function text_width(surface, value, size)
  local measured = surface:measure_text(value, size)
  if type(measured) == "table" and type(measured.width) == "number" then
    return measured.width
  end
  return #tostring(value or "") * (tonumber(size) or 10) * 0.58
end

-- Vertical centre of a text run inside a box. Text anchors top-left, so every
-- centred label needs its own baseline; guessing an offset per label is how
-- bars drift out of alignment when one string changes size.
local function centre_y(top, height, size)
  return top + (height - size * 1.2) / 2
end

-- A target the host may route input to. `soil:canvas` carries no hit targets
-- yet (the Lua ABI deliberately records none), so a view declares one as a
-- transparent rect named `hit:<id>`: the document already carries the geometry
-- it was drawn with, so the host routes input from the same numbers and the two
-- can never drift. When the ABI grows a target field this helper is what changes.
local function target(surface, id, x, y, width, height)
  surface:rect("hit:" .. id, x, y, width, height, "background", 0, 0)
end

-- Initials for the profile chip. ASCII only for now: slicing a multi-byte name
-- by byte would split a glyph, the same trap `fit` has.
local function initials(profile)
  local name = tostring(profile.name or ""):gsub("^%s+", ""):gsub("%s+$", "")
  local first, last = name:match("^(%S+)%s+(%S+)")
  if first and last then return (first:sub(1, 1) .. last:sub(1, 1)):upper() end
  if #name > 0 then return name:sub(1, 2):upper() end
  return "?"
end

local SHEET_TOP, SHEET_ROW = 10, 34

-- Classification banners. The paint is the standard's, not a design choice:
-- white on red for SECRET, black on yellow for TOP SECRET, white on blue and
-- white on green below them. The colours arrive as theme inks named by level,
-- which the *host* supplies from the instance profile -- so a personal theme
-- cannot restyle a marking, and the view never hardcodes a hue.
local BANNER_INK = {
  ["TOP SECRET"] = "marking-ts",
  ["SECRET"] = "marking-s",
  ["CONFIDENTIAL"] = "marking-c",
  ["CUI"] = "marking-cui",
  ["UNCLASSIFIED"] = "marking-u",
}

-- The banner is its own document at the extent the host grants it, the same way
-- a sheet is: one view, three extent-driven shapes. The host supplies the
-- marking string (it owns resolution); the view only decides how a level looks.
local function render_banner(snapshot, surface, WIDTH, HEIGHT)
  local marking = snapshot.marking or {}
  local level = tostring(marking.level or "UNCLASSIFIED"):upper()
  local ink = BANNER_INK[level] or BANNER_INK["UNCLASSIFIED"]
  local label = tostring(marking.label or level)
  surface:rect("banner", 0, 0, WIDTH, HEIGHT, ink, 1, 0)
  surface:rect("banner-rule", 0, HEIGHT - 1, WIDTH, 1, "border", 0.35, 0)
  local size = math.min(13, math.max(9, HEIGHT - 9))
  surface:text("banner-label", label,
    (WIDTH - text_width(surface, label, size)) / 2, centre_y(0, HEIGHT, size),
    size, ink .. "-text")
  return surface
end

-- Cut a run to the width it is allowed, with an ellipsis. A bar that lets a
-- title run under the clock looks like a clock that cannot be read.
local function fit(surface, value, size, limit)
  value = tostring(value or "")
  if text_width(surface, value, size) <= limit then return value end
  local text = value
  while #text > 1 and text_width(surface, text .. "…", size) > limit do
    text = text:sub(1, -2)
  end
  return text .. "…"
end

-- A sheet is the same package, drawn at the extent the host grants a popover:
-- the host decides where it hangs, the view decides what is in it. Rows arrive
-- from the snapshot, so the sheet never invents a destination.
local function render_sheet(snapshot, surface, WIDTH, HEIGHT)
  local rows = snapshot.rows or {}
  surface:rect("sheet", 0, 0, WIDTH, HEIGHT, "surface", 1, 12)
  surface:rect("sheet-edge", 0, HEIGHT - 1, WIDTH, 1, "border", 0.7)
  local y = SHEET_TOP
  for index, row in ipairs(rows) do
    local on = row.selected and true or false
    if on then
      surface:rect("row-on-" .. index, 6, y, WIDTH - 12, SHEET_ROW - 4, "accent", 0.16, 8)
    end
    surface:rect("row-mark-" .. index, 14, y + 5, 18, 18,
      on and "accent" or "border", on and 1 or 0.55, 5)
    local label = fit(surface, row.label, 12, WIDTH - 120)
    surface:text("row-label-" .. index, label, 42, centre_y(y, SHEET_ROW - 4, 12), 12,
      on and "text" or "muted")
    if row.note and row.note ~= "" then
      local note = tostring(row.note)
      surface:text("row-note-" .. index, note,
        WIDTH - 14 - text_width(surface, note, 10),
        centre_y(y, SHEET_ROW - 4, 10), 10, "border")
    end
    target(surface, "row:" .. tostring(row.id or index), 6, y, WIDTH - 12, SHEET_ROW - 4)
    y = y + SHEET_ROW
  end
  return surface
end

function M.render(snapshot)
  snapshot = snapshot or {}
  local WIDTH, HEIGHT = extent()
  local surface = ui.surface(WIDTH, HEIGHT, ui.theme())

  -- A popover is the same view at another extent. The host asks for it by name;
  -- the bar keeps no state, so closing a sheet cannot leave the bar lying.
  local sheet = snapshot.sheet or ""
  if sheet ~= "" and HEIGHT > 60 then
    return render_sheet(snapshot, surface, WIDTH, HEIGHT)
  end

  if snapshot.banner then
    return render_banner(snapshot, surface, WIDTH, HEIGHT)
  end

  surface:rect("ground", 0, 0, WIDTH, HEIGHT, "background", 1)
  surface:rect("edge", 0, HEIGHT - 1, WIDTH, 1, "border", 0.9)

  -- Left: the desktop switcher, then the desktops themselves. The switcher is a
  -- grid of dots rather than a glyph, so it does not depend on the font.
  local pill_top = math.floor((HEIGHT - PILL_H) / 2)
  local switcher_w = 30
  local open = snapshot.open_sheet == "desktops"
  surface:rect("switcher", 6, pill_top, switcher_w, PILL_H,
    open and "accent" or "surface", open and 1 or 0.85, 9)
  local dot, step = 4, 8
  local grid_x = 6 + (switcher_w - (dot + step)) / 2
  local grid_y = pill_top + (PILL_H - (dot + step)) / 2
  for row = 0, 1 do
    for col = 0, 1 do
      surface:rect(("switcher-%d-%d"):format(row, col),
        grid_x + col * step, grid_y + row * step, dot, dot,
        open and "background" or "muted", 1, 1)
    end
  end
  target(surface, "switcher", 6, pill_top, switcher_w, PILL_H)

  local workspaces = snapshot.workspaces or {}
  local x = 6 + switcher_w + PILL_GAP
  for index, workspace in ipairs(workspaces) do
    local active = workspace.focused and true or false
    local label = tostring(workspace.id)
    local size = 12
    surface:rect("ws-" .. index, x, pill_top, PILL_W, PILL_H,
      active and "accent" or "surface", active and 1 or 0.85, 9)
    surface:text("ws-label-" .. index, label,
      x + (PILL_W - text_width(surface, label, size)) / 2,
      centre_y(pill_top, PILL_H, size), size,
      active and "background" or "muted")
    target(surface, "ws:" .. label, x, pill_top, PILL_W, PILL_H)
    x = x + PILL_W + PILL_GAP
  end
  local left_free = x + 8

  -- Right: the person, then the ambient facts. The avatar is the anchor, so the
  -- facts pack leftwards from it and the bar still reads the same when the
  -- battery is missing.
  local profile = snapshot.profile or {}
  local avatar = 26
  local ax = WIDTH - 10 - avatar
  local ay = math.floor((HEIGHT - avatar) / 2)
  surface:rect("avatar", ax, ay, avatar, avatar, "accent", 1, avatar / 2)
  local mark = tostring(profile.initials or initials(profile))
  surface:text("avatar-initials", mark,
    ax + (avatar - text_width(surface, mark, 10)) / 2, centre_y(ay, avatar, 10),
    10, "background")
  target(surface, "avatar", ax, ay, avatar, avatar)

  local ambient = snapshot.ambient or {}
  local right = ax - 12
  local function push(id, value, size, ink)
    if not value or value == "" then return end
    local width = text_width(surface, value, size)
    surface:text(id, value, right - width, centre_y(0, HEIGHT, size), size, ink)
    right = right - width - 12
  end
  push("clock", ambient.clock, 13, "text")
  if right < ax - 12 then
    surface:rect("divider", right, pill_top, 1, PILL_H, "border", 0.8)
    right = right - 11
  end
  push("battery", ambient.battery, 11, "muted")
  push("network", ambient.network, 11, "muted")

  -- Centre: what has focus, in the space the two sides actually left. Both
  -- lines are centred as one block, so a missing app leaves the title centred
  -- rather than sitting above a gap.
  local active = snapshot.active or {}
  local gap = right - left_free
  if gap > 40 then
    local title = fit(surface, active.title, 13, gap)
    local app = fit(surface, active.app, 10, gap)
    local block = 0
    if title ~= "" then block = block + 13 * 1.2 end
    if app ~= "" then block = block + 10 * 1.2 end
    local top = (HEIGHT - block) / 2
    if title ~= "" then
      surface:text("active-title", title,
        left_free + (gap - text_width(surface, title, 13)) / 2, top, 13, "text")
      top = top + 13 * 1.2
    end
    if app ~= "" then
      surface:text("active-app", app,
        left_free + (gap - text_width(surface, app, 10)) / 2, top, 10, "muted")
    end
  end

  return surface
end

return M
