-- Landing queue. Finished work that is waiting on a merge decision.
--
-- Built on the `fungos.ui` package in `sdk/ui` (see `tend.lua`): props-first nodes,
-- one-pass layout, and it paints itself through `ui.render`, so Canvas and
-- `tend --scene` see the same primitives.
--
-- A pure projection over the tracker's `review` state: the view ranks and
-- explains, and emits intents. It cannot merge, close, or reach a forge — v0
-- has no `soil:http` — because the work graph already has exactly one
-- authority and a window is not it.
--
-- Rows are ordered by cost of waiting, not by recency: a merged PR still marked
-- review is finished work sitting unlanded, which is the most expensive thing
-- on the list. The producer sorts; the view paints.

local ui = require("fungos.ui")

local M = { name = "dev.fungos.git", version = "0.1.0" }

local function clip(value, limit)
  value = tostring(value or "")
  if value == "" then return "" end
  if #value <= limit then return value end
  return value:sub(1, limit - 1) .. "…"
end

-- What the forge says, and the ink that says it. `merged` is the only state
-- that is landable, so it is the only one that reads as primary.
local function state_of(row)
  local pr = row.pr
  if not pr then return "no pr", "muted" end
  if pr.state == "merged" then return "merged", "primary" end
  if pr.state == "closed" then return "closed", "danger" end
  if pr.draft then return "draft", "muted" end
  return "open", "text"
end

local function row(item, index, selected)
  local id = "git-row-" .. index
  local state, state_ink = state_of(item)
  local where = item.pr
    and string.format("%s#%d", item.pr.repo, item.pr.number)
    or "no pull request"

  local actions = {}
  -- The two decisions this queue exists for. `merged` means land it; anything
  -- else is still waiting, so the second button is the honest one to offer.
  if item.pr then
    actions[#actions + 1] = ui.button(item.pr.state == "merged" and "Land" or "Review", {
      id = id .. "-land",
      small = true,
      quiet = item.pr.state ~= "merged",
      action = ui.action("git.land", { id = item.id }),
    })
  end
  if item.marble then
    actions[#actions + 1] = ui.button("Open", {
      id = id .. "-marble",
      small = true,
      quiet = true,
      action = ui.action("git.open_marble", { id = item.marble }),
    })
  end

  local second = { ui.text(where .. "  ·  " .. tostring(item.project), {
    id = id .. "-meta", style = "caption", ink = "muted", grow = true,
  }) }
  for _, action in ipairs(actions) do second[#second + 1] = action end

  return ui.card({
    id = id,
    pad = "sm",
    gap = "xs",
    selected = selected,
    action = ui.action("git.select", { id = item.id }),
    children = {
      ui.row({
        id = id .. "-top",
        gap = "sm",
        children = {
          ui.text(clip(item.title, 68), {
            id = id .. "-title", style = "body",
            -- Selection has to be visible in what is painted: `selected` on the
            -- card is layout metadata the library does not colour.
            ink = selected and "primary" or "text", grow = true,
          }),
          ui.text(tostring(item.age or ""), {
            id = id .. "-age", style = "caption", ink = "muted",
          }),
          ui.text(state, {
            id = id .. "-state", style = "caption", ink = state_ink,
          }),
        },
      }),
      ui.row({ id = id .. "-bottom", gap = "sm", children = second }),
    },
  })
end

local function header(view)
  local counts = view.counts or {}
  return ui.row({
    id = "git-head",
    gap = "sm",
    children = {
      ui.text(view.title or "REVIEW", {
        id = "git-title", style = "heading", ink = "text", grow = true,
      }),
      ui.text(string.format("%d waiting  ·  %d merged  ·  %d no pr",
        counts.waiting or 0, counts.merged or 0, counts.no_pr or 0), {
        id = "git-counts", style = "caption", ink = "muted",
      }),
    },
  })
end

local function projects(view)
  local children = {
    ui.text("PROJECTS", { id = "git-side-label", style = "caption", ink = "muted" }),
  }
  for index, project in ipairs(view.projects or {}) do
    children[#children + 1] = ui.button(clip(project.label or project.id, 22) .. "  " .. tostring(project.count or 0), {
      id = "git-side-" .. index,
      small = true,
      quiet = not project.selected,
      action = ui.action("git.filter", { project = project.id }),
    })
  end
  return ui.column({ id = "git-side", gap = "xs", children = children })
end

function M.render(a, b)
  -- Accept both `render(view)` and the `render(ctx, view)` shape the other
  -- Spore views use, so the caller does not have to know which library a view
  -- was written against.
  local view = b or a or {}
  local theme = view.theme or (module and module.theme) or {}

  local body = {
    header(view),
    ui.rule({ id = "git-rule" }),
    ui.text("READY TO LAND", { id = "git-section", style = "caption", ink = "muted" }),
  }

  local rows = view.rows or {}
  if #rows == 0 then
    body[#body + 1] = ui.text(
      "Nothing is waiting. New reviews arrive when an agent opens a pull request.",
      { id = "git-empty", style = "body", ink = "muted" })
  else
    for index, item in ipairs(rows) do
      body[#body + 1] = row(item, index, item.selected or item.id == view.selected_id)
    end
  end

  local tree = ui.row({
    id = "git-root",
    gap = "md",
    pad = "md",
    children = {
      projects(view),
      ui.column({ id = "git-main", gap = "sm", grow = true, children = body }),
    },
  })

  ui.render(tree, theme)
end

return M
