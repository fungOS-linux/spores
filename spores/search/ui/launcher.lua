-- Quick-search view.
local ui = assert(_G.ui, "ui library not composed")

local M = {}
M.name = "search"
M.version = "0.2.0"
local DEFAULT_THEME = { body = 0.145, title = 0.34, large = 0.24, micro = 0.085,
 radius = 0.02 }
local function theme() return (_G.module and _G.module.theme) or DEFAULT_THEME end

-- A row is a label, not a sentence: `application` is the protocol's word for
-- the kind and `App` is the one that fits next to a title. The map lives in the
-- view because the kind is data and its width is a presentation choice.
local KIND = {
  application = "App", file = "File", recordentry = "Record",
  repository = "Repo", spore = "Spore", peer = "Peer", service = "Service",
  action = "Action", task = "Task", principal = "Person",
}
local function kind_label(kind)
  return KIND[(kind or "file"):lower()] or kind
end

-- The mark a row leads with. `soil:search` carries a character rather than an
-- asset, so a row stays recognisable on a machine with no icon theme; Canvas
-- resolves real artwork from the hit's id, not from this string.
local function mark(title, icon)
  if icon and icon ~= "" then return icon end
  local first = (title or "?"):sub(1, 1)
  if first == "" then return "?" end
  return first:upper()
end

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
            widths = { 0.07, 0.73, 0.2 },
            children = {
              ui.text(mark(hit.title, hit.icon), { style = "caption", ink = "muted" }),
              ui.column {
                gap = "none",
                children = {
                  ui.text(hit.title or "Untitled", { style = "body", ink = "primary" }),
                  (hit.subtitle and hit.subtitle ~= "") and ui.text(hit.subtitle, { style = "caption", ink = "muted" }) or ui.spacer { size = "xs" },
                },
              },
              ui.text(kind_label(hit.kind), { style = "caption", ink = "muted" }),
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
