-- Renders the Spore's Lua view with a stand-in for the host's `fungos.ui`
-- module and prints the resulting tree as JSON.
--
-- This is not the host. Soil's Lua UI runtime is not in this branch yet, so this
-- shim exists to prove the view file and to hand the same tree to a preview
-- renderer. The node shape mirrors spores/notes/ui/API.md.
package.path = "spores/sheets/ui/?.lua;spores/sheets/?.lua;./?.lua;" .. package.path

local function node(kind, id, props, extra)
  local value = { kind = kind, id = id, props = props or {} }
  for key, item in pairs(extra or {}) do value[key] = item end
  return value
end

local ui = {
  text = function(id, text, props) return node("text", id, props, { text = text }) end,
  button = function(id, label, props) return node("button", id, props, { text = label }) end,
  row = function(id, props, children) return node("row", id, props, { children = children or {} }) end,
  column = function(id, props, children) return node("column", id, props, { children = children or {} }) end,
  rule = function(id, props) return node("rule", id, props) end,
  push = function(list, value) list[#list + 1] = value end,
  action_bar = function(id, props, children) return node("row", id, props, { children = children or {} }) end,
}

local action = { intent = function(name, payload) return { name = name, payload = payload } end }

package.preload["fungos.ui"] = function() return ui end
package.preload["fungos.action"] = function() return action end

local view = dofile("spores/sheets/ui/sheet.lua")
local snapshot = dofile(arg[1] or "spores/sheets/examples/fpl-snapshot.lua")
local tree = view.render(snapshot)

local function encode(value)
  local kind = type(value)
  if kind == "string" then
    return '"' .. value:gsub('\\', '\\\\'):gsub('"', '\\"'):gsub('\n', '\\n') .. '"'
  elseif kind == "number" then
    return string.format("%.6g", value)
  elseif kind == "boolean" then
    return tostring(value)
  elseif kind == "table" then
    local is_array = #value > 0
    local parts = {}
    if is_array then
      for _, item in ipairs(value) do parts[#parts + 1] = encode(item) end
      return "[" .. table.concat(parts, ",") .. "]"
    end
    local keys = {}
    for key in pairs(value) do
      if type(key) == "string" then keys[#keys + 1] = key end
    end
    table.sort(keys)
    for _, key in ipairs(keys) do
      parts[#parts + 1] = encode(key) .. ":" .. encode(value[key])
    end
    return "{" .. table.concat(parts, ",") .. "}"
  end
end

local f = io.open(arg[2] or "/dev/stdout", "w")
f:write(encode(tree))
if arg[2] then f:close() end
