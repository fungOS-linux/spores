-- CRM snapshot UI. Pure projection over the host-provided snapshot; emits action
-- intents only. The host owns data access and namespaces.
--
-- The view builds a `fungos.ui` tree and hands it to `ui.present`, which lays it
-- out (host Taffy when there is one, the kit's own pass when there is not) and
-- paints it. Returning the tree instead would paint nothing: a host records
-- primitives, it does not mount a tree.
local ui = require("fungos.ui")
local action = require("fungos.action")

local M = { name = "dev.fungos.crm", version = "0.1.0" }

local function clip(value, limit)
  value = tostring(value or "")
  if #value <= limit then return value end
  return value:sub(1, limit - 1) .. "…"
end

local function selected_account(snapshot)
  for _, account in ipairs(snapshot.accounts or {}) do
    if account.selected or account.id == snapshot.selected then return account end
  end
  return (snapshot.accounts or {})[1]
end

local function account_rail(snapshot)
  local children = {
    ui.text("crm-label", "ACCOUNTS", { size = 9, height = 16, ink = "muted" }),
  }
  for index, account in ipairs(snapshot.accounts or {}) do
    ui.push(children, ui.button("account-" .. index, clip(account.name or account.id, 28), {
      height = 28,
      size = 10,
      variant = account.selected and "primary" or nil,
      action = action.intent("crm.account", { id = account.id }),
    }))
  end
  if #(snapshot.accounts or {}) == 0 then
    ui.push(children, ui.text("crm-empty", "No accounts found.", {
      size = 10, height = 24, ink = "muted",
    }))
  end
  return ui.panel("crm-rail", { width = 240, flex_shrink = 0, gap = 2, padding = 12 }, children)
end

local function detail(snapshot)
  local account = snapshot.account or selected_account(snapshot) or {}
  local children = {
    ui.text("crm-title", account.name or "CRM", { size = 22, height = 30, ink = "primary" }),
    ui.text("crm-subtitle", table.concat(account.domains or {}, ", "), {
      size = 10, height = 18, ink = "muted",
    }),
  }
  for index, contact in ipairs(account.contacts or {}) do
    ui.push(children, ui.text("crm-contact-" .. index,
      tostring(contact.name or contact.contact_token or "") .. " · " .. tostring(contact.role or ""), {
        size = 11, height = 22, wrap = true,
      }))
  end
  for index, event in ipairs(account.timeline or snapshot.events or {}) do
    ui.push(children, ui.text("crm-event-" .. index,
      tostring(event.kind or "event") .. " · " .. tostring(event.title or ""), {
        size = 10, height = 22, ink = "muted", wrap = true,
      }))
  end
  return ui.column("crm-detail", { flex_grow = 1, gap = 3 }, children)
end

local function money(amount)
  local value = tonumber(amount) or 0
  if value >= 1000000 then return string.format("$%.1fM", value / 1000000) end
  if value >= 1000 then return string.format("$%.1fK", value / 1000) end
  return string.format("$%d", value)
end

local function pipeline_board(snapshot)
  local pipeline = snapshot.pipeline or {}
  local columns = {}
  for stage_index, stage in ipairs(pipeline.stages or {}) do
    local children = {
      ui.text("crm-stage-" .. stage_index, string.upper(tostring(stage.stage or "stage")), {
        size = 9, height = 16, ink = "muted",
      }),
      ui.text("crm-stage-total-" .. stage_index,
        tostring(stage.count or 0) .. " · " .. money(stage.amount), {
          size = 10, height = 18, ink = "muted",
        }),
    }
    for card_index, opportunity in ipairs(stage.opportunities or {}) do
      -- One line: `canvas.text` carries no width and does not honour a newline,
      -- so a three-line label would paint as one long string across the column.
      ui.push(children, ui.button("crm-opp-" .. stage_index .. "-" .. card_index,
        tostring(opportunity.name or opportunity.id) .. " · "
          .. tostring(opportunity.account_name or "") .. " · "
          .. money(opportunity.amount), {
          height = 64,
          size = 10,
          action = action.intent("crm.opportunity", { id = opportunity.id }),
        }))
    end
    ui.push(columns, ui.panel("crm-stage-col-" .. stage_index, {
      width = 220, flex_shrink = 0, gap = 2, padding = 10,
    }, children, { opacity = 0.24 }))
  end
  if #columns == 0 then
    ui.push(columns, ui.text("crm-pipeline-empty", "No pipeline stages.", {
      size = 12, ink = "muted",
    }))
  end
  return ui.column("crm-pipeline-root", { flex_grow = 1, gap = 12, padding = 16 }, {
    ui.text("crm-pipeline-title", "Pipeline", { size = 22, height = 30, ink = "primary" }),
    ui.row("crm-pipeline-board", { flex_grow = 1, gap = 16 }, columns),
  })
end

function M.render(snapshot)
  local tree
  if snapshot.mode == "pipeline" then
    tree = pipeline_board(snapshot)
  else
    tree = ui.row("crm-root", { flex_grow = 1, gap = 16, padding = 16 }, {
      account_rail(snapshot),
      detail(snapshot),
    })
  end
  M.hits = ui.present(tree)
end

return M
