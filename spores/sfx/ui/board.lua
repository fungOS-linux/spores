-- SFX · the board.
--
-- A pure function of the snapshot. Pads are tiles (the whole tile is the hit
-- region), rules are the event mapping, and the side panel says which room the
-- last sound actually reached. The view does not know what a VFS is, what an
-- MP3 is, or how a receiver is addressed; it names intents and paints facts.

local ui = require("fungos.ui")

local M = { name = "dev.fungos.sfx", version = "0.1.0" }

local function viewport()
  local vp = (_G.module and _G.module.viewport)
    or (_G.module and _G.module.inputs)
    or (_G.module and _G.module.theme and _G.module.theme.viewport)
  local width = vp and vp.width or 1200
  local height = vp and vp.height or 900
  return math.max(320, width), math.max(320, height)
end

local function theme(snapshot)
  return snapshot.theme or (_G.module and _G.module.theme) or {
    title = 28, large = 22, body = 16, small = 13, micro = 10, radius = 10,
  }
end

local function clip(value, limit)
  value = tostring(value or "")
  if #value <= limit then return value end
  return value:sub(1, limit - 1) .. "…"
end

local function detail_for(pad)
  if pad.available == false then
    return clip(pad.reason or "unavailable", 28)
  end
  local category = tostring(pad.category or "pad")
  if pad.duration_ms then
    return string.format("%s · %d ms", category, pad.duration_ms)
  end
  return category
end

local function pad_tile(pad)
  return ui.tile(pad.label or pad.id, {
    id = "sfx-pad-" .. tostring(pad.id),
    detail = detail_for(pad),
    meta = pad.key,
    selected = pad.selected == true,
    enabled = pad.available ~= false,
    action = ui.action("sfx-trigger", {
      action = "sfx.trigger", pad = pad.id, room = pad.room,
    }),
    h = 88,
  })
end

local function pad_grid(pads)
  if #pads == 0 then
    return { ui.text("No pads. Add a built-in or a sample to the mapping.", {
      id = "sfx-pads-empty", style = "body", ink = "muted",
    }) }
  end
  local rows = {}
  local columns = 4
  for start = 1, math.max(1, #pads), columns do
    local children = {}
    for index = start, math.min(start + columns - 1, #pads) do
      children[#children + 1] = pad_tile(pads[index])
    end
    -- Keep the last row the same tile width as the rest. A single full-width
    -- pad reads as a different kind of control.
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
  return rows
end

local function rule_row(rule, index, armed)
  local id = "sfx-rule-" .. index
  local quiet = not rule.enabled
  return ui.row({
    id = id,
    widths = { 0.46, 0.26, 0.14, 0.14 },
    gap = "sm",
    children = {
      ui.button(clip(rule.kind, 34), {
        id = id .. "-bind",
        small = true,
        quiet = not armed,
        action = ui.action("sfx-bind", { action = "sfx.bind", kind = rule.kind }),
      }),
      ui.text((rule.label or rule.pad or "?") .. (rule.available == false and "  (missing)" or ""), {
        id = id .. "-pad",
        style = "body",
        ink = rule.available == false and "muted" or "text",
      }),
      ui.button("TEST", {
        id = id .. "-test",
        small = true,
        quiet = true,
        action = ui.action("sfx-test", { action = "sfx.test", kind = rule.kind }),
      }),
      ui.button(quiet and "ON" or "MUTE", {
        id = id .. "-toggle",
        small = true,
        quiet = quiet,
        action = ui.action("sfx-enabled", {
          action = "sfx.enabled", kind = rule.kind, enabled = quiet,
        }),
      }),
    },
  })
end

local function rules(snapshot, armed)
  local rows = {
    ui.row({
      id = "sfx-rules-head",
      children = {
        ui.text("EVENT MAPPING", { id = "sfx-rules-label", style = "caption", ink = "muted" }),
      },
    }),
  }
  local count = #(snapshot.rules or {})
  if count == 0 then
    rows[#rows + 1] = ui.text(
      "No events are mapped. Bind a kind to a pad, or add one in appdata.",
      { id = "sfx-rules-empty", style = "body", ink = "muted" })
  end
  for index, rule in ipairs(snapshot.rules or {}) do
    rows[#rows + 1] = rule_row(rule, index, armed == rule.kind)
  end
  return ui.column({ id = "sfx-rules", gap = "xs", children = rows })
end

local function rooms(snapshot, status)
  local children = {
    ui.text("ROOMS", { id = "sfx-rooms-label", style = "caption", ink = "muted" }),
  }
  local list = snapshot.rooms or {}
  if #list == 0 then
    children[#children + 1] = ui.text("No rooms reported by the host.", {
      id = "sfx-rooms-empty", style = "caption", ink = "muted",
    })
  end
  for index, room in ipairs(list) do
    local marker = room.current and " · now" or (room.live and " · live" or " · silent")
    children[#children + 1] = ui.button(clip(room.label or room.name, 22) .. marker, {
      id = "sfx-room-" .. index,
      small = true,
      quiet = not room.current,
      action = ui.action("sfx-room", { action = "sfx.room", room = room.name }),
    })
  end
  return ui.column({ id = "sfx-rooms", gap = "xs", children = children })
end

local function side(snapshot, status, mapping)
  local children = {
    ui.text("OUTPUT", { id = "sfx-output-label", style = "caption", ink = "muted" }),
    ui.text(status.output or "48 kHz stereo", { id = "sfx-output", style = "body", ink = "text" }),
    ui.text("room: " .. tostring(status.opened_room or status.current_room or "unrouted"), {
      id = "sfx-room-now", style = "caption", ink = "muted",
    }),
    ui.text("last: " .. tostring(status.last_pad or "—") .. (status.last_kind and ("  ←  " .. status.last_kind) or ""), {
      id = "sfx-last", style = "caption", ink = "muted",
    }),
    ui.button("STOP ALL", {
      id = "sfx-stop",
      small = true,
      quiet = true,
      action = ui.action("sfx-stop", { action = "sfx.stop" }),
    }),
    ui.rule({ id = "sfx-side-rule" }),
    rooms(snapshot, status),
    ui.rule({ id = "sfx-side-rule-2" }),
    ui.text("MAPPING", { id = "sfx-mapping-label", style = "caption", ink = "muted" }),
    ui.text(string.format("%s · generation %s · gain %.2f",
      tostring(mapping.scope or "memory"),
      tostring(mapping.generation or "—"),
      tonumber(mapping.gain) or 0), {
      id = "sfx-mapping-meta", style = "caption", ink = "muted",
    }),
  }
  if mapping.error then
    children[#children + 1] = ui.text(clip(mapping.error, 120), {
      id = "sfx-mapping-error", style = "caption", ink = "danger",
    })
  end
  local issues = snapshot.issues or {}
  for index = 1, math.min(#issues, 3) do
    children[#children + 1] = ui.text(clip(issues[index].message, 96), {
      id = "sfx-issue-" .. index, style = "caption", ink = "warning",
    })
  end
  return ui.card({ id = "sfx-side", pad = "md", gap = "xs", children = children })
end

local function banner(snapshot, status)
  if not status.bind then return nil end
  return ui.bar({
    id = "sfx-bind-bar",
    pad = "sm",
    ink = "primary",
    opacity = 0.18,
    border = true,
    children = {
      ui.row({
        widths = { 0.82, 0.18 },
        children = {
          ui.text("BINDING " .. status.bind .. " — press a pad", {
            id = "sfx-bind-label", style = "body", ink = "text",
          }),
          ui.button("CANCEL", {
            id = "sfx-bind-cancel",
            small = true,
            quiet = true,
            action = ui.action("sfx-cancel-bind", { action = "sfx.cancel_bind" }),
          }),
        },
      }),
    },
  })
end

local function alerts(snapshot, status)
  local out = {}
  if status.audio_error then
    out[#out + 1] = ui.text("audio: " .. clip(status.audio_error, 120), {
      id = "sfx-audio-error", style = "caption", ink = "danger",
    })
  end
  if status.last_error then
    out[#out + 1] = ui.text("last: " .. clip(status.last_error, 120), {
      id = "sfx-last-error", style = "caption", ink = "warning",
    })
  end
  return out
