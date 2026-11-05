-- Installed Spore launcher.
local M = {}
M.name = "launcher"
M.version = "0.1.0"
local ui = assert(_G.ui, "ui library not composed")
local DEFAULT_THEME = { body = 0.145, title = 0.34, large = 0.24, micro = 0.085, radius = 0.02 }
local function theme() return (_G.module and _G.module.theme) or DEFAULT_THEME end
local function text(value, style, ink)
  return ui.text(tostring(value or ""), { style = style or "body", ink = ink or "text" })
end
function M.render(snapshot)
  snapshot = snapshot or {}
  local cards = {
    ui.column {
      gap = "xs",
      children = {
        text("Spore launcher", "title", "primary"),
        text("Installed packages and their launch surfaces", "caption", "muted"),
        ui.rule {},
      },
    },
  }
  for _, entry in ipairs(snapshot.entries or {}) do
    local workspace = entry.workspace or ""
    cards[#cards + 1] = ui.bar {
      pad = ui.gap.sm,
      ink = "surface",
      opacity = 0.7,
      radius = theme().radius,
      children = {
        ui.row {
          widths = { 0.78, 0.22 },
          children = {
            ui.column {
              gap = "none",
              children = {
                text(entry.name or entry.id, "heading", "primary"),
                text(entry.summary or "", "caption", "muted"),
              },
            },
            workspace ~= "" and ui.button("Open", { id = "workspace-" .. workspace }) or text("no workspace", "caption", "muted"),
          },
        },
      },
    }
  end
  ui.render(ui.column { h = 8, gap = "sm", pad = ui.gap.md, children = cards }, theme())
end
return M
