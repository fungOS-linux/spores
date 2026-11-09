-- Check a composed Canvas module against the host's Lua contract.
--
-- The host parses every primitive strictly: unknown fields are rejected, a
-- control's `action` must be the *name*, `appearance` must be one it knows, and
-- control geometry lives in an 8-unit space (`canvas/src/lua_module.rs`,
-- `canvas/src/lua_scene.rs`). A module that gets any of that wrong composes
-- fine, installs fine, and then dies on the wall with nothing but a Lua error
-- card where the surface should be.
--
--   lua tools/check-module-contract.lua <artifact.lua>
--
-- Exit status is 1 when the artifact would be rejected.

local path = assert(arg[1], "usage: check-module-contract.lua <artifact.lua>")

local problems = {}
local counts = { group = 0, rect = 0, text = 0, button = 0 }

local function fail(fmt, ...)
  problems[#problems + 1] = string.format(fmt, ...)
end

local function finite(value)
  return type(value) == "number" and value == value and value ~= math.huge and value ~= -math.huge
end

local function check_fields(kind, spec, allowed)
  for key in pairs(spec) do
    if not allowed[key] then
      fail("%s: unknown field %q (the host rejects unknown fields)", kind, tostring(key))
    end
  end
end

local function check_transform(kind, spec)
  local translation = spec.translation
  if translation ~= nil then
    if type(translation) ~= "table" or #translation ~= 3 then
      fail("%s: translation must be three numbers", kind)
    else
      for i = 1, 3 do
        if not finite(translation[i]) then
          fail("%s: translation[%d] is not finite", kind, i)
        end
      end
    end
  end
  local scale = spec.scale
  if scale ~= nil then
    if type(scale) ~= "table" or #scale ~= 3 then
      fail("%s: scale must be three numbers", kind)
    else
      for i = 1, 3 do
        if scale[i] == 0 then
          fail("%s: scale[%d] cannot be zero", kind, i)
        end
      end
    end
  end
end

local NODE_FIELDS = { id = true, parent = true, translation = true, rotation = true, scale = true }

local function with(base, extra)
  local allowed = {}
  for key in pairs(base) do allowed[key] = true end
  for key in pairs(extra or {}) do allowed[key] = true end
  return allowed
end

canvas = {
  group = function(spec)
    counts.group = counts.group + 1
    check_fields("canvas.group", spec, NODE_FIELDS)
    check_transform("canvas.group", spec)
  end,
  rect = function(spec)
    counts.rect = counts.rect + 1
    check_fields("canvas.rect", spec, with(NODE_FIELDS, { ink = true, opacity = true, radius = true }))
    check_transform("canvas.rect", spec)
    if spec.opacity ~= nil and not (finite(spec.opacity) and spec.opacity >= 0 and spec.opacity <= 1) then
      fail("canvas.rect: opacity must be between 0 and 1")
    end
  end,
  text = function(spec)
    counts.text = counts.text + 1
    check_fields("canvas.text", spec, with(NODE_FIELDS, { text = true, ink = true }))
    check_transform("canvas.text", spec)
    if type(spec.text) ~= "string" then
      fail("canvas.text: text must be a string, got %s", type(spec.text))
    end
  end,
  image = function() end,
  mesh = function() end,
  host_resource = function() end,
}

local BUTTON_FIELDS = {
  id = true, label = true, action = true, body = true, x = true, y = true,
  width = true, height = true, enabled = true, appearance = true,
  icon = true, icon_size = true, icon_caption = true,
}
local APPEARANCES = { standard = true, quiet = true, hit = true }

widget = {
  button = function(spec)
    counts.button = counts.button + 1
    check_fields("widget.button", spec, BUTTON_FIELDS)
    for _, field in ipairs({ "id", "label", "action" }) do
      local value = spec[field]
      if type(value) ~= "string" then
        local hint = type(value) == "table"
          and " (ui.action returns a table; a control prim owes the host the action name)"
          or ""
        fail("widget.button: %s must be a string, got %s%s", field, type(value), hint)
      end
    end
    if spec.body ~= nil and type(spec.body) ~= "table" then
      fail("widget.button: body must be an object, got %s", type(spec.body))
    end
    if spec.appearance ~= nil and not APPEARANCES[spec.appearance] then
      fail("widget.button: appearance %q is not one the host knows (standard, quiet, hit)",
        tostring(spec.appearance))
    end
    for _, field in ipairs({ "x", "y", "width", "height" }) do
      if not finite(spec[field]) then
        fail("widget.button: %s must be a finite number", field)
      end
    end
    if finite(spec.x) and (spec.x < -4.0 or spec.x > 4.0) then
      fail("widget.button: x %s is outside the -4..4 control space", tostring(spec.x))
    end
    if finite(spec.y) and (spec.y < -4.0 or spec.y > 4.0) then
      fail("widget.button: y %s is outside the -4..4 control space", tostring(spec.y))
    end
    for _, field in ipairs({ "width", "height" }) do
      local value = spec[field]
      if finite(value) and (value <= 0 or value > 8.0) then
        fail("widget.button: %s %s is outside (0, 8]", field, tostring(value))
      end
    end
  end,
  hit = function() end,
  tokens = setmetatable({}, { __index = function() return function() return 1 end end }),
}

module = {
  id = "contract-check",
  inputs = {},
  theme = setmetatable({}, { __index = function() return 0.5 end }),
  event = { kind = "load" },
  resources = {},
}

local chunk, load_error = loadfile(path)
if not chunk then
  print("cannot load " .. path .. ": " .. tostring(load_error))
  os.exit(1)
end

local ok, run_error = pcall(chunk)
if not ok then
  print("module error: " .. tostring(run_error))
  os.exit(1)
end

local summary = string.format("%s: %d rect, %d text, %d group, %d control",
  path, counts.rect, counts.text, counts.group, counts.button)

if #problems == 0 then
  print("contract ok — " .. summary)
  os.exit(0)
end

print(string.format("contract FAILED (%d) — %s", #problems, summary))
for _, problem in ipairs(problems) do print("  " .. problem) end
os.exit(1)
