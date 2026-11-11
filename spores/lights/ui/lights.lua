-- Lights: room-first Hue control.
--
-- The view is a projection. The host supplies the bridge snapshot and owns every
-- action; selecting a room, toggling a light, or recalling a scene is an intent
-- (`lights.*`) that the host turns into a Geode-backed Hue request. No key,
-- bridge address, socket, or credential ever reaches this file.

local ui = require("fungos.ui")

local M = { name = "dev.fungos.lights", version = "0.1.0" }

local DEFAULT_THEME = {
  micro = 10, small = 12, body = 15, large = 20, title = 26, display = 30,
  radius = 10,
}

local function theme()
  return (_G.module and _G.module.theme) or DEFAULT_THEME
end

local function label(value, style, ink)
  return ui.text(tostring(value or ""), { style = style or "caption", ink = ink or "muted" })
end

local function action_button(text, action, payload, props)
  props = props or {}
  props.action = ui.action(action, payload or {})
  return ui.button(text, props)
end

local function percent(bri)
  return math.max(0, math.min(100, math.floor((tonumber(bri) or 0) * 100 / 254 + 0.5)))
end

local function meter(value)
  local filled = math.max(0, math.min(10, math.floor(value / 10 + 0.5)))
  return string.rep("■", filled) .. string.rep("□", 10 - filled)
end

local function room_by_id(view, id)
  for _, room in ipairs(view.rooms or {}) do
    if tostring(room.id) == tostring(id) then return room end
  end
  return (view.rooms or {})[1]
end

