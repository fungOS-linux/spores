-- Render a Spore view to a scene document without a compositor.
--
--   lua tools/shot.lua <view.lua> <snapshot.lua> <theme.lua> <out.json> [w] [h]
--
-- A preview tool, not a host: it provides the same `canvas` ABI Canvas does,
-- resolves `require` for the SDK, and nothing else. Delete it the day a host can
-- render a view headlessly.
local view_path, snapshot_path, theme_path, out_path =
  arg[1], arg[2], arg[3], arg[4]
local width, height = tonumber(arg[5]) or 1200, tonumber(arg[6]) or 44

assert(view_path and snapshot_path and theme_path and out_path,
  "usage: lua tools/shot.lua <view.lua> <snapshot.lua> <theme.lua> <out.json> [w] [h]")

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
_G.canvas = {
  rect = function(s)
    prims[#prims + 1] = { type = "rect", id = s.id, ink = s.ink,
      translation = s.translation, scale = s.scale, rotation = s.rotation,
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
      translation = s.translation, scale = s.scale,
      px = scale * scale * (TYPE.body or 16) * 1.25 }
  end,
}
_G.widget = { hit = function() end }
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

-- The SDK is a build-time import in a package; here it is a checkout. Two
-- layouts are in flight at once: `fungos-sdk/lua/fungos/ui.lua` is the surface
-- library the worklog composes, and `spores/sdk/ui/ui.lua` is the tree library
-- the gallery and the fleet view compose. Name one, and take whichever the
-- checkout actually contains rather than making every caller learn the split.
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
package.preload["fungos.ui"] = function()
  return dofile(sdk_ui())
end

local view = assert(loadfile(view_path))()
local snapshot = assert(loadfile(snapshot_path))()
view.render(snapshot)

-- A theme maps semantic ink to colour. Resolving it here is what keeps a view
-- free of colour, and lets one file restyle every Spore at once.
local themed = 0
for _, node in ipairs(prims) do
  local rgb = hex(inks[node.ink or ""])
  if rgb then node.rgb, themed = rgb, themed + 1 end
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
  if node.px then
    fields[#fields + 1] = string.format('%s:%.5f', quote("px"), node.px)
  end
  return "{" .. table.concat(fields, ",") .. "}"
end

local parts = {}
for _, node in ipairs(prims) do parts[#parts + 1] = node_json(node) end
local file = assert(io.open(out_path, "wb"))
file:write('{"kind":"scene","schema":1,"nodes":[', table.concat(parts, ","), "]}")
file:close()
print(string.format("%s: %d primitives, %d themed -> %s",
  theme.name or "theme", #prims, themed, out_path))
