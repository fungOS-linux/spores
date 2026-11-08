-- Compose the worklog view into a single Canvas module.
--
--   lua tools/compose_worklog.lua "<Canvas runtime modules dir>" [snapshot] [--all]
--
-- The default snapshot is the committed fixture. Pass the output of
-- `worklog render-lua` to compose the real log instead:
--
--   ../worklog/target/release/worklog render-lua --log ~/.local/share/fungos/worklog.jsonl > /tmp/worklog.lua
--   lua tools/compose_worklog.lua "$MODULES" /tmp/worklog.lua

local script = arg[0] or "tools/compose_worklog.lua"
-- Resolve the repository root from wherever this was invoked: `<repo>/spores/<spore>/tools/`.
local spore = script:match("^(.*)/tools/[^/]+$") or "."
local compose = dofile(spore .. "/../../tools/compose_lua.lua")

local out_dir = assert(arg[1], "usage: compose_worklog.lua <modules dir> [snapshot]")

compose.install {
  name = "spore-dev-worklog",
  title = "Worklog",
  out_dir = out_dir,
  view = "ui/worklog.lua",
  snapshot = arg[2] or "examples/worklog_mock.lua",
  -- One UI kit, composed into every view rather than copied per Spore.
  ui_lib = spore .. "/../unibus/ui/lib/ui.lua",
  app_id = "dev.fungos.worklog",
}
