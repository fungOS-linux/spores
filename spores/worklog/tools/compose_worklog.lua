-- Compose the worklog into a single Canvas module.
--
--   lua tools/compose_worklog.lua "<Canvas runtime modules dir>" [snapshot]
--
-- This exists rather than using `tend` because of one concrete failure: the
-- view calls `require("fungos.ui")`, and `tend`'s composed module resolves that
-- during its own preflight but not when Canvas loads it — the module then has no
-- `ui` at all and the surface dies with "attempt to index a nil value (global
-- 'ui')". Inlining the library as the view's `ui` upvalue, which is what this
-- composer does, has no such step to get wrong.
--
-- The layout kit is the one in `spores/unibus/ui/lib/ui.lua`: pure Lua, so it
-- works where Canvas provides no `canvas.layout`.

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
  ui_lib = spore .. "/../unibus/ui/lib/ui.lua",
  app_id = "dev.fungos.worklog",
  -- The one control this view emits. The host refuses a button whose action is
  -- not declared here, so the grant and the `press` call have to name the same
  -- string; `dev.fungos.worklog` is the Spore the intent is addressed to.
  actions = {
    { id = "worklog-open-locator", destination = "dev.fungos.worklog", kind = "intent" },
  },
}
