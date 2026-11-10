-- Render a Spore view to a scene document without a compositor.
--
--   lua tools/shot.lua <view.lua> <snapshot.lua> <theme.lua> <out.json> [w] [h]
--                     [--query=<text>] [--scroll=<px>] [--sidebar=<n>] [--hover=<id>]
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
  .. " [--query=<text>] [--scroll=<px>] [--sidebar=<n>] [--hover=<id>]")

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
local hits, presses = {}, 0
-- Press regions travel with the scene. Canvas owns hit-testing, so a view states
-- a rectangle and an intent; a host that can route the intent draws nothing for
-- it. `tools/shot.lua` used to drop these, which is why a preview could not
-- answer a click. The document now carries them and the page can.
local function record_hit(spec)
  if type(spec) == "table" then hits[#hits + 1] = spec end
end
_G.canvas = {
  rect = function(s)
    prims[#prims + 1] = { type = "rect", id = s.id, ink = s.ink,
      translation = s.translation, scale = s.scale, rotation = s.rotation, rgb = s.rgb,
      opacity = s.opacity or 1, radius = s.radius or 0 }
  end,
  text = function(s)
    -- The library asks for a font size by solving `scale^2 * body * depth = size`
    -- (see text_prim). The preview has to finish that equation: using `scale *`
    -- instead of `scale^2 *` drew every label about 40% larger than the width the
    -- layout reserved, which is why tables and graphs looked crowded while the
    -- geometry was correct.
    local scale = s.scale[1] or 1
    prims[#prims + 1] = { type = "text", id = s.id, ink = s.ink, text = s.text,
      translation = s.translation, scale = s.scale, rgb = s.rgb,
      -- A text node may carry styled runs. They travel with the node so a line
      -- is one node, not one node per colour change.
      runs = s.runs,
      px = scale * scale * (TYPE.body or 16) * 1.25 }
  end,
}
_G.widget = {
  hit = record_hit,
  button = function(spec)
    presses = presses + 1
    record_hit(spec)
  end,
}
_G.module = {
  viewport = { width = width, height = height },
  inputs = { width = width, height = height },
  -- The complete type scale, not the four sizes the worklog happens to use: a
  -- dashboard stat draws at `title`, and a theme without it fails at the first
  -- number instead of rendering one size too small.
  theme = { micro = TYPE.micro, small = TYPE.small, body = TYPE.body,
    large = TYPE.large, title = TYPE.title or TYPE.large,
    display = TYPE.display or TYPE.title or TYPE.large,
    radius = theme.radius or 10 },
}
if flags.query then _G.module.query = flags.query end
if flags.scroll then _G.module.scroll = { offset = tonumber(flags.scroll) or 0 } end
-- A panel's own scroll offset. Same shape as `scroll`: the host says how far,
-- the view decides what moves. The terminal's scrollback and its device list
-- are two different lists, so they are two different offsets.
if flags.sidebar then _G.module.sidebar = { offset = tonumber(flags.sidebar) or 0 } end
if flags.hover then _G.module.hover = flags.hover end

-- The SDK is a build-time import in a package; here it is a checkout. Two
-- layouts are in flight at once: `fungos-sdk/lua/fungos/ui.lua` is the surface
-- library a packaged view composes, and `spores/sdk/ui/ui.lua` is the tree
-- library this repo composes. Take whichever the checkout actually contains;
-- the `lua/` tree goes on the path too, because a view may require more than
-- `fungos.ui` (`fungos.action`, `fungos.chart`, ...).
local sdk = os.getenv("FUNGOS_SDK") or (os.getenv("HOME") .. "/src/fungos-sdk")
local function sdk_ui()
  for _, candidate in ipairs({ sdk .. "/lua/fungos/ui.lua", sdk .. "/sdk/ui/ui.lua" }) do
    local handle = io.open(candidate, "rb")
    if handle then
      handle:close()
      return candidate
    end
  end
  error("shot: no fungos.ui under " .. sdk .. " (pass FUNGOS_SDK)")
end
package.path = sdk .. "/lua/?.lua;" .. sdk .. "/lua/?/init.lua;" .. package.path
package.preload["fungos.ui"] = function()
  return dofile(sdk_ui())
end

local view = assert(loadfile(view_path))()
-- `-` is stdin: a host that already has the snapshot in memory should not have
-- to write it to a file to hand it over, and a file in a temp directory can be
-- swept away between the write and the read.
local snapshot
if snapshot_path == "-" then
  local source = io.read("*a")
  snapshot = assert(load(source, "@stdin"))()
else
  snapshot = assert(loadfile(snapshot_path))()
end
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
  -- Canvas takes a Z quaternion; the document takes Euler XYZ. Only Z is drawn,
  -- so the conversion is one two-argument atan and the x/y terms stay zero.
  local rotation = node.rotation or { 0, 0, 0, 1 }
  local angle = 2 * math.atan(rotation[3] or 0, rotation[4] or 1)
  local fields = {
    quote("id") .. ":" .. quote(node.id),
    quote("type") .. ":" .. quote(node.type),
    quote("ink") .. ":" .. quote(node.ink),
    string.format('%s:{"translation":[%.5f,%.5f,%.5f],"rotation":[0,0,%.5f],"scale":[%.5f,%.5f,%.5f]}',
      quote("transform"),
      node.translation[1], node.translation[2], node.translation[3] or 0,
      angle,
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
  if node.runs then
    -- Each run resolves its own ink, because a terminal row names its own
    -- colours and the line's first colour is not the line's colour.
    local runs = {}
    for _, run in ipairs(node.runs) do
      local run_fields = { quote("text") .. ":" .. quote(run.text) }
      if run.ink then
        run_fields[#run_fields + 1] = quote("ink") .. ":" .. quote(run.ink)
      end
      local rgb = run.rgb or hex(inks[run.ink or ""])
      if rgb then
        run_fields[#run_fields + 1] = string.format('%s:[%d,%d,%d]', quote("rgb"), rgb[1], rgb[2], rgb[3])
      end
      runs[#runs + 1] = "{" .. table.concat(run_fields, ",") .. "}"
    end
    fields[#fields + 1] = quote("runs") .. ":[" .. table.concat(runs, ",") .. "]"
  end
  if node.px then
    fields[#fields + 1] = string.format('%s:%.5f', quote("px"), node.px)
  end
  return "{" .. table.concat(fields, ",") .. "}"
end

local parts = {}
for _, node in ipairs(prims) do parts[#parts + 1] = node_json(node) end

-- A hit target is a rectangle plus an intent. The payload is the view's own, so
-- it has to survive the document: strings, numbers, booleans and flat tables are
-- enough to name a path and say what to do with it.
local function json_value(value)
  local kind = type(value)
  if kind == "string" then
    return quote(value)
  elseif kind == "number" then
    if value ~= value or value == math.huge or value == -math.huge then return "null" end
    return string.format("%.17g", value)
  elseif kind == "boolean" then
    return tostring(value)
  elseif kind == "table" then
    local keys = {}
    for key in pairs(value) do keys[#keys + 1] = key end
    if #keys == 0 then return "{}" end
    table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
    local array = true
    for index, key in ipairs(keys) do
      if key ~= index then array = false end
    end
    local out = {}
    if array then
      for _, key in ipairs(keys) do out[#out + 1] = json_value(value[key]) end
      return "[" .. table.concat(out, ",") .. "]"
    end
    for _, key in ipairs(keys) do
      out[#out + 1] = quote(tostring(key)) .. ":" .. json_value(value[key])
    end
    return "{" .. table.concat(out, ",") .. "}"
  end
  return "null"
end

local function hit_json(hit)
  return "{" .. table.concat({
    quote("id") .. ":" .. quote(hit.id or ""),
    quote("label") .. ":" .. quote(hit.label or ""),
    quote("action") .. ":" .. (hit.action and quote(hit.action) or "null"),
    quote("body") .. ":" .. json_value(hit.body),
    string.format('%s:%.6f', quote("x"), hit.x or 0),
    string.format('%s:%.6f', quote("y"), hit.y or 0),
    string.format('%s:%.6f', quote("width"), hit.width or 0),
    string.format('%s:%.6f', quote("height"), hit.height or 0),
    string.format('%s:%.6f', quote("w"), hit.width or 0),
    string.format('%s:%.6f', quote("h"), hit.height or 0),
  }, ",") .. "}"
end

local hit_parts = {}
for _, hit in ipairs(hits) do hit_parts[#hit_parts + 1] = hit_json(hit) end

local window = view.window or {}
-- What the frame *is*, as opposed to what it draws. Worklog publishes the row
-- window it chose; a terminal publishes the grid it is showing. A view adds its
-- own keys through `M.meta` rather than a second document beside this one.
local meta_table = {
  row_height = tonumber(view.row_height) or 0,
  first = tonumber(window.first) or 0,
  shown = tonumber(window.shown) or 0,
  total = tonumber(window.total) or 0,
  presses = presses,
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
    -- Keep the real key beside its printed form: looking the value up by the
    -- string after sorting (`value["1"]` on an array) returns nil, and a list of
    -- objects serialises as `{}` — present, empty, and wrong.
    local entries = {}
    for key, item in pairs(value) do
      entries[#entries + 1] = { key = tostring(key), value = item }
    end
    table.sort(entries, function(left, right) return left.key < right.key end)
    local out = {}
    for _, entry in ipairs(entries) do
      local encoded = meta_value(entry.value)
      if encoded then out[#out + 1] = quote(entry.key) .. ":" .. encoded end
    end
    return "{" .. table.concat(out, ",") .. "}"
  end
  return nil
end
local meta = meta_value(meta_table)

if out_path == "-" then
  io.write('{"kind":"scene","schema":1,"nodes":[', table.concat(parts, ","),
    '],"hits":[', table.concat(hit_parts, ","), '],"meta":', meta, "}")
  io.stdout:flush()
  -- stdout is the document; the human-readable summary has to go elsewhere.
  io.stderr:write(string.format("%s: %d primitives, %d themed, %d hit(s) -> stdout\n",
    theme.name or "theme", #prims, themed, #hits))
else
  local file = assert(io.open(out_path, "wb"))
  file:write('{"kind":"scene","schema":1,"nodes":[', table.concat(parts, ","),
    '],"hits":[', table.concat(hit_parts, ","), '],"meta":', meta, "}")
  file:close()
  print(string.format("%s: %d primitives, %d themed, %d hit(s) -> %s",
    theme.name or "theme", #prims, themed, #hits, out_path))
end
