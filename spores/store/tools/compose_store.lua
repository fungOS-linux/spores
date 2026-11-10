-- Compose the store into a single Canvas module.
--
--   lua tools/compose_store.lua "<Canvas runtime modules dir>" [snapshot.lua]
--
-- With no snapshot the view falls back to its own fixture. Pass the file
-- `soil-store snapshot` writes to show what is actually installed and what the
-- active source offers.

local script = arg[0] or "tools/compose_store.lua"
local spore = script:match("^(.*)/tools/[^/]+$") or "."
local compose = dofile(spore .. "/../../tools/compose_lua.lua")

local out_dir = assert(arg[1], "usage: compose_store.lua <modules dir> [snapshot]")

compose.install {
  name = "spore-dev-store",
  title = "Store",
  out_dir = out_dir,
  view = "ui/store.lua",
  snapshot = arg[2],
  libs = { ["fungos.ui"] = spore .. "/../../sdk/ui/ui.lua" },
}
