-- Dump the home surface as the scene document a host actually draws.
--
--   lua tools/dump_scene.lua > ../../../canvas/crates/canvas-web/scenes/home.scene.json
--
-- Run from `spores/home`. The fixture is host-owned: this script supplies the
-- same inputs and theme the headless test uses, records the document that
-- `ui.present` hands to the host, and records the controls separately because
-- buttons are host-owned pixels rather than scene nodes.
--
-- The output is checked in beside the browser renderer. Regenerate it whenever
-- `ui/home.lua` or the ui SDK changes; `canvas-web` has a test that fails when
-- the launcher's landmark text disappears from the fixture.

local function library()
  local source = assert(io.open("../../sdk/ui/ui.lua", "rb"),
    "../../sdk/ui/ui.lua is missing; run this from spores/home")
  local chunk = assert(load(source:read("*a"), "sdk/ui/ui.lua"))
  source:close()
  return chunk()
end

package.preload["fungos.ui"] = library

local document
local buttons = {}

_G.module = {
  inputs = { width = 1440, height = 1600 },
  theme = { title = 30, large = 22, body = 15, small = 12, micro = 10, radius = 10 },
}
_G.canvas = {
  present = function(value)
    document = value
    return true
  end,
}
_G.widget = {
  button = function(spec)
    buttons[#buttons + 1] = {
      id = spec.id,
      label = spec.label,
      action = spec.action,
      body = spec.body or {},
      x = spec.x,
      y = spec.y,
      width = spec.width,
      height = spec.height,
      enabled = spec.enabled ~= false,
      appearance = spec.appearance,
    }
  end,
}

local home = assert(loadfile("ui/home.lua"))()
home.render(nil)
assert(document and document.nodes and #document.nodes > 0, "home.lua drew no document")

-- ---- JSON ---------------------------------------------------------------

local function quote(value)
  local out = { '"' }
  for index = 1, #value do
    local byte = value:sub(index, index)
    local code = value:byte(index)
    if byte == '"' then out[#out + 1] = '\\"'
    elseif byte == "\\" then out[#out + 1] = "\\\\"
    elseif byte == "\n" then out[#out + 1] = "\\n"
    elseif byte == "\r" then out[#out + 1] = "\\r"
    elseif byte == "\t" then out[#out + 1] = "\\t"
    elseif code < 0x20 then out[#out + 1] = string.format("\\u%04x", code)
    else out[#out + 1] = byte end
  end
  out[#out + 1] = '"'
  return table.concat(out)
end

local function is_array(value)
  local count = 0
  for key in pairs(value) do
    if type(key) ~= "number" or key < 1 or key % 1 ~= 0 then return false end
    count = count + 1
  end
  for index = 1, count do
    if value[index] == nil then return false end
  end
  return true
end

local function encode(value)
  local kind = type(value)
  if value == nil then return "null"
  elseif kind == "boolean" then return value and "true" or "false"
  elseif kind == "number" then
    assert(value == value and value ~= math.huge and value ~= -math.huge,
      "scene contains a non-finite number")
    return string.format("%.9g", value)
  elseif kind == "string" then return quote(value)
  elseif kind == "table" then
    if is_array(value) then
      local items = {}
      for index = 1, #value do items[#items + 1] = encode(value[index]) end
      return "[" .. table.concat(items, ",") .. "]"
    end
    local keys = {}
    for key in pairs(value) do
      assert(type(key) == "string", "scene table keys must be strings")
      keys[#keys + 1] = key
    end
    table.sort(keys)
    local items = {}
    for _, key in ipairs(keys) do items[#items + 1] = quote(key) .. ":" .. encode(value[key]) end
    return "{" .. table.concat(items, ",") .. "}"
  end
  error("cannot encode " .. kind)
end

io.write(encode({ scene = document, buttons = buttons }), "\n")
