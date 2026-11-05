-- unibus · inbox view
--
-- A pure function of the snapshot. The tree is structure, roles and copy; the
-- host owns pixels. No I/O, no clock, no state, no callbacks. Pressable things
-- are affordances until the host routes an action back to the Spore.

local M = {}

M.name = "unibus"
M.version = "0.2.0"

local ui = assert(_G.ui, "ui library not composed")
local DEFAULT_THEME = {
  title = 0.34,
  large = 0.24,
  body = 0.145,
  small = 0.105,
  micro = 0.085,
  radius = 0.02,
}

local function theme()
  return (_G.module and _G.module.theme) or DEFAULT_THEME
end

local ROLE = {
  title = "primary",
  heading = "primary",
  body = "text",
  quiet = "muted",
  success = "success",
  warning = "warning",
  danger = "danger",
}

local function text_px(style)
  local t = theme()
  local base = t.body or 16
  return ({
    title = base * 0.58,
    heading = base * 0.50,
    subheading = base * 0.46,
    body = base * 0.34,
    caption = base * 0.26,
    micro = base * 0.22,
  })[style or "body"] or (base * 0.44)
end

local function text(value, style, ink, chars)
  return ui.text(tostring(value or ""), {
    style = style or "body",
    ink = ink or ROLE.body,
    chars = chars,
    px = text_px(style),
  })
end

local function dot(ink, size)
  return ui.icon("dot", { size = size or "micro", ink = ink or "muted" })
end

local function pill(label, opts)
  opts = opts or {}
  return ui.button(label, {
    small = true,
    quiet = opts.quiet,
    selected = opts.selected,
  })
end


local function filter_row(filter, index)
  local label = filter.label
  local count = filter.count and tostring(filter.count) or ""
  local ink = filter.selected and ROLE.title or ROLE.body
  local row = ui.row {
    widths = { 0.78, 0.22 },
    children = {
      text(label, "body", ink),
      text(count, "caption", filter.selected and ROLE.title or ROLE.quiet),
    },
  }
  if filter.selected then
    return ui.bar {
      hit = { id = "filter-" .. index },
      pad = ui.gap.xs,
      ink = "primary",
      opacity = 0.14,
      radius = theme().radius,
      children = { row },
    }
  end
  return ui.bar { hit = { id = "filter-" .. index }, pad = ui.gap.xs, children = { row } }
end

local function thread_row(thread, cursor, index)
  local unread = thread.unread and true or false
  local title_ink = unread and ROLE.title or ROLE.body
  local labels = thread.labels and ("#" .. table.concat(thread.labels, " #")) or ""
  local body = ui.column {
    gap = "none",
    children = {
      ui.row {
        widths = { 0.035, 0.45, 0.30, 0.215 },
        gap = "xs",
        children = {
          dot(unread and ROLE.title or ROLE.quiet),
          text(thread.sender or "", "caption", ROLE.body, 16),
          text(labels, "caption", ROLE.primary, 22),
          text(thread.time or "", "caption", ROLE.quiet),
        },
      },
      text(thread.title, "body", title_ink, 30),
    },
  }
  if thread.selected then
    return ui.bar {
      hit = { id = "thread-" .. index },
      pad = ui.gap.xs,
      ink = "primary",
      opacity = 0.10,
      radius = theme().radius,
      children = { body },
    }
  end
  if cursor then
    return ui.bar {
      hit = { id = "thread-" .. index },
      pad = ui.gap.xs,
      ink = "primary",
      opacity = 0.05,
      radius = theme().radius,
      children = { body },
    }
  end
  return ui.bar { hit = { id = "thread-" .. index }, pad = ui.gap.xs, children = { body } }
end

local function message(msg)
  return ui.column {
    gap = "xs",
    children = {
      ui.row {
        widths = { 0.72, 0.28 },
        children = {
          text(msg.from, "body", ROLE.body, 24),
          text(msg.time or "", "caption", ROLE.quiet),
        },
      },
      text(msg.preview or "", "body", ROLE.body, 30),
    },
  }
end

