-- SFX · the minimal pad surface.
--
-- Same snapshot and same `sfx-pad-<id>` hit ids as `board.lua`, but only the
-- pads. Point `[ui].entry` here when a host wants the shortest path from a
-- surface to a sound.

local ui = require("fungos.ui")

local M = { name = "dev.fungos.sfx/pads", version = "0.1.0" }

local FALLBACK_WIDTH, FALLBACK_HEIGHT = 1200, 900
local PAD = 24
local GAP = 12
local COLUMNS = 4
local TILE_H = 84

local function extent()
  local viewport = module and module.viewport or {}
  return tonumber(viewport.width) or FALLBACK_WIDTH,
    tonumber(viewport.height) or FALLBACK_HEIGHT
end

local function clip(value, limit)
  value = tostring(value or "")
  if #value <= limit then return value end
  return value:sub(1, limit - 1) .. "…"
end

local function width_of(surface, value, size)
  local measured = surface:measure_text(value, size)
  if type(measured) == "table" and type(measured.width) == "number" then
    return measured.width
  end
  return #tostring(value or "") * size * 0.58
end

local function detail(pad)
  if pad.available == false then return pad.reason or "unavailable" end
  if pad.duration_ms then
    return string.format("%s · %d ms", pad.category or "pad", pad.duration_ms)
  end
  return pad.category or "sample"
end

function M.render(snapshot)
  snapshot = snapshot or {}
  local width, height = extent()
  local surface = ui.surface(width, height, ui.theme())
  surface:background("background")

  surface:text("title", "SFX", PAD, 18, 30, "text")
  local status = snapshot.status or {}
  local room = status.opened_room or status.current_room or "host default"
  surface:text("room", "tap a pad · " .. tostring(room), PAD, 56, 12, "muted")

  local pads = snapshot.pads or {}
  local tile_w = (width - PAD * 2 - GAP * (COLUMNS - 1)) / COLUMNS
  local top = 92
  for index, pad in ipairs(pads) do
    local column = (index - 1) % COLUMNS
    local row = math.floor((index - 1) / COLUMNS)
    local x = PAD + column * (tile_w + GAP)
    local y = top + row * (TILE_H + GAP)
    local selected = pad.selected == true
    local available = pad.available ~= false
    surface:rect("pad-" .. tostring(pad.id), x, y, tile_w, TILE_H,
      selected and "primary" or "surface", selected and 0.95 or 0.88, 10)
    surface:rect("pad-mark-" .. tostring(pad.id), x + 12, y + 12, 6, 6,
      available and (selected and "background" or "success") or "danger", 1, 3)
    surface:text("pad-name-" .. tostring(pad.id), clip(pad.label or pad.id, 22),
      x + 26, y + 11, 15, selected and "background" or "text")
    surface:text("pad-detail-" .. tostring(pad.id), clip(detail(pad), 30),
      x + 12, y + 38, 10, selected and "background" or "muted")
    if pad.key then
      local key = tostring(pad.key)
      local key_w = width_of(surface, key, 10) + 12
      surface:rect("pad-key-" .. tostring(pad.id), x + tile_w - key_w - 10, y + 10,
        key_w, 18, "background", selected and 0.25 or 0.6, 4)
      surface:text("pad-key-text-" .. tostring(pad.id), key,
        x + tile_w - key_w - 4, y + 14, 10, selected and "background" or "muted")
    end
    surface:hit("sfx-pad-" .. tostring(pad.id), {
      x = x, y = y, width = tile_w, height = TILE_H,
    }, pad.label or pad.id)
  end

  if status.last_error then
    surface:text("last", clip(status.last_error, 80), PAD, height - 28, 11, "danger")
  elseif status.last_pad then
    surface:text("last", "last · " .. tostring(status.last_pad),
      PAD, height - 28, 11, "muted")
  end
  return surface
end

return M