local function lights_in(view, room)
  local wanted = {}
  for _, id in ipairs(room.lights or {}) do wanted[tostring(id)] = true end
  local list = {}
  for _, light in ipairs(view.lights or {}) do
    if wanted[tostring(light.id)] then list[#list + 1] = light end
  end
  return list
end

local function scenes_in(view, room)
  local list = {}
  for _, scene in ipairs(view.scenes or {}) do
    if tostring(scene.group_id) == tostring(room.id) then list[#list + 1] = scene end
  end
  return list
end

local function all_on(view)
  local count = 0
  for _, light in ipairs(view.lights or {}) do
    if light.on == true then count = count + 1 end
  end
  return count
end

local function header(view, selected)
  local total = #(view.lights or {})
  local on = all_on(view)
  local summary = string.format("%s · %d lights · %d on",
    (view.bridge and view.bridge.name) or "Hue Bridge", total, on)
  return ui.row({
    id = "lights-header",
    widths = { 0.62, 0.38 },
    gap = "md",
    children = {
      ui.column({ gap = "xs", children = {
        ui.text("Lights", { style = "title", ink = "primary" }),
        label(summary),
      } }),
      ui.row({ align = "end", gap = "sm", children = {
        label("●  BRIDGE ONLINE", "caption", "success"),
        action_button("All off", "lights.all_off", {}, { quiet = true }),
      } }),
    },
  })
end

local function rooms_table(view, selected)
  local rows = {}
  for _, room in ipairs(view.rooms or {}) do
    rows[#rows + 1] = {
      id = tostring(room.id),
      name = room.name or "room",
      state = string.format("%d/%d", tonumber(room.on) or 0, tonumber(room.total) or 0),
      mark = tostring(room.id) == tostring(selected.id) and "●" or " ",
    }
  end
  return ui.table({
    id = "lights-rooms",
    title = "ROOMS",
    columns = {
      { key = "name", label = "Room", width = 2.4 },
      { key = "state", label = "On", width = 0.8, align = "right" },
      { key = "mark", label = "", width = 0.3, align = "right", ink = "warning" },
    },
    rows = rows,
    row_h = 36,
    row_action = "lights.room",
    row_key = "id",
  })
end

local function bridge_card(view)
  local bridge = view.bridge or {}
  return ui.card({
    id = "lights-bridge",
    pad = "md",
    gap = "sm",
    children = {
      label("BRIDGE", "subheading", "muted"),
      ui.row({ gap = "sm", children = {
        label("●", "caption", "success"),
        ui.text(bridge.name or "Hue Bridge", { style = "heading", ink = "primary" }),
      } }),
      label(string.format("LAN · %s · %d lights", bridge.model or "BSB002", tonumber(bridge.lights) or #(view.lights or {}))),
      label("geode://home/hue/bridge", "caption", "muted"),
    },
  })
end

local function room_panel(view, selected)
  local on = tonumber(selected.on) or 0
  local total = tonumber(selected.total) or #lights_in(view, selected)
  local bri = tonumber(selected.bri_pct) or 0
  return ui.card({
    id = "lights-room-panel",
    pad = "md",
    gap = "sm",
    children = {
      ui.row({ widths = { 0.62, 0.38 }, gap = "md", children = {
        ui.column({ gap = "xs", children = {
          ui.text(selected.name or "Lights", { style = "title", ink = "primary" }),
          label(string.format("%d of %d on", on, total)),
        } }),
        ui.row({ align = "end", gap = "sm", children = {
          label(on > 0 and "ON" or "OFF", "heading", on > 0 and "warning" or "muted"),
          action_button(on > 0 and "Turn off" or "Turn on", "lights.room_power", {
            group_id = selected.id, on = on == 0,
          }, { quiet = on == 0 }),
        } }),
      } }),
      ui.row({ widths = { 0.22, 0.78 }, gap = "sm", children = {
        label("BRIGHTNESS", "subheading", "muted"),
        ui.row({ align = "end", gap = "sm", children = {
          ui.text(string.format("%d%%", bri), { style = "title", ink = "warning" }),
          ui.text(meter(bri), { style = "heading", ink = "warning" }),
          label(on > 0 and "lit" or "standby"),
        } }),
      } }),
    },
  })
end

local function scene_rail(view, selected)
  local scenes = scenes_in(view, selected)
  local children = {
    label("SCENES", "subheading", "muted"),
  }
  if #scenes == 0 then
    children[#children + 1] = label("No scenes assigned to this group.")
  else
    local buttons = {}
    for _, scene in ipairs(scenes) do
      buttons[#buttons + 1] = action_button(scene.name or "Scene", "lights.scene", {
        scene_id = scene.id, group_id = selected.id,
      }, { small = true, quiet = true })
    end
    children[#children + 1] = ui.row({ gap = "sm", children = buttons })
  end
  return ui.column({ id = "lights-scenes", gap = "sm", children = children })
end

local function light_row(light, dense)
  local on = light.on == true
  local bri = percent(light.bri)
  local id = tostring(light.id)
  local name = light.name or ("Light " .. id)
  local power = action_button(on and "Off" or "On", "lights.power", { light_id = id, on = not on }, {
    small = true, selected = on, quiet = not on,
  })
  local presets = ui.row({ align = "end", gap = "xs", children = {
    action_button("25", "lights.brightness", { light_id = id, level = 25 }, { small = true, quiet = true }),
    action_button("50", "lights.brightness", { light_id = id, level = 50 }, { small = true, quiet = true }),
    action_button("100", "lights.brightness", { light_id = id, level = 100 }, { small = true, quiet = true }),
  } })

  if dense then
    return ui.card({
      id = "lights-light-" .. id,
      pad = "sm",
      gap = "xs",
      children = {
        ui.row({ widths = { 0.6, 0.4 }, gap = "sm", children = {
          ui.text(name, { style = "body", ink = on and "primary" or "text" }),
          ui.row({ align = "end", children = { power } }),
        } }),
        ui.row({ widths = { 0.36, 0.64 }, gap = "sm", children = {
          label(string.format("%d%% · %s", bri, on and "on" or "off"), "caption", on and "warning" or "muted"),
          presets,
        } }),
      },
    })
  end

  return ui.card({
    id = "lights-light-" .. id,
    pad = "sm",
    gap = "xs",
    children = {
      ui.row({ widths = { 0.7, 0.3 }, gap = "sm", children = {
        ui.column({ gap = "xs", children = {
          ui.text(name, { style = "heading", ink = on and "primary" or "text" }),
          label(on and "On" or "Off", "caption", on and "warning" or "muted"),
        } }),
        ui.row({ align = "end", children = { power } }),
      } }),
      ui.row({ widths = { 0.48, 0.52 }, gap = "sm", children = {
        ui.column({ gap = "xs", children = {
          label(string.format("%d%%", bri), "body", on and "warning" or "muted"),
          ui.text(meter(bri), { style = "caption", ink = on and "warning" or "muted" }),
        } }),
        presets,
      } }),
    },
  })
end

local function lights_list(view, selected)
  local lights = lights_in(view, selected)
  local children = {
    ui.row({ widths = { 0.7, 0.3 }, children = {
      label(string.format("LIGHTS · %d", #lights), "subheading", "muted"),
      ui.row({ align = "end", children = { label("tap a preset to dim", "caption", "muted") } }),
    } }),
  }
  if #lights == 0 then
    children[#children + 1] = label("No lights in this group.")
  else
    local cols = #lights > 4 and 3 or 2
    local widths = {}
    for index = 1, cols do widths[index] = 1 / cols end
    local row = {}
    for _, light in ipairs(lights) do
      row[#row + 1] = light_row(light, #lights > 4)
      if #row == cols then
        children[#children + 1] = ui.row({ widths = widths, gap = "sm", children = row })
        row = {}
      end
    end
    if #row > 0 then
      while #row < cols do row[#row + 1] = ui.spacer({ size = "xs" }) end
      children[#children + 1] = ui.row({ widths = widths, gap = "sm", children = row })
    end
  end
  return ui.column({ id = "lights-list", gap = "sm", children = children })
end

function M.render(view)
  view = view or {}
  local selected = room_by_id(view, view.selected)
  if not selected then
    selected = { id = "0", name = "Lights", on = 0, total = #(view.lights or {}), bri_pct = 0 }
  end
  local width, height = ui.viewport(1200, 820)
  ui.present(ui.column({
    id = "lights-root",
    h = math.max(760, height),
    pad = "lg",
    gap = "md",
    children = {
      header(view, selected),
      ui.row({
        id = "lights-body",
        widths = { 0.23, 0.77 },
        gap = "md",
        children = {
          ui.column({ gap = "md", children = {
            rooms_table(view, selected),
            bridge_card(view),
          } }),
          ui.column({ gap = "md", children = {
            room_panel(view, selected),
            scene_rail(view, selected),
            lights_list(view, selected),
          } }),
        },
      }),
    },
  }), theme())
end

return M
