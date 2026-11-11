-- Pulse console: crons on a schedule, and the webhooks that arrive unasked.
--
-- A pure projection over the snapshot `pulse snapshot` emits. Every control is
-- an intent (`{ kind = ... }`), never a callback; the host applies it with the
-- same reducer the CLI uses and re-renders. Nothing here reads a clock, a file
-- or a socket.
local ui = require("fungos.ui")

-- Canvas's `widget` table has no `hit` today (docs/interaction-gap.md). The
-- SDK's `ui.present` registers a hit target for every control and calls it
-- unconditionally, so a view with buttons dies on the one host that exists.
-- Give that host a sink: the specs are still returned in `M.hits`, so a host
-- that can route a press still receives every one of them. Remove this when
-- `widget.hit` lands in Canvas.
local function ensure_hit_sink()
  local host = rawget(_G, "widget")
  if type(host) == "table" and type(host.hit) ~= "function" then
    host.hit = function() end
  end
end

local M = { name = "dev.fungos.pulse", version = "0.1.0" }

local CRON_INK = {
  ok = "success", idle = "text", paused = "muted", overdue = "danger",
  failing = "danger", invalid = "danger", unschedulable = "danger",
}
local CRON_GLYPH = {
  ok = "●", idle = "○", paused = "○", overdue = "▲",
  failing = "✕", invalid = "!", unschedulable = "!",
}
local HOOK_INK = {
  healthy = "success", unknown = "muted", degraded = "warning",
  failing = "danger", silent = "muted",
}
local HOOK_GLYPH = {
  healthy = "●", unknown = "○", degraded = "▲", failing = "✕", silent = "○",
}

local function intent(kind, fields)
  local out = { kind = kind }
  for key, value in pairs(fields or {}) do out[key] = value end
  return out
end

local function clip(value, limit)
  return ui.clip(value or "", limit)
end

local function viewport()
  local m = rawget(_G, "module")
  local vp = m and m.viewport
  return (vp and vp.width) or 1182, (vp and vp.height) or 1569
end

local function header(snapshot)
  local summary = snapshot.summary or {}
  local crons = string.format("%d crons · %d overdue · %d failing · %d paused",
    summary.crons or 0, summary.overdue or 0, summary.failing or 0, summary.paused or 0)
  local hooks = string.format("%d hooks · %d failing · %d degraded",
    summary.hooks or 0, summary.hooks_failing or 0, summary.hooks_degraded or 0)
  return ui.row("pulse-header", { height = 34, gap = 10 }, {
    ui.text("pulse-title", snapshot.title or "PULSE", { size = 17, ink = "primary", width = 84 }),
    ui.text("pulse-cron-facts", crons, { size = 10, ink = "muted", width = 250 }),
    ui.text("pulse-hook-facts", hooks, {
      size = 10,
      ink = (summary.hooks_failing or 0) > 0 and "danger" or "muted",
      width = 210,
    }),
    ui.spacer("pulse-header-gap", { flex_grow = 1 }),
    ui.text("pulse-now", "UTC " .. tostring(snapshot.now or ""), { size = 10, ink = "muted", width = 200 }),
  })
end

local function tabs(snapshot)
  local summary = snapshot.summary or {}
  local ingress = snapshot.tab == "ingress"
  return ui.tabbar("pulse-tabs", {
    {
      id = "crons",
      label = "CRONS",
      badge = summary.crons or 0,
      selected = not ingress,
      action = intent("tab", { tab = "crons" }),
    },
    {
      id = "ingress",
      label = "INGRESS",
      badge = summary.hooks or 0,
      selected = ingress,
      action = intent("tab", { tab = "ingress" }),
    },
  }, { height = 30, gap = 4 })
end

local function cron_row(cron, index, snapshot)
  local id = "pulse-cron-" .. index
  return ui.list_row(id, {
    height = 56,
    selected = cron.id == snapshot.selected,
    action = intent("select", { id = cron.id }),
  }, {
    ui.text(id .. "-glyph", CRON_GLYPH[cron.health] or "○", {
      size = 11, width = 14, ink = CRON_INK[cron.health] or "muted",
    }),
    ui.column(id .. "-body", { flex_grow = 1, gap = 1 }, {
      ui.text(id .. "-name", clip(cron.name, 24), { size = 12, ink = "text" }),
      ui.text(id .. "-schedule", clip(cron.schedule, 28), { size = 9, ink = "muted" }),
    }),
    ui.column(id .. "-state", { width = 92, gap = 1 }, {
      ui.text(id .. "-health", clip(cron.health_text, 14), {
        size = 9, ink = CRON_INK[cron.health] or "muted",
      }),
      ui.text(id .. "-next", clip(cron.next_text, 12), { size = 9, ink = "muted" }),
    }),
  })
