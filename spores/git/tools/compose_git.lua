-- Compose the landing queue into a single Canvas module.
--
--   lua tools/compose_git.lua "<Canvas runtime modules dir>" [snapshot]
--
-- The view is written against the `fungos.ui` package in `sdk/ui`, which is
-- declared by this repo's `tend.lua` and shipped as `sdk/ui/package.toml`. The
-- tool that composes spores also ships a `fungos.ui` of its own with an
-- incompatible, id-first API, so `require("fungos.ui")` is resolved here by
-- name and not left to the tool: whatever is embedded has to be the package
-- this repo pins, or the view runs against a library neither of us chose.

local script = arg[0] or "tools/compose_git.lua"
local spore = script:match("^(.*)/tools/[^/]+$") or "."
local compose = dofile(spore .. "/../../tools/compose_lua.lua")

local out_dir = assert(arg[1], "usage: compose_git.lua <modules dir> [snapshot]")

compose.install {
  name = "spore-dev-git",
  title = "Review",
  out_dir = out_dir,
  view = "ui/review.lua",
  snapshot = arg[2] or "ui/view.example.lua",
  libs = { ["fungos.ui"] = spore .. "/../../sdk/ui/ui.lua" },
}
