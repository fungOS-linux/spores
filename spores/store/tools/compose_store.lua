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
  app_id = "dev.fungos.store",
  -- The storefront draws rows, cards and controls, so it composes without the
  -- dashboard section: the host's module bound is 64 KiB and the whole kit
  -- pushes the artifact past it.
  ui_profile = "layout",
  -- Every control states an intent here, and the host refuses a control whose
  -- action it was not told about. The ids are the view's `ui.action` names.
  actions = {
    { id = "store-select", destination = "dev.fungos.store", kind = "intent" },
    { id = "store-source", destination = "dev.fungos.store", kind = "intent" },
    { id = "store-category", destination = "dev.fungos.store", kind = "intent" },
    { id = "store-back", destination = "dev.fungos.store", kind = "intent" },
    { id = "store-review", destination = "dev.fungos.store", kind = "intent" },
    { id = "store-install", destination = "dev.fungos.store", kind = "intent" },
    { id = "store-open", destination = "dev.fungos.store", kind = "intent" },
    { id = "store-update", destination = "dev.fungos.store", kind = "intent" },
    { id = "store-update-all", destination = "dev.fungos.store", kind = "intent" },
    { id = "store-unpin", destination = "dev.fungos.store", kind = "intent" },
  },
}
