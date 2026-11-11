-- Compose the launcher into a single Canvas module.
--
--   lua tools/compose_home.lua "<Canvas runtime modules dir>" [snapshot.lua]
--
-- The snapshot comes from `soil-home snapshot`; without one the view falls
-- back to its fixture, which is what the headless test renders.

local script = arg[0] or "tools/compose_home.lua"
local spore = script:match("^(.*)/tools/[^/]+$") or "."
local compose = dofile(spore .. "/../../tools/compose_lua.lua")

local out_dir = assert(arg[1], "usage: compose_home.lua <modules dir> [snapshot]")

compose.install {
  name = "spore-dev-home",
  title = "Home",
  out_dir = out_dir,
  view = "ui/home.lua",
  snapshot = arg[2],
  libs = { ["fungos.ui"] = spore .. "/../../sdk/ui/ui.lua" },
  -- The launcher draws rows, cards and controls only, and the host's module
  -- bound is 64 KiB, so it composes without the dashboard section.
  ui_profile = "layout",
  -- A tile's action is a grant the manifest has to name, or the host replaces
  -- the surface with an error card. One intent: open the app this tile is.
  actions = {
    { id = "home-open", destination = "dev.fungos.home", kind = "intent" },
  },
}