local function rail(snapshot)
  local children = {
    text("ACCOUNTS", "caption", ROLE.quiet),
    ui.bar {
      pad = ui.gap.xs,
      ink = "surface",
      opacity = 0.55,
      radius = theme().radius,
      children = {
        ui.row {
          widths = { 0.78, 0.22 },
          children = {
            text((snapshot.account and (snapshot.account.label or snapshot.account.provider)) or "No account", "caption", ROLE.body),
            text((snapshot.account and snapshot.account.sync) or "", "caption", ROLE.quiet),
          },
        },
      },
    },
    ui.spacer { size = "xs" },
    text("FILTERS", "caption", ROLE.quiet),
  }
  for index, filter in ipairs(snapshot.filters or {}) do
    children[#children + 1] = filter_row(filter, index)
  end
  children[#children + 1] = ui.spacer { size = "xs" }
  children[#children + 1] = ui.rule {}
  children[#children + 1] = text("AGENT", "caption", ROLE.quiet)
  children[#children + 1] = text("Drafting on", "caption", ROLE.success)
  children[#children + 1] = text("Approval required", "caption", ROLE.quiet)
  return ui.column {
    gap = "xs",
    pad = ui.gap.sm,
    children = children,
  }
end

local function list(snapshot)
  local threads = snapshot.threads or {}
  local layout = snapshot.layout or {}
  local offset = math.max(0, math.floor(tonumber(layout.list_offset) or 0))
  local limit = math.max(1, math.floor(tonumber(layout.list_limit) or #threads or 1))
  local first = math.min(#threads, offset + 1)
  local last = math.min(#threads, offset + limit)
  if #threads == 0 then
    first, last = 0, 0
  end
  local range = (#threads == 0) and "0 of 0" or string.format("%d–%d of %d", first, last, #threads)
  local children = {
    ui.row {
      widths = { 0.62, 0.38 },
      children = {
        text("MAIL · " .. range, "caption", ROLE.quiet),
        text((snapshot.sort or "Newest first"), "caption", ROLE.quiet),
      },
    },
    ui.rule {},
  }
  for index = first, last do
    local thread = threads[index]
    if thread then
      children[#children + 1] = thread_row(thread, thread.id == layout.cursor_id, index)
    end
  end
  if #threads > last then
    children[#children + 1] = text(string.format("↓ %d more", #threads - last), "caption", ROLE.warning)
  elseif offset > 0 then
    children[#children + 1] = text("↑ back to the top", "caption", ROLE.quiet)
  end
  return ui.column { gap = "none", children = children }
end

local function detail(snapshot, narrow)
  local thread = nil
  for _, candidate in ipairs(snapshot.threads or {}) do
    if candidate.selected or candidate.id == snapshot.selected_id then
      thread = candidate
      break
    end
  end
  if not thread then
    return ui.column {
      pad = ui.gap.md,
      children = { text("Select a conversation", "body", ROLE.quiet) },
    }
  end

  local children = {
    ui.row {
      widths = { 0.72, 0.28 },
      children = {
        ui.column {
          gap = "none",
          children = {
            text(thread.title, "heading", ROLE.heading, 22),
            text((thread.sender or "") .. " · " .. (thread.time or ""), "caption", ROLE.quiet, 36),
          },
        },
        ui.column {
          gap = "none",
          children = {
            text(thread.status or "open", "caption", thread.urgent and ROLE.warning or ROLE.quiet),
            text(thread.assignee and ("@" .. thread.assignee) or "unassigned", "caption", ROLE.quiet),
          },
        },
      },
    },
    thread.labels and #thread.labels > 0 and text(
      "#" .. table.concat(thread.labels, "  #"),
      "caption",
      ROLE.primary,
      36
    ) or ui.spacer { size = "none" },
    (function()
      local actions = {
        pill("Reply"),
        pill("Reply all", { quiet = true }),
        pill("Assign", { quiet = true }),
        pill("Resolve", { quiet = true }),
        ui.icon_button("more", { size = "small", quiet = true }),
      }
      if narrow then table.insert(actions, 1, pill("← Back", { quiet = true })) end
      return ui.row { gap = "xs", children = actions }
    end)(),
    ui.rule {},
    ui.bar {
      pad = ui.gap.sm,
      ink = "primary",
      opacity = 0.08,
      radius = theme().radius,
      children = {
        ui.column {
          gap = "xs",
          children = {
            text("AGENT BRIEF", "caption", ROLE.quiet),
            text(thread.brief or thread.preview or "", "body", ROLE.body, 30),
          },
        },
      },
    },
    ui.spacer { size = "xs" },
    text("THREAD", "caption", ROLE.quiet),
  }
  for _, msg in ipairs(thread.messages or {}) do
    children[#children + 1] = message(msg)
    children[#children + 1] = ui.rule {}
  end
  if thread.draft then
    children[#children + 1] = text("DRAFT · waiting for approval", "caption", ROLE.warning, 40)
  else
    children[#children + 1] = ui.bar {
      pad = ui.gap.sm,
      ink = "surface",
      opacity = 0.55,
      radius = theme().radius,
      children = { text("Reply with the agent… approval required before send", "caption", ROLE.quiet) },
    }
  end
  return ui.column { gap = "sm", pad = ui.gap.md, children = children }
end

local function state_view(snapshot)
  local state = snapshot.state or {}
  local mode = state.mode or "ready"
  if mode == "ready" then
    return nil
  end
  local copy = {
    empty = { icon = "check", title = "Inbox zero", body = "No conversations match this view." },
    loading = { icon = "eye", title = "Syncing", body = "Fetching the latest metadata from the provider." },
    offline = { icon = "dot", title = "Offline", body = "Showing the last synced snapshot. Actions will queue." },
    error = { icon = "close", title = "Could not sync", body = state.message or "The provider returned an error." },
  }
  local view = copy[mode] or copy.error
  return ui.row {
    widths = { 0.24, 0.52, 0.24 },
    children = {
      ui.spacer { size = "md" },
      ui.card {
        gap = "sm",
        children = {
          ui.icon(view.icon, { size = "large", ink = mode == "error" and ROLE.danger or ROLE.quiet }),
          text(view.title, "heading", ROLE.heading),
          text(view.body, "body", ROLE.body),
          ui.row {
            gap = "xs",
            children = {
              pill(mode == "offline" and "Reconnect" or "Retry"),
              pill("Open settings", { quiet = true }),
            },
          },
        },
      },
      ui.spacer { size = "md" },
    },
  }
end

local function searchbar(snapshot)
  local value = snapshot.search or ""
  if value == "" then
    value = "Search mail, people, tags"
  end
  return ui.bar {
    pad = ui.gap.xs,
    ink = "surface",
    opacity = 0.55,
    radius = theme().radius,
    children = {
      ui.row {
        children = {
          text(value, "caption", ROLE.quiet, 60),
        },
      },
    },
  }
end

local function header(snapshot)
  local account = snapshot.account or {}
  return ui.bar {
    h = ui.gap.xl,
    pad = ui.gap.md,
    ink = "surface",
    opacity = 0.95,
    border = true,
    children = {
      ui.row {
        widths = { 0.52, 0.48 },
        children = {
          ui.column {
            gap = "none",
            children = {
              text(snapshot.title or "Inbox", "title", ROLE.title),
              text((account.email or "") .. " · " .. (account.sync or ""), "caption", ROLE.quiet),
            },
          },
          ui.row {
            gap = "xs",
            align = "end",
            children = {
              ui.icon_button("columns", { size = "small", quiet = true }),
              ui.icon_button("doc", { size = "small", quiet = true }),
              pill("New"),
            },
          },
        },
      },
    },
  }
end

local function summary(snapshot)
  local c = snapshot.counts or {}
  local line = string.format(
    "%d unread   ·   %d attention   ·   %d support   ·   %d draft",
    c.unread or 0,
    c.urgent or 0,
    c.support or 0,
    c.drafts or 0
  )
  return ui.bar {
    pad = ui.gap.xs,
    ink = "surface",
    opacity = 0.55,
    border = true,
    children = { text(line, "caption", ROLE.quiet, 92) },
  }
end


function M.tree(snapshot)
  snapshot = snapshot or {}
  local layout = snapshot.layout or {}
  local narrow = layout.width_class == "narrow"
  local main = state_view(snapshot)
  if main then
    -- non-ready state replaces the rail/list/detail
  elseif narrow then
    if layout.focus == "detail" then
      main = detail(snapshot, true)
    else
      main = list(snapshot)
    end
  else
    local rail_collapsed = layout.rail_collapsed and true or false
    local detail_collapsed = layout.detail_collapsed and true or false
    local panes = {}
    local widths = {}
    if not rail_collapsed then
      panes[#panes + 1] = rail(snapshot)
      widths[#widths + 1] = 0.21
    end
    if detail_collapsed then
      panes[#panes + 1] = list(snapshot)
      widths[#widths + 1] = rail_collapsed and 1.0 or 0.79
    else
      panes[#panes + 1] = ui.row {
        widths = { 0.38, 0.62 },
        gap = "md",
        children = {
          list(snapshot),
          detail(snapshot, false),
        },
      }
      widths[#widths + 1] = rail_collapsed and 1.0 or 0.79
    end
    main = ui.row {
      widths = widths,
      gap = "md",
      children = panes,
    }
  end
  main.grow = true
  return ui.column {
    h = 8,
    gap = "none",
    children = {
      header(snapshot),
      summary(snapshot),
      searchbar(snapshot),
      main,
    },
  }
end

local function apply_local_event(snapshot)
  local event = _G.module and _G.module.event
  if not event or event.kind ~= "activate" then return snapshot end
  local target = event.target_id or ""
  local thread_index = tonumber(target:match("^thread%-(%d+)$"))
  if thread_index then
    local thread = (snapshot.threads or {})[thread_index]
    if thread then
      snapshot.selected_id = thread.id
      for _, candidate in ipairs(snapshot.threads or {}) do
        candidate.selected = candidate.id == snapshot.selected_id
      end
    end
  else
    local filter_index = tonumber(target:match("^filter%-(%d+)$"))
    if filter_index and (snapshot.filters or {})[filter_index] then
      for index, filter in ipairs(snapshot.filters or {}) do
        filter.selected = index == filter_index
      end
    end
  end
  return snapshot
end

function M.render(snapshot)
  snapshot = apply_local_event(snapshot or {})
  ui.render(M.tree(snapshot), theme())
end

return M
