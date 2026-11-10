-- Compose the fleet view into the single Lua module Canvas loads.
--
--   lua tools/compose_fleet.lua "<Canvas runtime modules dir>" [snapshot]
--
-- The library is the Spore repository's `fungos.ui` package (`sdk/ui/ui.lua`),
-- named by the view through `require` and inlined here: Canvas strips `require`
-- and `package` from its sandbox, so the composer hands the view a `require`
-- whose one entry is that file.

local script = arg[0] or "tools/compose_fleet.lua"
local spore = script:match("^(.*)/tools/[^/]+$") or "."
local compose = dofile(spore .. "/../../tools/compose_lua.lua")

local out_dir = assert(arg[1], "usage: compose_fleet.lua <modules dir> [snapshot]")

compose.install {
  name = "spore-dev-fleet",
  title = "Fleet",
  out_dir = out_dir,
  view = spore .. "/ui/fleet.lua",
  snapshot = arg[2] or (spore .. "/examples/fleet_snapshot.lua"),
  libs = { ["fungos.ui"] = spore .. "/../../sdk/ui/ui.lua" },
  app_id = "dev.fungos.fleet",
  -- The host refuses a control whose action is not declared here, so the grant
  -- and the `hit_region` call have to name the same string. Every one is an
  -- intent addressed to this Spore; none of them are wired to a consumer yet.
  actions = {
    { id = "fleet-inspect-count", destination = "dev.fungos.fleet", kind = "intent" },
    { id = "fleet-inspect-peer", destination = "dev.fungos.fleet", kind = "intent" },
    { id = "fleet-filter-site", destination = "dev.fungos.fleet", kind = "intent" },
    { id = "fleet-inspect-release", destination = "dev.fungos.fleet", kind = "intent" },
    { id = "fleet-inspect-mesh", destination = "dev.fungos.fleet", kind = "intent" },
  },
}
