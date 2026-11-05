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

local function text(value, style, ink, props)
  props = props or {}
  props.style = style or "body"
  props.ink = ink or ROLE.body
  return ui.text(tostring(value or ""), props)
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

local function stat(label, value, ink)
  return ui.column {
    gap = "none",
    children = {
      text(value, "subheading", ink or ROLE.title),
      text(label, "caption", ROLE.quiet),
    },
  }
end

local function filter_row(filter)
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
      pad = ui.gap.xs,
      ink = "primary",
      opacity = 0.14,
      radius = theme().radius,
      children = { row },
    }
  end
  return ui.bar { pad = ui.gap.xs, children = { row } }
end

local function thread_row(thread, cursor)
  local unread = thread.unread and true or false
  local title_ink = unread and ROLE.title or ROLE.body
  local status_ink = thread.urgent and ROLE.warning or ROLE.quiet
  local status = thread.status or (thread.kind == "chat" and "chat" or "open")
  local who = thread.assignee and ("@" .. thread.assignee) or (thread.sender or "")
  local meta = ui.column {
    gap = "none",
    children = {
      text(status, "caption", status_ink),
      text(who, "caption", ROLE.quiet),
      text(thread.time or "", "caption", ROLE.quiet),
    },
  }
  local body = ui.row {
    widths = { 0.035, 0.60, 0.20, 0.165 },
    gap = "xs",
    children = {
      dot(unread and ROLE.title or ROLE.quiet),
      ui.column {
        gap = "none",
        children = {
          text(thread.title, "body", title_ink),
          text(thread.preview or thread.summary or "", "caption", ROLE.quiet),
        },
      },
      ui.column {
        gap = "none",
        children = {
          text(thread.sender or "", "caption", ROLE.body),
          text(thread.labels and table.concat(thread.labels, " · ") or "", "caption", ROLE.quiet),
        },
      },
      meta,
    },
  }
  if thread.selected then
    return ui.bar {
      pad = ui.gap.xs,
      ink = "primary",
      opacity = 0.10,
      radius = theme().radius,
      children = { body },
    }
  end
  if cursor then
    return ui.bar {
      pad = ui.gap.xs,
      ink = "primary",
      opacity = 0.05,
      radius = theme().radius,
      children = { body },
    }
  end
  return ui.bar { pad = ui.gap.xs, children = { body } }
end

local function message(msg)
  return ui.column {
    gap = "xs",
    children = {
      ui.row {
        widths = { 0.72, 0.28 },
        children = {
          text(msg.from, "body", ROLE.body),
          text(msg.time or "", "caption", ROLE.quiet),
        },
      },
      text(msg.preview or "", "body", ROLE.body),
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
  for _, filter in ipairs(snapshot.filters or {}) do
    children[#children + 1] = filter_row(filter)
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
      children[#children + 1] = thread_row(thread, thread.id == layout.cursor_id)
    end
  end
  if #threads > last then
    children[#children + 1] = ui.row {
      widths = { 0.72, 0.28 },
      children = {
        text("j/k move · space opens", "caption", ROLE.quiet),
        text(string.format("↓ %d more", #threads - last), "caption", ROLE.warning),
      },
    }
  elseif offset > 0 then
    children[#children + 1] = text("↑ back to the top with g", "caption", ROLE.quiet)
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
            text(thread.title, "heading", ROLE.heading),
            text((thread.sender or "") .. " · " .. (thread.time or ""), "caption", ROLE.quiet),
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
    ui.spacer { size = "xs" },
    ui.row {
      gap = "xs",
      children = {
        narrow and pill("← Back", { quiet = true }) or nil,
        pill("Reply"),
        pill("Reply all", { quiet = true }),
        pill("Assign", { quiet = true }),
        pill("Resolve", { quiet = true }),
        ui.icon_button("more", { size = "small", quiet = true }),
      },
    },
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
            text(thread.brief or thread.preview or "", "body", ROLE.body),
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
    children[#children + 1] = ui.bar {
      pad = ui.gap.sm,
      ink = "surface",
      opacity = 0.7,
      radius = theme().radius,
      children = {
        ui.column {
          gap = "xs",
          children = {
            text("DRAFT · needs approval", "caption", ROLE.warning),
            text(thread.draft, "body", ROLE.body),
          },
        },
      },
    }
    children[#children + 1] = ui.row {
      gap = "xs",
      children = { pill("Approve & send"), pill("Edit", { quiet = true }), pill("Discard", { quiet = true }) },
    }
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
        widths = { 0.60, 0.40 },
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
              pill("Compose"),
              ui.icon_button("search", { size = "small", quiet = true }),
              ui.icon_button("more", { size = "small", quiet = true }),
            },
          },
        },
      },
    },
  }
end

local function summary(snapshot)
  local c = snapshot.counts or {}
  return ui.bar {
    pad = ui.gap.sm,
    ink = "surface",
    opacity = 0.55,
    border = true,
    children = {
      ui.row {
        widths = { 0.25, 0.25, 0.25, 0.25 },
        children = {
          stat("unread", c.unread or 0, ROLE.title),
          stat("need attention", c.urgent or 0, ROLE.warning),
          stat("support open", c.support or 0, ROLE.body),
          stat("agent drafts", c.drafts or 0, ROLE.success),
        },
      },
    },
  }
end

local function footer(snapshot)
  return ui.bar {
    h = ui.gap.lg,
    pad = ui.gap.md,
    ink = "surface",
    opacity = 0.95,
    border = true,
    border_top = true,
    children = {
      ui.row {
        widths = { 0.62, 0.38 },
        children = {
          text(
            "j/k move · enter open · r reply · a assign · / search · focus: "
              .. tostring((snapshot.layout and snapshot.layout.focus) or "list"),
            "caption",
            ROLE.quiet
          ),
          ui.row {
            gap = "sm",
            align = "end",
            children = {
              ui.kbd("⌘K"),
              text((snapshot.account and snapshot.account.provider) or "AgentMail", "caption", ROLE.quiet),
            },
          },
        },
      },
    },
  }
end

function M.tree(snapshot)
  snapshot = snapshot or {}
  local layout = snapshot.layout or {}
  local narrow = layout.width_class == "narrow"
  local main
  if narrow then
    if layout.focus == "detail" then
      main = detail(snapshot, true)
    else
      main = list(snapshot)
    end
  else
    main = ui.row {
      widths = { 0.24, 0.76 },
      gap = "md",
      children = {
        rail(snapshot),
        ui.row {
          widths = { 0.56, 0.44 },
          gap = "md",
          children = {
            list(snapshot),
            detail(snapshot, false),
          },
        },
      },
    }
  end
  return ui.column {
    h = 8,
    gap = "none",
    children = {
      header(snapshot),
      summary(snapshot),
      main,
      footer(snapshot),
    },
  }
end

function M.render(snapshot)
  ui.render(M.tree(snapshot), theme())
end

return M