end

function M.render(snapshot)
  snapshot = snapshot or {}
  local width, height = viewport()
  local wide = width >= 760
  local status = snapshot.status or {}
  local mapping = snapshot.mapping or {}
  local counts = snapshot.counts or {}
  local bind_bar = banner(snapshot, status)

  local body = {
    ui.row({
      id = "sfx-head",
      widths = { 0.72, 0.28 },
      gap = "sm",
      children = {
        ui.column({
          id = "sfx-heading",
          gap = "none",
          children = {
            ui.text("SFX", { id = "sfx-title", style = "title", ink = "text" }),
            ui.text("events in, sound out · the room decides who hears it", {
              id = "sfx-subtitle", style = "caption", ink = "muted",
            }),
          },
        }),
        ui.text(string.format("%d pads · %d rules · %d live",
          counts.pads or 0, counts.enabled or 0, counts.live_rooms or 0), {
          id = "sfx-counts", style = "caption", ink = "muted",
        }),
      },
    }),
  }
  if bind_bar then body[#body + 1] = bind_bar end
  for _, alert in ipairs(alerts(snapshot, status)) do body[#body + 1] = alert end
  body[#body + 1] = rules(snapshot, status.bind)
  body[#body + 1] = ui.rule({ id = "sfx-rules-rule" })
  body[#body + 1] = ui.text("PADS", { id = "sfx-pads-label", style = "caption", ink = "muted" })
  for _, row in ipairs(pad_grid(snapshot.pads or {})) do body[#body + 1] = row end

  local children = {
    ui.column({ id = "sfx-main", gap = "sm", grow = true, children = body }),
  }
  local widths
  if wide then
    children[#children + 1] = side(snapshot, status, mapping)
    widths = { 0.68, 0.32 }
  else
    children[#children + 1] = ui.text(
      "room: " .. tostring(status.opened_room or status.current_room or "unrouted"),
      { id = "sfx-footer-room", style = "caption", ink = "muted" })
    widths = { 1.0 }
  end

  ui.render(
    ui.column({
      id = "sfx-root",
      -- The host viewport is arbitrary pixels; the layout token rule is a
      -- 4px grid. Round the root to the grid instead of letting a real window
      -- height (1182×1902 here) fail the module at render time.
      h = math.max(4, math.floor(height / 4) * 4),
      gap = "md",
      pad = "md",
      children = { ui.row({ id = "sfx-body", widths = widths, gap = "md", children = children }) },
    }),
    theme(snapshot))
end

return M
