-- Landing queue. Finished work that is waiting on a merge decision.
--
-- A pure projection over the tracker's `review` state: the view ranks and
-- explains, and emits intents. It cannot merge, close, or reach a forge — v0
-- has no `soil:http` — because the work graph already has exactly one
-- authority and a window is not it.
--
-- Rows are ordered by cost of waiting, not by recency: a merged PR that is
-- still marked review is finished work sitting unlanded, which is the most
-- expensive thing on the list. The producer sorts; the view paints.

local ui = require("fungos.ui")
local action = require("fungos.action")

local M = { name = "dev.fungos.git", version = "0.1.0" }

local function clip(value, limit)
  value = tostring(value or "")
  if value == "" then return "" end
  if #value <= limit then return value end
  return value:sub(1, limit - 1) .. "…"
end

-- A badge pair that reads as one sentence: what the forge says, then how long
-- it has been true.
local function state_of(row)
  local pr = row.pr
  if not pr then return "no pr", "muted" end
  if pr.state == "merged" then return "merged", "primary" end
  if pr.state == "closed" then return "closed", "muted" end
  if pr.draft then return "draft", "muted" end
  return "open", "text"
end

local function row(node_id, row, selected)
  local state, state_ink = state_of(row)
  local where = row.pr
    and string.format("%s#%d", row.pr.repo, row.pr.number)
    or "no pull request"

  local children = {
    ui.column(node_id .. "-text", { flex_grow = 1, gap = 1 }, {
      ui.text(node_id .. "-title", clip(row.title, 72), {
        size = 12, height = 18, ink = "text",
      }),
      ui.text(node_id .. "-meta", where .. "  ·  " .. row.project, {
        size = 9, height = 14, ink = "muted",
      }),
    }),
    ui.badge(node_id .. "-age", row.age, { ink = "muted" }),
    ui.badge(node_id .. "-state", state, { ink = state_ink }),
  }

  -- The two decisions this queue exists for. `merged` means land it; anything
  -- else is still waiting, so the second button is the honest one to offer.
  if row.pr then
    children[#children + 1] = ui.button(node_id .. "-land",
      row.pr.state == "merged" and "Land" or "Review", {
        height = 26, size = 10,
        variant = row.pr.state == "merged" and "primary" or nil,
        action = action.intent("git.land", { id = row.id }),
      })
  end
  if row.marble then
    children[#children + 1] = ui.button(node_id .. "-marble", "Open", {
      height = 26, size = 10,
      action = action.intent("git.open_marble", { id = row.marble }),
    })
  end

  return ui.list_row(node_id, {
    height = 54,
    gap = 8,
    selected = selected,
    action = action.intent("git.select", { id = row.id }),
  }, children)
end

local function sidebar(view)
  local groups = {}
  for _, project in ipairs(view.projects or {}) do
    groups[#groups + 1] = {
      id = project.id,
      label = clip(project.label, 22),
      count = project.count,
      selected = project.selected,
      action = action.intent("git.filter", { project = project.id }),
    }
  end
  return ui.sidebar("git-side", { width = 210 }, {
    ui.sidebar_group("git-side-projects", "PROJECTS", groups, { height = 26 }),
  })
end

local function main(view)
  local counts = view.counts or {}
  local children = {
    ui.row("git-head", { height = 30, gap = 8, align_items = "center" }, {
      ui.text("git-title", view.title or "REVIEW", { size = 13, ink = "text", flex_grow = 1 }),
      ui.text("git-counts", string.format("%d waiting  ·  %d merged  ·  %d no pr",
        counts.waiting or 0, counts.merged or 0, counts.no_pr or 0), {
        size = 10, ink = "muted",
      }),
    }),
    ui.command_bar("git-bar", {
      query = view.query,
      placeholder = "Filter by repo, project, or age",
      hint = "⌘K",
    }),
    ui.section_label("git-section", "READY TO LAND"),
  }

  local rows = view.rows or {}
  if #rows == 0 then
    ui.push(children, ui.empty_state("git-empty", "Nothing is waiting",
      "Every review item has been landed or closed. New ones arrive when an agent opens a pull request."))
  else
    for index, item in ipairs(rows) do
      children[#children + 1] = row("git-row-" .. index, item,
        item.selected or item.id == view.selected_id)
    end
  end

  return ui.column("git-main", { flex_grow = 1, gap = 4, padding = { top = 10, right = 14, bottom = 10, left = 14 } }, children)
end

function M.render(ctx, view)
  view = view or {}
  return ui.row("git-root", { flex_grow = 1, gap = 6, padding = { top = 10, bottom = 12, left = 12, right = 12 } }, {
    sidebar(view),
    main(view),
  })
end

return M
