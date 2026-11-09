-- Compose the soundboard into a single Canvas module.
--
--   lua tools/compose_sfx.lua "<Canvas runtime modules dir>" [snapshot]
--
-- Run from this Spore's directory. `fungos.ui` is resolved here by name so the
-- embedded library is the package this repo declares, not whatever the
-- composing tool happens to ship.

local script = arg[0] or "tools/compose_sfx.lua"
local spore = script:match("^(.*)/tools/[^/]+$") or "."
local compose = dofile(spore .. "/../../tools/compose_lua.lua")

local out_dir = assert(arg[1], "usage: compose_sfx.lua <modules dir> [snapshot]")

-- Canvas grants the controls their routes. The ids are identifiers (hyphens,
-- no dots); the canonical Spore action travels in the body's `action` field.
local actions = {}
for _, name in ipairs({
  "trigger", "bind", "test", "enabled", "room", "stop", "cancel-bind",
}) do
  actions[#actions + 1] = {
    id = "sfx-" .. name,
    destination = "dev.fungos.sfx",
    kind = "dev.fungos.sfx." .. name .. ".v1",
  }
end

compose.install {
  name = "spore-dev-sfx",
  title = "SFX",
  out_dir = out_dir,
  view = "ui/board.lua",
  snapshot = arg[2] or "ui/board.example.lua",
  libs = { ["fungos.ui"] = spore .. "/../../sdk/ui/ui.lua" },
  actions = actions,
}
