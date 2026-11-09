-- Compose the Rooms view into the single Lua module Canvas loads.
--
--   lua tools/compose_rooms.lua "<Canvas runtime modules dir>"
--
-- ui/snapshot.lua reads declared text chunks when a live host supplies them and
-- otherwise falls back to the reviewed fixture. `room-host` publishes the
-- chunks; the matching Canvas binder is installed beside the module.

-- Local controls stay Canvas-local. A click leaves a bounded request marker in
-- the scene; `room-host` reads it and owns the call process. No Unibus action
-- route is declared in the module.

local script = arg[0] or "tools/compose_rooms.lua"
local spore = script:match("^(.*)/tools/[^/]+$") or "."
local compose = dofile(spore .. "/../../tools/compose_lua.lua")

local out_dir = assert(arg[1], "usage: compose_rooms.lua <modules dir>")

local inputs = {}
for index = 1, 8 do
  inputs[#inputs + 1] = { id = "snapshot-" .. index, kind = "text" }
end

compose.install {
  name = "spore-dev-rooms",
  title = "Rooms",
  out_dir = out_dir,
  view = "ui/rooms.lua",
  snapshot = "ui/snapshot.lua",
  inputs = inputs,
  ui_profile = "layout",
  libs = { ["fungos.ui"] = spore .. "/../../sdk/ui/ui.lua" },
  app_id = "dev.fungos.rooms",
}

-- A live room host publishes one `canvas.widget.snapshot.v1` envelope. Canvas'
-- binder turns it into the module inputs above; the module itself never opens a
-- socket. Keep the binder next to the module so one compose command installs a
-- matched pair.
local runtime = out_dir:match("^(.*)/modules$")
assert(runtime, "modules directory must end in /modules")
local binder_dir = runtime .. "/binders"
os.execute(string.format("mkdir -p %q", binder_dir))
local binder_source = compose.read(spore .. "/tools/rooms-v1.lua")
local binder_path = binder_dir .. "/rooms-v1.lua"
local binder_file = assert(io.open(binder_path, "wb"), "cannot write " .. binder_path)
binder_file:write(binder_source)
binder_file:close()
print(string.format("wrote %s (%d bytes)", binder_path, #binder_source))
