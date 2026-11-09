-- Compose the files view into a single Canvas module.
--
--   lua tools/compose_files.lua "<Canvas runtime modules dir>" [snapshot]
--
-- Canvas strips `require` and `package` from its sandbox, so a view authored as
-- two files has to arrive as one. `tools/compose_lua.lua` owns the rules; this
-- is the file list and the action catalog. `FUNGOS_SDK` points at a fungos-sdk
-- checkout; `~/src/fungos-sdk` is the default.
local function sdk_ui()
  local root = os.getenv("FUNGOS_SDK") or ((os.getenv("HOME") or "") .. "/src/fungos-sdk")
  local path = root .. "/lua/fungos/ui.lua"
  local handle = io.open(path, "rb")
  if not handle then
    io.stderr:write("compose_files: no fungos.ui at " .. path .. "\n"
      .. "  set FUNGOS_SDK to a fungos-sdk checkout (the one with lua/fungos/ui.lua)\n")
    os.exit(1)
  end
  handle:close()
  return path
end

local script = arg[0] or "tools/compose_files.lua"
local spore = script:match("^(.*)/tools/[^/]+$") or "."
local compose = dofile(spore .. "/../../tools/compose_lua.lua")

local out_dir = assert(arg[1], "usage: compose_files.lua <modules dir> [snapshot]")

compose.install {
  name = "spore-dev-files",
  title = "files",
  out_dir = out_dir,
  view = "ui/files.lua",
  snapshot = arg[2] or "examples/files_mock.lua",
  libs = { ["fungos.ui"] = sdk_ui() },
  app_id = "dev.fungos.files",
  -- The host refuses a button whose action is not declared here, so the grant
  -- and the `press` calls have to name the same two strings.
  actions = {
    { id = "files-open", destination = "dev.fungos.files", kind = "intent" },
    { id = "files-up", destination = "dev.fungos.files", kind = "intent" },
  },
}
