-- Render the Rooms view to the scene document a browser can draw.
--
--   lua tools/render_rooms.lua <view.lua> <snapshot.lua> <ui.lua> <out.json> <w> <h>
--
-- This is the web presenter's equivalent of Canvas's module loader: it gives
-- the real Lua view the real library, a viewport and a snapshot, then records
-- the primitives and the controls the view emitted. It deliberately contains
-- no room logic, so the browser cannot grow a second opinion about the UI.

local view_path, snapshot_path, ui_path, out_path, width, height =
  arg[1], arg[2], arg[3], arg[4], tonumber(arg[5]) or 1200, tonumber(arg[6]) or 800
assert(view_path and snapshot_path and ui_path and out_path,
  "usage: render_rooms.lua <view> <snapshot> <ui> <out> [w] [h]")

local ui = assert(loadfile(ui_path))()
package.preload["fungos.ui"] = function() return ui end
_G.ui = ui

local prims, buttons = {}, {}
_G.canvas = {
  rect = function(spec)
    prims[#prims + 1] = {
      id = spec.id, type = "rect", ink = spec.ink,
      translation = spec.translation, scale = spec.scale,
      opacity = spec.opacity or 1, radius = spec.radius or 0,
    }
  end,
  text = function(spec)
    prims[#prims + 1] = {
      id = spec.id, type = "text", ink = spec.ink, text = spec.text,
      translation = spec.translation, scale = spec.scale,
      px = (spec.scale[1] or 1) * 16 * 1.25,
    }
  end,
}
_G.widget = {
  button = function(spec) buttons[#buttons + 1] = spec end,
  hit = function() end,
}

_G.module = { viewport = { width = width, height = height } }
local view = assert(loadfile(view_path))()
local snapshot = assert(loadfile(snapshot_path))()
view.render(snapshot)

local ESCAPES = {
  ['"'] = '\\"', ['\\'] = '\\\\', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t',
}
local function quote(value)
  local body = tostring(value or ""):gsub('[%c"\\]', function(char)
    return ESCAPES[char] or string.format("\\u{%04x}", char:byte())
  end)
  return '"' .. body .. '"'
end
local function number(value) return string.format("%.5f", tonumber(value) or 0) end
local function transform(node)
  local t = node.translation or { 0, 0, 0 }
  local s = node.scale or { 1, 1, 1 }
  return string.format('{"translation":[%s,%s,%s],"scale":[%s,%s,%s]}',
    number(t[1]), number(t[2]), number(t[3] or 0),
    number(s[1]), number(s[2]), number(s[3] or 1))
end
local function node_json(node)
  local fields = {
    '"id":' .. quote(node.id),
    '"type":' .. quote(node.type),
    '"ink":' .. quote(node.ink),
    '"transform":' .. transform(node),
    '"opacity":' .. number(node.opacity or 1),
  }
  if node.radius and node.radius > 0 then
    fields[#fields + 1] = '"radius":' .. number(node.radius)
  end
  if node.text then fields[#fields + 1] = '"text":' .. quote(node.text) end
  if node.px then fields[#fields + 1] = '"px":' .. number(node.px) end
  return "{" .. table.concat(fields, ",") .. "}"
end

local nodes = {}
for index, node in ipairs(prims) do nodes[index] = node_json(node) end
local controls = {}
for index, button in ipairs(buttons) do
  controls[index] = string.format(
    '{"id":%s,"label":%s,"action":%s,"appearance":%s,"enabled":%s,"x":%s,"y":%s,"width":%s,"height":%s}',
    quote(button.id), quote(button.label), button.action and quote(button.action) or "null",
    button.appearance and quote(button.appearance) or "null",
    button.enabled == false and "false" or "true",
    number(button.x), number(button.y), number(button.width), number(button.height))
end

local file = assert(io.open(out_path, "wb"), "cannot write " .. out_path)
file:write('{"kind":"scene","schema":1,"nodes":[', table.concat(nodes, ","),
  '],"buttons":[', table.concat(controls, ","), ']}')
file:close()