end

local function cron_list(snapshot)
  local children = {
    ui.section_label("pulse-crons-label", string.format("CRONS  %d", #(snapshot.crons or {})), {}),
  }
  for index, cron in ipairs(snapshot.crons or {}) do
    ui.push(children, cron_row(cron, index, snapshot))
  end
  if #(snapshot.crons or {}) == 0 then
    ui.push(children, ui.empty_state("pulse-crons-empty", "No crons",
      "Pulse writes pulse.cron records; `pulse set` creates one.", { detail_height = 34 }))
  end
  return ui.panel("pulse-crons-panel", { width = 320, gap = 2, padding = 12 }, children,
    { opacity = 0.3 })
end

local function hook_row(hook, index, snapshot)
  local id = "pulse-hook-" .. index
  return ui.list_row(id, {
    height = 56,
    selected = hook.slug == snapshot.selected_hook,
    action = intent("select-hook", { slug = hook.slug }),
  }, {
    ui.text(id .. "-glyph", HOOK_GLYPH[hook.health] or "○", {
      size = 11, width = 14, ink = HOOK_INK[hook.health] or "muted",
    }),
    ui.column(id .. "-body", { flex_grow = 1, gap = 1 }, {
      ui.text(id .. "-name", clip(hook.name, 24), { size = 12, ink = "text" }),
      ui.text(id .. "-route", tostring(hook.provider) .. " · " .. clip(hook.tenant, 12), {
        size = 9, ink = "muted",
      }),
    }),
    ui.column(id .. "-state", { width = 92, gap = 1 }, {
      ui.text(id .. "-health", clip(hook.health_text, 14), {
        size = 9, ink = HOOK_INK[hook.health] or "muted",
      }),
      ui.text(id .. "-counts", string.format("%d ok · %d bad", hook.accepted_24h or 0, hook.rejected_24h or 0), {
        size = 9, ink = "muted",
      }),
    }),
  })
end

local function hook_list(snapshot)
  local children = {
    ui.section_label("pulse-hooks-label", string.format("WEBHOOKS  %d", #(snapshot.hooks or {})), {}),
  }
  for index, hook in ipairs(snapshot.hooks or {}) do
    ui.push(children, hook_row(hook, index, snapshot))
  end
  if #(snapshot.hooks or {}) == 0 then
    ui.push(children, ui.empty_state("pulse-hooks-empty", "No hooks",
      "The Confluence ingress writes one confluence.hook record per endpoint.", { detail_height = 34 }))
  end
  return ui.panel("pulse-hooks-panel", { width = 320, gap = 2, padding = 12 }, children,
    { opacity = 0.3 })
end

local function selected_cron(snapshot)
  for _, cron in ipairs(snapshot.crons or {}) do
    if cron.id == snapshot.selected then return cron end
  end
  return (snapshot.crons or {})[1]
end

local function problem_rows(cron)
  local rows = {}
  for index, problem in ipairs(cron.problems or {}) do
    ui.push(rows, ui.text("pulse-problem-" .. index,
      tostring(problem.field) .. ": " .. tostring(problem.text),
      { size = 10, ink = "danger", height = 16 }))
  end
  return ui.column("pulse-problems", { height = #rows * 16, gap = 0 }, rows)
end

local function step_button(id, label, cron, field, delta)
  local editable = cron.editable or {}
  if editable[field] ~= true then
    return ui.text(id, label, { size = 10, ink = "muted", width = 52, height = 28 })
  end
  return ui.action_button(id, label, {
    width = 52,
    height = 28,
    size = 11,
    action = intent("step", { id = cron.id, field = field, delta = delta }),
  })
end

local function adjust_row(cron)
  local id = "pulse-adjust"
  local editable = cron.editable or {}
  return ui.row(id, { height = 28, gap = 6 }, {
    ui.section_label(id .. "-label", "ADJUST", { width = 66 }),
    ui.text(id .. "-note", editable.minute and editable.hour and "step the hour and minute"
      or "this schedule is a list; pick a preset", { size = 9, ink = "muted", flex_grow = 1 }),
    step_button(id .. "-hm", "−1h", cron, "hour", -1),
    step_button(id .. "-mm", "−1m", cron, "minute", -1),
    step_button(id .. "-pm", "+1m", cron, "minute", 1),
    step_button(id .. "-ph", "+1h", cron, "hour", 1),
  })
end

local function weekday_row(cron)
  local id = "pulse-weekdays"
  local children = { ui.section_label(id .. "-label", "DAYS", { width = 66 }) }
  for _, day in ipairs(cron.weekdays or {}) do
    ui.push(children, ui.action_button(id .. "-" .. day.day, day.label, {
      width = 30,
      height = 26,
      size = 11,
      variant = day.on and "primary" or nil,
      action = intent("weekday", { id = cron.id, day = day.day }),
    }))
  end
  return ui.row(id, { height = 26, gap = 4 }, children)
end

local function preset_row(snapshot, cron)
  local id = "pulse-presets"
  local children = { ui.section_label(id .. "-label", "PRESETS", { width = 66 }) }
  for _, preset in ipairs(snapshot.presets or {}) do
    ui.push(children, ui.action_button(id .. "-" .. preset.id, preset.label, {
      height = 26,
      size = 10,
      variant = preset.active and "primary" or nil,
      action = intent("preset", { id = cron.id, preset = preset.id }),
    }))
  end
  return ui.row(id, { height = 26, gap = 6 }, children)
end

local function target_row(snapshot, cron, width)
  local id = "pulse-targets"
  -- Five buttons do not fit beside a list in a narrow window. Chunking is the
  -- layout's job, not the reader's: the same buttons, two rows.
  local per_row = width >= 1100 and 5 or 3
  local rows, current = {}, {}
  for index, target in ipairs(snapshot.targets or {}) do
    ui.push(current, ui.action_button(id .. "-" .. index, clip(target.label, 20), {
      height = 26,
      size = 10,
      variant = target.selected and "primary" or nil,
      action = intent("target", { id = cron.id, target = target.id }),
    }))
    if #current == per_row then
      ui.push(rows, ui.row(id .. "-row-" .. #rows, { height = 26, gap = 6 }, current))
      current = {}
    end
  end
  if #current > 0 then
    ui.push(rows, ui.row(id .. "-row-" .. #rows, { height = 26, gap = 6 }, current))
  end
  local children = { ui.section_label(id .. "-label", "TARGET", { height = 16 }) }
  for _, row in ipairs(rows) do ui.push(children, row) end
  return ui.column(id, { height = 16 + #rows * 30, gap = 4 }, children)
end

local function action_row(cron)
  local id = "pulse-actions"
  return ui.row(id, { height = 32, gap = 8 }, {
    ui.section_label(id .. "-label", "STATE", { width = 66 }),
    ui.action_button(id .. "-toggle", cron.enabled and "Pause" or "Resume", {
      height = 30,
      size = 11,
      action = intent("toggle", { id = cron.id }),
    }),
    ui.spacer(id .. "-spacer", { flex_grow = 1 }),
    ui.action_button(id .. "-delete", "Delete", {
      height = 30,
      size = 11,
      danger = true,
      confirm = true,
      action = intent("delete", { id = cron.id }),
    }),
  })
end

local function runs(cron)
  local id = "pulse-runs"
  local last = "no runs recorded yet"
  if cron.last then
    last = "last " .. tostring(cron.last.text) .. " · " .. tostring(cron.last.outcome)
    if cron.last.detail and cron.last.detail ~= "" then
      last = last .. " · " .. clip(cron.last.detail, 70)
    end
  end
  local children = {
    ui.row(id .. "-head", { height = 18, gap = 10 }, {
      ui.section_label(id .. "-label", "RECENT RUNS", { width = 100 }),
      ui.text(id .. "-last", last, { size = 9, ink = "muted", flex_grow = 1 }),
    }),
  }
  for index, run in ipairs(cron.runs or {}) do
    local outcome = tostring(run.outcome or "")
    local ink = "muted"
    if outcome == "ok" then ink = "success" elseif outcome == "failed" then ink = "danger" end
    ui.push(children, ui.row(id .. "-" .. index, { height = 18, gap = 10 }, {
      ui.text(id .. "-" .. index .. "-when", tostring(run.text or ""), {
        size = 9, ink = "muted", width = 90,
      }),
      ui.text(id .. "-" .. index .. "-outcome", string.upper(outcome), {
        size = 9, ink = ink, width = 66,
      }),
      ui.text(id .. "-" .. index .. "-detail", clip(run.detail, 96), {
        size = 9, ink = "muted", flex_grow = 1,
      }),
    }))
  end
  return ui.column(id, { gap = 2 }, children)
end

local function cron_detail(snapshot, width)
  local cron = selected_cron(snapshot)
  if not cron then
    return ui.panel("pulse-detail-panel", { flex_grow = 1, gap = 8, padding = 14 }, {
      ui.empty_state("pulse-detail-empty", "No cron selected",
        "Pulse crons arrive as pulse.cron records.", {}),
    }, { opacity = 0.24 })
  end
  local id = "pulse-detail"
  local enabled = cron.enabled and "enabled" or "paused"
  local children = {
    ui.row(id .. "-head", { height = 30, gap = 10 }, {
      ui.text(id .. "-name", clip(cron.name, 40), { size = 16, ink = "primary", flex_grow = 1 }),
      ui.text(id .. "-state", string.upper(enabled), { size = 9, ink = "muted", width = 60 }),
      ui.text(id .. "-health", string.upper(clip(cron.health_text, 18)), {
        size = 10, ink = CRON_INK[cron.health] or "muted", width = 120,
      }),
    }),
    ui.rule(id .. "-rule-a", { height = 1 }),
    ui.row(id .. "-schedule", { height = 46, gap = 14 }, {
      ui.column(id .. "-schedule-text", { flex_grow = 1, gap = 2 }, {
        ui.text(id .. "-schedule-label", "SCHEDULE", { size = 9, ink = "muted" }),
        ui.text(id .. "-schedule-value", tostring(cron.schedule), { size = 15, ink = "primary" }),
        ui.text(id .. "-target-value", "target " .. clip(cron.target, 40), { size = 9, ink = "muted" }),
      }),
      ui.column(id .. "-schedule-when", { width = 340, gap = 2 }, {
        ui.text(id .. "-description", clip(cron.description, 52), { size = 10, ink = "text" }),
        ui.text(id .. "-next", "next " .. tostring(cron.next_iso or "never")
          .. " · " .. tostring(cron.next_text), { size = 9, ink = "muted" }),
        ui.text(id .. "-prev", cron.prev_iso and ("due " .. tostring(cron.prev_iso)) or "", {
          size = 9, ink = "muted",
        }),
      }),
    }),
    problem_rows(cron),
    ui.rule(id .. "-rule-b", { height = 1 }),
    adjust_row(cron),
    weekday_row(cron),
    preset_row(snapshot, cron),
    target_row(snapshot, cron, width),
    action_row(cron),
    ui.rule(id .. "-rule-c", { height = 1 }),
    runs(cron),
  }
  return ui.panel("pulse-detail-panel", { flex_grow = 1, gap = 8, padding = 14 }, children,
    { opacity = 0.24 })
end

local function selected_hook(snapshot)
  for _, hook in ipairs(snapshot.hooks or {}) do
    if hook.slug == snapshot.selected_hook then return hook end
  end
  return (snapshot.hooks or {})[1]
end

local rejections, deliveries

local function hook_detail(snapshot)
  local hook = selected_hook(snapshot)
  if not hook then
    return ui.panel("pulse-ingress-panel", { flex_grow = 1, gap = 8, padding = 14 }, {
      ui.empty_state("pulse-ingress-empty", "No webhook selected",
        "The Confluence ingress writes confluence.hook records.", {}),
    }, { opacity = 0.24 })
  end
  local id = "pulse-ingress"
  local last = "no delivery recorded"
  if hook.last then
    last = "last " .. tostring(hook.last.text) .. " · " .. tostring(hook.last.status)
    if hook.last.reason and hook.last.reason ~= "" then
      last = last .. " · " .. clip(hook.last.reason, 60)
    end
  end
  local children = {
    ui.row(id .. "-head", { height = 30, gap = 10 }, {
      ui.text(id .. "-name", clip(hook.name, 40), { size = 16, ink = "primary", flex_grow = 1 }),
      ui.text(id .. "-health", string.upper(clip(hook.health_text, 18)), {
        size = 10, ink = HOOK_INK[hook.health] or "muted", width = 140,
      }),
    }),
    ui.row(id .. "-route", { height = 20, gap = 14 }, {
      ui.text(id .. "-provider", "provider " .. tostring(hook.provider), { size = 10, ink = "text", width = 160 }),
      ui.text(id .. "-tenant", "tenant " .. tostring(hook.tenant), { size = 10, ink = "muted", width = 160 }),
      ui.text(id .. "-slug", "slug " .. tostring(hook.slug), { size = 10, ink = "muted", flex_grow = 1 }),
    }),
    ui.row(id .. "-meta", { height = 20, gap = 14 }, {
      ui.text(id .. "-kind", "kind " .. clip(hook.kind, 34), { size = 9, ink = "muted", flex_grow = 1 }),
      ui.text(id .. "-credential", "credential " .. clip(hook.credential, 24),
        { size = 9, ink = "muted", width = 260 }),
      ui.text(id .. "-created", hook.created_text ~= "" and ("created " .. tostring(hook.created_text)) or "",
        { size = 9, ink = "muted", width = 220 }),
    }),
    ui.rule(id .. "-rule-a", { height = 1 }),
    ui.row(id .. "-counts", { height = 26, gap = 16 }, {
      ui.section_label(id .. "-counts-label", "24 HOURS", { width = 80 }),
      ui.text(id .. "-accepted", string.format("%d accepted", hook.accepted_24h or 0),
        { size = 11, ink = "success", width = 110 }),
      ui.text(id .. "-rejected", string.format("%d rejected", hook.rejected_24h or 0),
        { size = 11, ink = (hook.rejected_24h or 0) > 0 and "danger" or "muted", width = 110 }),
      ui.text(id .. "-total", string.format("%d total", (hook.accepted_total or 0) + (hook.rejected_total or 0)),
        { size = 9, ink = "muted", width = 90 }),
      ui.text(id .. "-last", last, { size = 9, ink = "muted", flex_grow = 1 }),
    }),
    ui.section_label(id .. "-reasons-label", "REJECTIONS", { height = 18 }),
    rejections(hook),
    ui.rule(id .. "-rule-b", { height = 1 }),
    ui.section_label(id .. "-deliveries-label", "DELIVERIES", { height = 18 }),
    deliveries(hook),
  }
  return ui.panel("pulse-ingress-panel", { flex_grow = 1, gap = 8, padding = 14 }, children,
    { opacity = 0.24 })
end

function rejections(hook)
  local id = "pulse-reasons"
  local children = {}
  for index, reason in ipairs(hook.reasons or {}) do
    ui.push(children, ui.row(id .. "-" .. index, { height = 18, gap = 10 }, {
      ui.text(id .. "-" .. index .. "-count", string.format("%d×", reason.count or 0),
        { size = 9, ink = "danger", width = 44 }),
      ui.text(id .. "-" .. index .. "-text", clip(reason.text, 110),
        { size = 9, ink = "text", flex_grow = 1 }),
    }))
  end
  if #children == 0 then
    ui.push(children, ui.text(id .. "-none", "none in the last 24 hours",
      { size = 9, ink = "muted", height = 18 }))
  end
  return ui.column(id, { height = #children * 18, gap = 0 }, children)
end

function deliveries(hook)
  local id = "pulse-deliveries"
  local children = {}
  for index, delivery in ipairs(hook.deliveries or {}) do
    local status = tostring(delivery.status or "")
    ui.push(children, ui.row(id .. "-" .. index, { height = 18, gap = 10 }, {
      ui.text(id .. "-" .. index .. "-when", tostring(delivery.text or ""),
        { size = 9, ink = "muted", width = 90 }),
      ui.text(id .. "-" .. index .. "-status", string.upper(status), {
        size = 9, width = 70,
        ink = status == "accepted" and "success" or "danger",
      }),
      ui.text(id .. "-" .. index .. "-kind", clip(delivery.event_type, 24),
        { size = 9, ink = "muted", width = 170 }),
      ui.text(id .. "-" .. index .. "-reason", clip(delivery.reason, 60),
        { size = 9, ink = "muted", flex_grow = 1 }),
      ui.text(id .. "-" .. index .. "-bytes", string.format("%d B", delivery.bytes or 0),
        { size = 9, ink = "muted", width = 70 }),
    }))
  end
  if #children == 0 then
    ui.push(children, ui.text(id .. "-none", "no deliveries recorded",
      { size = 9, ink = "muted", height = 18 }))
  end
  return ui.column(id, { height = #children * 18, gap = 0 }, children)
end

local function footer(snapshot)
  return ui.row("pulse-footer", { height = 18, gap = 16 }, {
    ui.text("pulse-footer-note", "schedules are UTC · every edit is a record write",
      { size = 9, ink = "muted", flex_grow = 1 }),
    ui.text("pulse-footer-cli", "pulse --records <dir> set <id> --schedule '…'",
      { size = 9, ink = "muted", width = 340 }),
  })
end

function M.render(snapshot)
  snapshot = snapshot or {}
  ensure_hit_sink()
  local width, height = viewport()
  local ingress = snapshot.tab == "ingress"
  local list = ingress and hook_list(snapshot) or cron_list(snapshot)
  local detail = ingress and hook_detail(snapshot) or cron_detail(snapshot, width)
  local root = ui.workspace("pulse-root", {
    width = width,
    height = height,
    gap = 8,
    padding = { top = 14, right = 18, bottom = 10, left = 18 },
  }, {
    header(snapshot),
    tabs(snapshot),
    ui.row("pulse-body", { flex_grow = 1, gap = 14 }, { list, detail }),
    footer(snapshot),
  })
  M.hits = ui.present(root, { width = width, height = height })
end

return M
