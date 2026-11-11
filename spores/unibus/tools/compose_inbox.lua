-- Compose the inbox views into single Canvas modules.
--
-- Canvas strips load/require/package from the sandbox, so the view library, the
-- view, and the mock snapshot are pre-composed. The rules live in
-- tools/compose_lua.lua; this file is the list of what to compose.
--
--   lua tools/compose_inbox.lua "<Canvas runtime modules dir>" [inbox snapshot] [contacts snapshot]

local script = arg[0] or "tools/compose_inbox.lua"
-- Resolve the repository root from wherever this was invoked: `<repo>/spores/<spore>/tools/`.
local spore = script:match("^(.*)/tools/[^/]+$") or "."
local compose = dofile(spore .. "/../../tools/compose_lua.lua")

local out_dir = assert(arg[1], "usage: compose_inbox.lua <modules dir>")
local ui_lib = "ui/lib/ui.lua"

local function install(name, title, view, snapshot, app_id)
  compose.install {
    name = name,
    title = title,
    out_dir = out_dir,
    view = view,
    snapshot = snapshot,
    ui_lib = ui_lib,
    app_id = app_id,
  }
end

install("spore-dev-inbox", "Inbox", "ui/inbox.lua", arg[2] or "examples/inbox_mock.lua", "dev.fungos.inbox")
install("spore-dev-approval2", "Approve reply", "ui/approval.lua", "examples/approval_mock.lua", "dev.fungos.inbox")
install("spore-dev-contacts", "Contacts", "ui/contacts.lua", arg[3] or "examples/contacts_mock.lua", "dev.fungos.contacts")
