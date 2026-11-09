-- Deterministic fixtures for the Divebar view: one frame of ambient state.
-- Shapes mirror what the host composes from `soil:shell`, `soil:time` and
-- `soil:device`; nothing here is live and nothing here has authority.

return {
  workspaces = {
    { id = "1", occupied = true },
    { id = "2", focused = true, occupied = true },
    { id = "3", occupied = true },
    { id = "4" },
    { id = "5" },
  },
  active = {
    app = "canvas",
    title = "divebar · package check",
  },
  ambient = {
    clock = "13:17",
    network = "wi-fi",
    -- battery absent on a desktop: the view leaves no hole for it.
  },
}
