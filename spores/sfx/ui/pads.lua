-- SFX · a minimal pad surface.
--
-- This is the small UI: twelve (or however many) pads, a room line, and the
-- last result. It is deliberately separate from `board.lua`, which carries the
-- event mapping and the binding workflow. Point `[ui].entry` at this file when
-- you want the shortest path from a surface to a sound.

local ui = require("fungos.ui")

local M = { name = "dev.fungos.sfx/pads", version = "0.1.0" }

local function clip(value, limit)
  value = tostring(value or "")
  if #value <= limit then return value end
  return value:sub(1, limit - 1) .. "…"
end

local function detail(pad)
  if pad.available == false then return clip(pad.reason or "unavailable", 24) end
  if pad.duration_ms then return string.format("%s · %d ms", pad.category or "pad", pad.duration_ms) end
  return pad.category or "pad"
end

function M.render(snapshot)
  local pads = snapshot.pads or {}
  local rows = {}
  local columns = 4
  for start = 1, math.max(1, #pads), columns do
    local children = {}
    for index = start, math.min(start + columns - 1, #pads) do
      local pad = pads[index]
      children[#children + 1] = ui.tile(pad.label or pad.id, {
        id = "sfx-pad-" .. tostring(pad.id),
        detail = detail(pad),
        meta = pad.key,
        selected = pad.selected == true,
        enabled = pad.available ~= false,
        action = ui.action("sfx-trigger", {
          action = "sfx.trigger", pad = pad.id, room = pad.room,
        }),
        h = 92,
      })
    end
    for _ = #children + 1, columns do
      children[#children + 1] = ui.spacer({ id = "sfx-fill-" .. start .. "-" .. _, size = "none" })
    end
    local widths = {}
    for index = 1, #children do widths[index] = 1 / #children end
    rows[#rows + 1] = ui.row({
      id = "sfx-pad-row-" .. #rows + 1,
      widths = widths,
      gap = "sm",
      children = children,
    })
  end

  local status = snapshot.status or {}
  local room = status.opened_room or status.current_room or "host default"
  local theme = snapshot.theme or (_G.module and _G.module.theme) or {
    title = 28, large = 22, body = 16, small = 13, micro = 10, radius = 10,
  }
  local body = {
    ui.text("SFX", { id = "sfx-title", style = "title", ink = "text" }),
    ui.text("tap a pad · " .. tostring(room), {
      id = "sfx-room-now", style = "body", ink = "muted",
    }),
  }
  for _, row in ipairs(rows) do body[#body + 1] = row end
  body[#body + 1] = ui.text(status.last_error or (status.last_pad and ("last: " .. status.last_pad)) or "ready", {
    id = "sfx-last", style = "caption", ink = status.last_error and "danger" or "muted",
  })
  ui.render(ui.column({ id = "sfx-root", gap = "md", pad = "md", children = body }), theme)
end

return M
