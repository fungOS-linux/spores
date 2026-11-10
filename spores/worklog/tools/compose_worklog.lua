-- Compose the worklog into a single Canvas module.
--
--   lua tools/compose_worklog.lua "<Canvas runtime modules dir>" [snapshot]
--
-- This exists rather than using `tend` because of one concrete failure: the
-- view calls `require("fungos.ui")`, and `tend`'s composed module resolves that
-- during its own preflight but not when Canvas loads it — the module then has no
-- `ui` at all and the surface dies with "attempt to index a nil value (global
-- 'ui')". Resolving it once, at compose time, has no such step to get wrong.
--
-- The library is the SDK's `fungos.ui`, named by the view through `require` and
-- resolved here: Canvas strips `require` and `package` from its sandbox, so the
-- composer hands the view a `require` whose one entry is this file.
-- `FUNGOS_SDK` points at a checkout of fungos-sdk; `~/src/fungos-sdk` is the
-- default.
local function sdk_ui()
  local root = os.getenv("FUNGOS_SDK") or ((os.getenv("HOME") or "") .. "/src/fungos-sdk")
  local path = root .. "/lua/fungos/ui.lua"
  local handle = io.open(path, "rb")
  if not handle then
    io.stderr:write("compose_worklog: no fungos.ui at " .. path .. "\n"
      .. "  set FUNGOS_SDK to a fungos-sdk checkout (the one with lua/fungos/ui.lua)\n")
    os.exit(1)
  end
  handle:close()
  return path
end

local script = arg[0] or "tools/compose_worklog.lua"
local spore = script:match("^(.*)/tools/[^/]+$") or "."
local compose = dofile(spore .. "/../../tools/compose_lua.lua")

local out_dir = assert(arg[1], "usage: compose_worklog.lua <modules dir> [snapshot]")

compose.install {
  name = "spore-dev-worklog",
  title = "Worklog",
  out_dir = out_dir,
  view = "ui/worklog.lua",
  snapshot = arg[2] or "examples/worklog_mock.lua",
  libs = { ["fungos.ui"] = sdk_ui() },
  app_id = "dev.fungos.worklog",
  -- The one control this view emits. The host refuses a button whose action is
  -- not declared here, so the grant and the `press` call have to name the same
  -- string; `dev.fungos.worklog` is the Spore the intent is addressed to.
  actions = {
    { id = "worklog-open-locator", destination = "dev.fungos.worklog", kind = "intent" },
  },
}
