-- SFX — the soundboard as a fungos.ui scene.
--
-- This is a pure projection: the host supplies the pad/rule/room snapshot and
-- routes the `hit:` targets. The view never opens a device and never invents a
-- destination.

local ui = require("fungos.ui")

local M = { name = "dev.fungos.sfx", version = "0.1.0" }

local FALLBACK_WIDTH, FALLBACK_HEIGHT = 1200, 900
local PAD = 24
local GAP = 12
local COLUMNS = 4
local TILE_H = 82
local HEADER_H = 96

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

local function label(surface, id, value, x, y, size, ink, limit)
  local text = clip(value, limit or 60)
  surface:text(id, text, x, y, size, ink or "text")
  return width_of(surface, text, size)
end

local function hit(surface, id, x, y, width, height, caption)
  if width <= 0 or height <= 0 then return end
  surface:hit(id, { x = x, y = y, width = width, height = height }, caption)
end

local function pad_detail(pad)
  if pad.available == false then return pad.reason or "unavailable" end
  if pad.duration_ms then
    return string.format("%s · %d ms", pad.category or "pad", pad.duration_ms)
  end
  return pad.category or "sample"
end

local function draw_pads(surface, snapshot, top, width)
  local pads = snapshot.pads or {}
  local tile_w = (width - PAD * 2 - GAP * (COLUMNS - 1)) / COLUMNS
  local rows = math.ceil(math.max(#pads, 1) / COLUMNS)
  surface:text("pads-label", "PADS", PAD, top, 11, "muted")
  top = top + 20

  for index, pad in ipairs(pads) do
    local column = (index - 1) % COLUMNS
    local row = math.floor((index - 1) / COLUMNS)
    local x = PAD + column * (tile_w + GAP)
    local y = top + row * (TILE_H + GAP)
    local selected = pad.selected == true
    local available = pad.available ~= false
    local ink = selected and "primary" or "surface"
    surface:rect("pad-" .. tostring(pad.id), x, y, tile_w, TILE_H, ink,
      selected and 0.95 or 0.88, 10)
    surface:rect("pad-mark-" .. tostring(pad.id), x + 12, y + 12, 6, 6,
      available and (selected and "background" or "success") or "danger", 1, 3)
    label(surface, "pad-name-" .. tostring(pad.id), pad.label or pad.id,
      x + 26, y + 11, 15, selected and "background" or "text", 22)
    label(surface, "pad-detail-" .. tostring(pad.id), pad_detail(pad),
      x + 12, y + 35, 10, selected and "background" or "muted", 32)
    if pad.room and pad.room ~= "" then
      label(surface, "pad-room-" .. tostring(pad.id), pad.room,
        x + 12, y + 55, 10, selected and "background" or "muted", 20)
    end
    if pad.key then
      local key = tostring(pad.key)
      local key_w = width_of(surface, key, 10) + 10
      surface:rect("pad-key-" .. tostring(pad.id), x + tile_w - key_w - 10, y + 10,
        key_w, 18, selected and "background" or "background", selected and 0.25 or 0.6, 4)
      label(surface, "pad-key-text-" .. tostring(pad.id), key,
        x + tile_w - key_w - 5, y + 14, 10, selected and "background" or "muted", 3)
    end
    hit(surface, "sfx-pad-" .. tostring(pad.id), x, y, tile_w, TILE_H,
      pad.label or pad.id)
  end
  return top + rows * (TILE_H + GAP)
end

local function draw_rules(surface, snapshot, top, width)
  local rules = snapshot.rules or {}
  surface:text("rules-label", "EVENT MAPPING", PAD, top, 11, "muted")
  top = top + 20
  local row_h = 28
  for index, rule in ipairs(rules) do
    local y = top + (index - 1) * row_h
    if index == 1 then
      surface:rect("rules-bg", PAD, top - 4, width - PAD * 2,
        math.max(1, #rules) * row_h + 4, "surface", 0.7, 8)
    end
    local on = rule.enabled ~= false
    local status = on and "ON" or "MUTED"
    label(surface, "rule-kind-" .. index, rule.kind, PAD + 12, y + 8, 11, "text", 34)
    label(surface, "rule-pad-" .. index, rule.pad or "?", PAD + width * 0.42, y + 8,
      11, "muted", 18)
    local status_w = width_of(surface, status, 10) + 16
    surface:rect("rule-status-" .. index, PAD + width - status_w - 12, y + 4,
      status_w, 19, on and "success" or "danger", 0.18, 4)
    label(surface, "rule-status-text-" .. index, status,
      PAD + width - status_w - 6, y + 8, 10, on and "success" or "danger", 8)
    hit(surface, "sfx-rule-" .. tostring(rule.kind), PAD + width * 0.42, y,
      width * 0.28, row_h - 2, rule.pad or rule.kind)
  end
  return top + math.max(1, #rules) * row_h
end

function M.render(snapshot)
  snapshot = snapshot or {}
  local width, height = extent()
  local surface = ui.surface(width, height, ui.theme())
  surface:background("background")

  surface:rect("header", 0, 0, width, HEADER_H, "surface", 0.55, 0)
  label(surface, "title", "SFX", PAD, 18, 30, "text", 12)
  label(surface, "subtitle", "events in, sound out", PAD, 54, 12, "muted", 48)

  local status = snapshot.status or {}
  local room = status.opened_room or status.current_room or "host default"
  local room_w = width_of(surface, room, 11)
  label(surface, "room", "room · " .. tostring(room), width - PAD - room_w - 44, 20,
    11, "muted", 40)
  label(surface, "output", status.output or "PCM", width - PAD - 80, 40, 11,
    "muted", 20)
  local counts = snapshot.counts or {}
  label(surface, "counts", string.format("%d pads · %d rules",
    counts.pads or #(snapshot.pads or {}), counts.rules or #(snapshot.rules or {})),
    width - PAD - 180, 60, 11, "muted", 30)

  local y = HEADER_H + 12
  local content_w = width - PAD * 2
  y = draw_rules(surface, snapshot, y, content_w)
  y = y + 22
  draw_pads(surface, snapshot, y, content_w)

  if status.last_error then
    label(surface, "last-error", status.last_error, PAD, height - 28, 11, "danger", 80)
  elseif status.last_pad then
    label(surface, "last-pad", "last · " .. tostring(status.last_pad),
      PAD, height - 28, 11, "muted", 40)
  end

  return surface
end

return M
