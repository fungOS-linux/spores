-- Render a Spore view to a scene document without a compositor.
--
--   lua tools/shot.lua <view.lua> <snapshot.lua> <theme.lua> <out.json> [w] [h]
--                     [--query=<text>] [--scroll=<px>] [--hover=<id>]
--
-- A preview tool, not a host: it provides the same `canvas` ABI Canvas does,
-- resolves `require` for the SDK, and nothing else. A view paints itself through
-- `ui.present`, which owns the layout — so this tool records primitives and never
-- lays anything out. A view that hands back a tree instead of presenting it
-- records nothing at all, which is why that case warns rather than passing.
--
-- The flags are the host state a view is allowed to read — a query, a scroll
-- offset, the control under the pointer — so the picture a page shows can be
-- asked for from a shell.
local positional, flags = {}, {}
for index = 1, #arg do
  local key, value = arg[index]:match("^%-%-([%w_]+)=(.*)$")
  if key then flags[key] = value else positional[#positional + 1] = arg[index] end
end

local view_path, snapshot_path, theme_path, out_path =
  positional[1], positional[2], positional[3], positional[4]
local width, height = tonumber(positional[5]) or 1200, tonumber(positional[6]) or 900

assert(view_path and snapshot_path and theme_path and out_path,
  "usage: lua tools/shot.lua <view.lua> <snapshot.lua> <theme.lua> <out.json> [w] [h]"
  .. " [--query=<text>] [--scroll=<px>] [--hover=<id>]")

local theme = dofile(theme_path)
local inks = theme.inks or {}
-- The theme declares the scale the preview simulates, so a preview of the wall
-- is drawn at the wall's sizes. `canvas.measure_text` is deliberately absent
-- below, which leaves the library's own estimate in charge — the same one Canvas
-- uses, so the lines wrap where the wall wraps them.
local TYPE = theme.type or { micro = 10, small = 13, body = 16, large = 20 }

local function hex(value)
  local r, g, b = tostring(value or ""):match("^#(%x%x)(%x%x)(%x%x)$")
  if not r then return nil end
  return { tonumber(r, 16), tonumber(g, 16), tonumber(b, 16) }
end

local prims = {}
local hits, presses = {}, {}
_G.canvas = {
  rect = function(s)
    prims[#prims + 1] = { type = "rect", id = s.id, ink = s.ink,
      translation = s.translation, scale = s.scale, rgb = s.rgb,
      opacity = s.opacity or 1, radius = s.radius or 0 }
  end,
  text = function(s)
    prims[#prims + 1] = { type = "text", id = s.id, ink = s.ink, text = s.text,
      translation = s.translation, scale = s.scale, rgb = s.rgb,
      px = (s.scale[1] or 1) * (TYPE.body or 16) * 1.25 }
  end,
}
_G.widget = {
  hit = function(s)
    hits[#hits + 1] = { id = s.id, x = s.x, y = s.y, width = s.width, height = s.height }
  end,
  button = function(s) presses[#presses + 1] = s end,
}
_G.module = {
  viewport = { width = width, height = height },
  theme = { micro = TYPE.micro, small = TYPE.small, body = TYPE.body, large = TYPE.large,
    radius = theme.radius or 10 },
}
if flags.query then _G.module.query = flags.query end
if flags.scroll then _G.module.scroll = { offset = tonumber(flags.scroll) or 0 } end
if flags.hover then _G.module.hover = flags.hover end

-- The SDK is a build-time import in a package; here it is a checkout. A view
-- may require more than `fungos.ui` (`fungos.action`, `fungos.chart`, ...), so
-- the whole `lua/` tree is on the path, not just one module.
local sdk = os.getenv("FUNGOS_SDK") or (os.getenv("HOME") .. "/src/fungos-sdk")
package.path = sdk .. "/lua/?.lua;" .. sdk .. "/lua/?/init.lua;" .. package.path

local view = assert(loadfile(view_path))()
local snapshot = assert(loadfile(snapshot_path))()
-- One argument, like every host: `tend`, `chrome-host` and Canvas all call
-- `render(snapshot)`. A view that also wants the host context reads it from the
-- `module` global, which is what the kit's own accessors do.
view.render(snapshot)

-- A theme maps semantic ink to colour. Resolving it here is what keeps a view
-- free of colour, and lets one file restyle every Spore at once.
local themed = 0
for _, node in ipairs(prims) do
  -- A literal colour wins: a terminal cell names its own ink, and a theme role
  -- cannot express sixteen ANSI colours without sixteen roles.
  if not node.rgb then
    local rgb = hex(inks[node.ink or ""])
    if rgb then node.rgb, themed = rgb, themed + 1 end
  end
end

-- Lua's `%q` is not JSON: it can emit decimal escapes a JSON reader rejects.
local ESCAPES = { ['"'] = '\\"', ['\\'] = '\\\\', ['\n'] = '\\n', ['\r'] = '\\r', ['\t'] = '\\t' }
local function quote(value)
  local body = tostring(value or ""):gsub('[%c"\\]', function(char)
    return ESCAPES[char] or string.format("\\u%04x", char:byte())
  end)
  return '"' .. body .. '"'
end

-- The document is a `soil_scene` node list: `transform.translation` and
-- `transform.scale` in the -4..4 space, `ink` as a role, and `id` for hit-testing.
-- A preview adds the two things a host resolves for itself — `rgb` from the
-- palette, `px` from the type scale — and adds nothing else, so a page that reads
-- this reads the same document Canvas does.
local function node_json(node)
  local fields = {
    quote("id") .. ":" .. quote(node.id),
    quote("type") .. ":" .. quote(node.type),
    quote("ink") .. ":" .. quote(node.ink),
    string.format('%s:{"translation":[%.5f,%.5f,%.5f],"scale":[%.5f,%.5f,%.5f]}',
      quote("transform"),
      node.translation[1], node.translation[2], node.translation[3] or 0,
      node.scale[1], node.scale[2], node.scale[3] or 1),
    string.format('%s:%.5f', quote("opacity"), node.opacity or 1),
  }
  if node.radius and node.radius > 0 then
    fields[#fields + 1] = string.format('%s:%.5f', quote("radius"), node.radius)
  end
  if node.rgb then
    fields[#fields + 1] = string.format('%s:[%d,%d,%d]', quote("rgb"), node.rgb[1], node.rgb[2], node.rgb[3])
  end
  if node.text then
    fields[#fields + 1] = quote("text") .. ":" .. quote(node.text)
  end
  if node.px then
    fields[#fields + 1] = string.format('%s:%.5f', quote("px"), node.px)
  end
  return "{" .. table.concat(fields, ",") .. "}"
end

local parts = {}
for _, node in ipairs(prims) do parts[#parts + 1] = node_json(node) end

-- The hit targets and the window the view drew are what a page needs to be
-- interactive: where a row is, and how far the log goes. They ride with the
-- document because they are answers about this exact frame.
local function hit_json(entry)
  return string.format('{"id":%s,"x":%.5f,"y":%.5f,"width":%.5f,"height":%.5f}',
    quote(entry.id), entry.x, entry.y, entry.width, entry.height)
end
local hit_parts = {}
for _, entry in ipairs(hits) do hit_parts[#hit_parts + 1] = hit_json(entry) end

local window = view.window or {}
-- What the frame *is*, as opposed to what it draws. Worklog publishes the row
-- window it chose; a terminal publishes the grid it is showing. A view adds its
-- own keys through `M.meta` rather than a second document beside this one.
local meta_table = {
  row_height = tonumber(view.row_height) or 0,
  first = tonumber(window.first) or 0,
  shown = tonumber(window.shown) or 0,
  total = tonumber(window.total) or 0,
  presses = #presses,
}
if type(view.meta) == "table" then
  for key, value in pairs(view.meta) do meta_table[key] = value end
end

local function meta_value(value)
  local kind = type(value)
  if kind == "string" then return quote(value) end
  if kind == "boolean" then return value and "true" or "false" end
  if kind == "number" then return string.format("%.6g", value) end
  if kind == "table" then
    local keys = {}
    for key in pairs(value) do keys[#keys + 1] = tostring(key) end
    table.sort(keys)
    local out = {}
    for _, key in ipairs(keys) do
      local encoded = meta_value(value[key])
      if encoded then out[#out + 1] = quote(key) .. ":" .. encoded end
    end
    return "{" .. table.concat(out, ",") .. "}"
  end
  return nil
end
local meta = meta_value(meta_table)

local file = assert(io.open(out_path, "wb"))
file:write('{"kind":"scene","schema":1,"nodes":[', table.concat(parts, ","),
  '],"hits":[', table.concat(hit_parts, ","), '],"meta":', meta, "}")
file:close()
if #prims == 0 then
  -- Not an error: an empty snapshot legitimately draws an empty scene. But it is
  -- also what a view that returns a tree instead of presenting it looks like, and
  -- that failure used to be silent.
  io.stderr:write("shot: the view painted no primitives; a returned tree is not a painted scene\n")
end

print(string.format("%s: %d primitives, %d themed -> %s",
  theme.name or "theme", #prims, themed, out_path))
