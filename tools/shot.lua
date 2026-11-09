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
      x = s.translation[1], y = s.translation[2], w = s.scale[1], h = s.scale[2],
      opacity = s.opacity or 1, radius = s.radius or 0 }
  end,
  text = function(s)
    prims[#prims + 1] = { type = "text", id = s.id, ink = s.ink, text = s.text,
      x = s.translation[1], y = s.translation[2],
      px = (s.scale[1] or 1) * (TYPE.body or 16) * 1.25 }
  end,
}
_G.widget = { hit = function() end }
_G.module = {
  viewport = { width = width, height = height },
  theme = { micro = TYPE.micro, small = TYPE.small, body = TYPE.body, large = TYPE.large,
    radius = theme.radius or 10 },
}

-- The SDK is a build-time import in a package; here it is a checkout.
local sdk = os.getenv("FUNGOS_SDK") or (os.getenv("HOME") .. "/src/fungos-sdk")
package.preload["fungos.ui"] = function()
  return dofile(sdk .. "/lua/fungos/ui.lua")
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

local parts = {}
for _, node in ipairs(prims) do
  local fields = {}
  for key, value in pairs(node) do
    if type(value) == "number" then
      fields[#fields + 1] = string.format("%q:%.5f", key, value)
    elseif type(value) == "table" then
      fields[#fields + 1] = string.format("%q:[%d,%d,%d]", key, value[1], value[2], value[3])
    else
      fields[#fields + 1] = string.format("%q:%q", key, tostring(value))
    end
  end
  table.sort(fields)
  parts[#parts + 1] = "{" .. table.concat(fields, ",") .. "}"
end
local file = assert(io.open(out_path, "wb"))
file:write("[", table.concat(parts, ","), "]")
file:close()
print(string.format("%s: %d primitives, %d themed -> %s",
  theme.name or "theme", #prims, themed, out_path))
