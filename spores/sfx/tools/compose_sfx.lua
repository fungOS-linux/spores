-- Compose the SFX view with the SDK, not a private copy of the UI library.
--
--   FUNGOS_SDK=$HOME/src/fungOS/fungos-sdk \
--     lua tools/compose_sfx.lua /tmp/sfx-modules [snapshot.lua]
--
-- The SDK's `tend` inlines `fungos.ui` and writes the Canvas module plus its
-- manifest. The authored view stays the source; the composed file is an
-- artifact, not something to edit.

local sdk = os.getenv("FUNGOS_SDK") or ((os.getenv("HOME") or "") .. "/src/fungOS/fungos-sdk")
local out_dir = assert(arg[1], "usage: compose_sfx.lua <modules dir> [snapshot]")
local snapshot = arg[2] or "ui/board.example.lua"

local command = string.format(
  "lua %q build %q %q --sdk %q --name spore-dev-sfx --out %q",
  sdk .. "/tools/tend.lua", "ui/board.lua", snapshot, sdk, out_dir)
local ok = os.execute(command)
if ok ~= true and ok ~= 0 then
  os.exit(1)
end
