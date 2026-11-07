-- Quick-search view.
local ui = assert(_G.ui, "ui library not composed")

local M = {}
M.name = "search"
M.version = "0.1.0"
local DEFAULT_THEME = { body = 0.145, title = 0.34, large = 0.24, micro = 0.085,
 radius = 0.02 }
local function theme() return (_G.module and _G.module.theme) or DEFAULT_THEME end

function M.render(snapshot)
  snapshot = snapshot or {}
  local query = snapshot.query or ""
  local hits = snapshot.hits or {}

  local rows = {}
  if #hits == 0 then
    rows[#rows + 1] = ui.text("No results", { style = "caption", ink = "muted" })
  else
    for i, hit in ipairs(hits) do
      rows[#rows + 1] = ui.bar {
        id = "hit-" .. (hit.id or i),
        pad = ui.gap.sm,
        ink = "surface",
        opacity = 0.7,
        radius = "sm",
        children = {
          ui.row {
            widths = { 0.1, 0.7, 0.2 },
            children = {
              ui.text(hit.kind or "file", { style = "caption", ink = "muted" }),
              ui.column {
                gap = "none",
                children = {
                  ui.text(hit.title or "Untitled", { style = "body", ink = "primary" }),
                  (hit.subtitle and hit.subtitle ~= "") and ui.text(hit.subtitle, { style = "caption", ink = "muted" }) or ui.spacer { size = "xs" },
                },
              },
              ui.text(tostring(hit.source or ""), { style = "caption", ink = "muted" }),
            },
          },
        },
      }
    end
  end

  ui.render(
    ui.column {
      h = 8,
      gap = "sm",
      pad = ui.gap.md,
      children = rows,
    },
    theme()
  )
end

return M
