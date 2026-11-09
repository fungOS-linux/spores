-- Compose a Spore view into the single Lua module Canvas loads.
--
-- Canvas strips `load`, `require` and `package` from its sandbox, so a view that
-- the author wrote as three files has to arrive as one. Every Spore with a Lua
-- UI needs the same four steps — read, strip, inline the layout kit, enforce the
-- size cap — so they live here once. A Spore's own composer is then its file
-- list, not a second copy of the rules.
--
--   local compose = dofile(tools .. "/tools/compose_lua.lua")
--   compose.install{ ... }

local compose = {}

-- Canvas used to reject a module source over 32 KiB; it does not any more —
-- `spore-dev-git` at 95 KB and `gallery-ui` at 54 KB both render. The old bound
-- was kept here as a build-time failure, which made this composer reject
-- modules the host would happily run. A genuinely huge module is still a
-- mistake, so the guard stays with a bound that only catches mistakes.
compose.LIMIT = 1 << 20

function compose.read(path)
  local file = assert(io.open(path, "rb"), "missing " .. path)
  local text = file:read("*a")
  file:close()
  return text
end

-- Strip comments and blank lines, tracking string literals so a `--` inside a
-- string survives. The authored files stay readable; the installed artifact
-- stays under the cap.
function compose.strip_comments(source)
  local out = {}
  local index, length = 1, #source
  local quote = nil
  while index <= length do
    local char = source:sub(index, index)
    if quote then
      out[#out + 1] = char
      if char == "\\" then
        index = index + 1
        out[#out + 1] = source:sub(index, index)
      elseif char == quote then
        quote = nil
      end
      index = index + 1
    elseif char == '"' or char == "'" then
      quote = char
      out[#out + 1] = char
      index = index + 1
    elseif char == "-" and source:sub(index + 1, index + 1) == "-" then
      while index <= length and source:sub(index, index) ~= "\n" do
        index = index + 1
      end
    else
      out[#out + 1] = char
      index = index + 1
    end
  end
  local lines = {}
  for raw in table.concat(out):gmatch("([^\n]*)\n?") do
    local line = raw:gsub("^%s+", ""):gsub("%s+$", "")
    if line ~= "" then lines[#lines + 1] = line end
  end
  return table.concat(lines, "\n")
end

-- The `BEGIN-LAYOUT` section of the UI kit: Canvas has no layout service, so a
-- view that wants rows and flex calls this instead. Only the section is
-- inlined; the kit is ~21 KB and the cap is 32.
function compose.layout_section(ui_lib_path)
  local lines, inside = {}, false
  for line in compose.read(ui_lib_path):gmatch("([^\n]*)\n?") do
    -- Plain find, not a pattern: `-` is magic in Lua patterns and would
    -- silently match nothing, leaving an empty section.
    if line:find("BEGIN-LAYOUT", 1, true) then
      inside = true
    elseif line:find("END-LAYOUT", 1, true) then
      inside = false
    elseif inside then
      lines[#lines + 1] = line
    end
  end
  assert(#lines > 0, "layout section not found in " .. ui_lib_path)
  return compose.strip_comments(table.concat(lines, "\n"))
end

-- The source a view sees as `ui`.
--
-- A view that only asks for layout gets the layout section and nothing else. A
-- view that asks for the kit gets the kit. A view that asks for neither gets
-- `nil`, so a reference to `ui` fails loudly instead of resolving to a global.
local function ui_chunk(view_source, ui_lib_path)
  if not view_source:find("ui%.") then return "local ui = nil", false end
  local needs_kit = false
  for name in view_source:gmatch("ui%.([%a_][%w_]*)") do
    if name ~= "layout" then
      needs_kit = true
      break
    end
  end
  if needs_kit then
    return table.concat({ "local ui = (function()", compose.strip_comments(compose.read(ui_lib_path)), "end)()" }, "\n"), true
  end
  return table.concat({ "local ui = {}", "(function()", compose.layout_section(ui_lib_path), "end)()" }, "\n"), false
end

-- Build the composed module source.
--
--   view      path to the view; required
--   snapshot  path to a Lua file returning the snapshot table, or nil
--   ui_lib    path to ui.lua for the in-page layout kit; required when the
--             view uses `ui.` and no `libs` entry provides the module
--   libs      map of module name to Lua file, preloaded so the view's
--             `require("fungus.ui")` resolves to *this* repo's package rather
--             than whatever the tool happens to ship. See `sdk/ui/package.toml`.
--   style     path to the user's style overrides, or nil
--   app_id    per-app style key, or nil
function compose.build(options)
  assert(options.view, "compose.build needs a view")
  local view = compose.strip_comments(compose.read(options.view))

  -- A package-provided library arrives through `require`, so no `ui` upvalue is
  -- built for it: the view declares its own local from the shim below.
  local uses_package = false
  for _, _ in pairs(options.libs or {}) do uses_package = true end

  local inlined_ui, has_kit = nil, false
  if not uses_package then
    inlined_ui, has_kit = ui_chunk(view, options.ui_lib)
  end

  local parts = {
    "-- generated by tools/compose_lua.lua from " .. options.view .. "; do not edit here",
  }

  if uses_package then
    -- Canvas strips `require` and `package` from the sandbox, so a module cannot
    -- register a `package.preload` entry and then call `require` — it gets
    -- neither, and the view receives nil instead of its library. A local factory
    -- table handed to the view as a parameter touches no global and survives.
    parts[#parts + 1] = "local __libs = {"
    for name, path in pairs(options.libs) do
      parts[#parts + 1] = string.format("  [%q] = function()", name)
      parts[#parts + 1] = compose.strip_comments(compose.read(path))
      parts[#parts + 1] = "  end,"
    end
    parts[#parts + 1] = "}"
    parts[#parts + 1] = "local function __require(name)"
    parts[#parts + 1] = "  local factory = __libs[name]"
    parts[#parts + 1] = "  if not factory then error('unresolved require: ' .. tostring(name), 0) end"
    parts[#parts + 1] = "  return factory()"
    parts[#parts + 1] = "end"
  else
    parts[#parts + 1] = inlined_ui
  end

  if options.snapshot then
    parts[#parts + 1] = "local snapshot = (function()"
    parts[#parts + 1] = compose.strip_comments(compose.read(options.snapshot))
    parts[#parts + 1] = "end)()"
  end

  local style_path = options.style
  if style_path == nil then
    style_path = (os.getenv("HOME") or "") .. "/.config/fungos/ui.lua"
  end
  local style_file = style_path ~= "" and io.open(style_path, "rb")
  local style_source = style_file and style_file:read("*a") or "return {}"
  if style_file then style_file:close() end
  parts[#parts + 1] = "local user_style = (function()"
  parts[#parts + 1] = style_source
  parts[#parts + 1] = "end)()"

  -- `ui.configure` only exists when the whole kit was inlined; calling it after
  -- inlining just the layout section would be a nil call at load time.
  if has_kit and options.app_id and options.app_id ~= "" then
    parts[#parts + 1] = "ui.configure(user_style, " .. string.format("%q", options.app_id) .. ")"
  elseif has_kit then
    parts[#parts + 1] = "-- no app id: configure skipped"
  else
    parts[#parts + 1] = "-- layout only: configure skipped"
  end

  if uses_package then
    parts[#parts + 1] = "local view = (function(require)"
    parts[#parts + 1] = view
    parts[#parts + 1] = "end)(__require)"
  else
    parts[#parts + 1] = "local view = (function(ui)"
    parts[#parts + 1] = view
    parts[#parts + 1] = "end)(ui)"
  end
  parts[#parts + 1] = "view.render(snapshot)"
  parts[#parts + 1] = ""
  return table.concat(parts, "\n")
end

-- Build, check the cap, and write `<out_dir>/<name>.lua` plus its manifest.
function compose.install(options)
  assert(options.out_dir, "compose.install needs an out_dir")
  assert(options.name, "compose.install needs a name")
  local source = compose.build(options)
  -- The caller names a directory, not necessarily an existing one; `tend` creates
  -- its output directory too, and a composer that cannot write where it was told
  -- is a worse error than the one it reports.
  os.execute(string.format("mkdir -p %q", options.out_dir))
  local path = options.out_dir .. "/" .. options.name .. ".lua"
  if #source > compose.LIMIT then
    io.stderr:write(string.format(
      "compose_lua: %s\n  is %d bytes, over the %d-byte sanity bound.\n"
        .. "  Trim the snapshot before composing.\n",
      path, #source, compose.LIMIT))
    os.exit(1)
  end

  local file = assert(io.open(path, "wb"), "cannot write " .. path)
  file:write(source)
  file:close()

  -- A control the host must route is a grant the manifest has to declare: an
  -- unlisted `widget.button` action is rejected at render time, which is how a
  -- surface that composed fine ends up as an error card on the wall. The caller
  -- names its intents; the composer writes them down.
  local actions = {}
  for _, action in ipairs(options.actions or {}) do
    assert(type(action.id) == "string" and type(action.destination) == "string"
      and type(action.kind) == "string",
      "compose: each action needs id, destination and kind")
    -- The host validates these as `identifier`/`contract`: lower case, digits and
    -- hyphens for the id, a dotted lower-case route for the destination.
    assert(action.id:match("^[a-z][a-z0-9%-]*$"),
      "compose: action id must be lower case letters, digits and hyphens: " .. action.id)
    actions[#actions + 1] = string.format(
      '{"id":"%s","destination":"%s","kind":"%s"}',
      action.id, action.destination, action.kind)
  end

  local manifest = table.concat({
    '{"schema_version":1,"id":"' .. options.name .. '","title":"' .. (options.title or options.name) .. '",',
    '"inputs":[],"resources":[],',
    '"actions":[' .. table.concat(actions, ",") .. '],',
    '"capabilities":{"write_scene":true,"read_inputs":[],"read_theme":true}}',
  })
  local manifest_path = options.out_dir .. "/" .. options.name .. ".json"
  file = assert(io.open(manifest_path, "wb"), "cannot write " .. manifest_path)
  file:write(manifest)
  file:close()
  print(string.format("wrote %s (%d bytes) and %s", path, #source, manifest_path))
  return path, #source
end

return compose
