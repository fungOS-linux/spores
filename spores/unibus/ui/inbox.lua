-- Inbox view. Host-side Taffy layout; this file owns structure and copy only.
--
-- The same view is a projection of the snapshot: list and detail are semantic
-- panes, and the host resolves the actual geometry with measured text.

local M = {}
M.name = "unibus"
M.version = "0.3.0"

local W, H = 1086, 1569
local Z = 0

local function use_viewport()
  local vp = (_G.module and _G.module.viewport)
    or (_G.module and _G.module.theme and _G.module.theme.viewport)
  if vp and vp.width and vp.height and vp.width > 0 and vp.height > 0 then
    W, H = vp.width, vp.height
  end
end

local function theme() return (_G.module and _G.module.theme) or { body = 16, radius = 9.6 } end
local function base() return (theme().body or 16) end
local function nx(px) return -4 + 8 * px / W end
local function ny(px) return 4 - 8 * px / H end
local function sw(px) return 8 * px / W end
local function sh(px) return 8 * px / H end
local function ts(px) return px / ((base() or 16) * 1.25) end

local function rect(id, x, y, w, h, ink, opacity, radius)
  canvas.rect {
    id = id, ink = ink or "surface", opacity = opacity or 1.0,
    radius = radius or 8,
    translation = { nx(x), ny(y), Z },
    scale = { sw(math.max(1, w)), sh(math.max(1, h)), 1 },
  }
end

local function text(id, value, x, y, px, ink, z)
  canvas.text {
    id = id, text = tostring(value or ""), ink = ink or "text",
    translation = { nx(x), ny(y), z or (Z + 0.1) },
    scale = { ts(px), ts(px), 1 },
  }
end

local function clip(value, limit)
  value = tostring(value or "")
  if value == "" then return "—" end
  if #value <= limit then return value end
  return value:sub(1, limit - 1) .. "…"
end

-- `widget.hit` has no host implementation: Canvas's `widget` is the theming
-- library (pixel_space, button, panel, label, badge, metric) and Soil exposes no
-- hit targets. Calling it crashed this module inside Canvas. Call it when a host
-- provides it, otherwise record the spec so the row is still addressable by
-- whatever consumes the scene. See docs/interaction-gap.md.
local pending_hits = {}
local function hit(spec)
  if type(widget) == "table" and type(widget.hit) == "function" then
    widget.hit(spec)
  else
    pending_hits[#pending_hits + 1] = spec
  end
end
M.pending_hits = pending_hits
M.hit = hit

local function label(id, value, size, layout)
  layout = layout or {}
  return {
    id = id, kind = "text", text = clip(value, 240),
    style = { size = size, line_height = size + 4, wrap = layout.wrap == true },
    layout = layout,
  }
end

local function find(laid, id)
  for _, node in ipairs(laid.nodes or {}) do
    if node.id == id then return node end
  end
end

local function selected_thread(snapshot)
  for index, thread in ipairs(snapshot.threads or {}) do
    if thread.selected or thread.id == snapshot.selected_id then return thread, index end
  end
  return (snapshot.threads or {})[1], 1
end

local function thread_row(thread, index, selected)
  local unread = thread.unread and true or false
  return {
    id = "thread-" .. index,
    kind = "container",
    layout = { height = 66, direction = "column", gap = 2, padding = { top = 9, right = 12, bottom = 7, left = 12 } },
    children = {
      {
        id = "thread-" .. index .. "-top", kind = "container",
        layout = { direction = "row", height = 16 },
        children = {
          label("thread-" .. index .. "-sender", clip(thread.sender, 24), 10, { flex_grow = 1 }),
          label("thread-" .. index .. "-time", thread.time or "", 10, { width = 64 }),
        },
      },
      label("thread-" .. index .. "-title", clip(thread.title, 42), unread and 14 or 13, { height = 19 }),
      label("thread-" .. index .. "-preview", clip(thread.preview, 62), 10, { height = 14 }),
    },
  }
end

local function list(snapshot)
  local threads = snapshot.threads or {}
  local layout = snapshot.layout or {}
  local offset = math.max(0, math.floor(tonumber(layout.list_offset) or 0))
  local limit = math.min(14, math.max(1, math.floor(tonumber(layout.list_limit) or #threads or 1)))
  local first = math.min(#threads, offset + 1)
  local last = math.min(#threads, offset + limit)
  if #threads == 0 then first, last = 0, 0 end
  local range = (#threads == 0) and "0 of 0" or string.format("%d–%d of %d", first, last, #threads)
  local selected, selected_index = selected_thread(snapshot)
  local children = {
    {
      id = "mail-head", kind = "container", layout = { direction = "row", height = 20 },
      children = {
        label("mail-range", "MAIL  ·  " .. range, 10, { flex_grow = 1 }),
        label("mail-sort", snapshot.sort or "Newest first", 10, { width = 92 }),
      },
    },
  }
  for index = first, last do
    local thread = threads[index]
    if thread then children[#children + 1] = thread_row(thread, index, index == selected_index) end
  end
  return { id = "mail-list", kind = "container", layout = { flex_grow = 1, direction = "column", gap = 0 }, children = children }
end

local function detail(snapshot)
  local thread = selected_thread(snapshot)
  if not thread then return { id = "detail-empty", kind = "container", layout = {}, children = { label("detail-empty-text", "Select a conversation", 13, {}) } } end
  local children = {
    {
      id = "detail-head", kind = "container", layout = { direction = "column", gap = 3 },
      children = {
        label("detail-title", thread.title, 19, { height = 25 }),
        label("detail-meta", (thread.sender or "") .. "  ·  " .. (thread.time or ""), 10, { height = 16 }),
      },
    },
    {
      id = "detail-actions", kind = "container", layout = { direction = "row", height = 32, gap = 8 },
      children = {
        label("detail-reply", "Reply", 11, { width = 74, height = 28 }),
        label("detail-reply-all", "Reply all", 11, { width = 88, height = 28 }),
        label("detail-assign", "Assign", 11, { width = 74, height = 28 }),
        label("detail-resolve", "Resolve", 11, { width = 78, height = 28 }),
        label("detail-archive", "Archive", 11, { width = 78, height = 28 }),
      },
    },
    {
      id = "detail-brief", kind = "container", layout = { height = 188, flex_grow = 0, direction = "column", gap = 5, padding = { top = 12, right = 13, bottom = 10, left = 13 } },
      children = {
        label("detail-brief-label", "AGENT BRIEF", 10, { height = 14 }),
        label("detail-brief-text", thread.brief or thread.preview or "", 11, { height = 132, wrap = true }),
      },
    },
  }
  return { id = "detail-page", kind = "container", layout = { flex_grow = 1, direction = "column", gap = 14 }, children = children }
end


local function account_rows(snapshot)
  local rows = snapshot.accounts
  if not rows then
    local account = snapshot.account or {}
    rows = { { id = "primary", label = account.label or "AgentMail", email = account.email or "", provider = account.provider or "AgentMail", unread = (snapshot.counts or {}).unread or 0, selected = true } }
  end
  return rows
end

local function inbox_rows(snapshot)
  local rows = snapshot.inboxes
  if not rows then
    rows = {
      { id = "inbox", label = "Inbox", unread = (snapshot.counts or {}).unread or 0, selected = true },
      { id = "archive", label = "Archive", unread = 0 },
    }
  end
  return rows
end

local function tag_rows(snapshot)
  local rows = snapshot.tags
  if not rows then
    local counts = {}
    for _, thread in ipairs(snapshot.threads or {}) do
      for _, tag in ipairs(thread.labels or {}) do counts[tag] = (counts[tag] or 0) + 1 end
    end
    rows = {}
    for tag, count in pairs(counts) do rows[#rows + 1] = { id = tag, label = tag, count = count } end
    table.sort(rows, function(a, b) return a.label < b.label end)
  end
  return rows
end

local function rail(snapshot)
  local children = { label("rail-accounts", "ACCOUNTS", 9, { height = 16 }) }
  for index, account in ipairs(account_rows(snapshot)) do
    children[#children + 1] = label("rail-account-" .. index,
      clip(account.label or account.email, 16) .. "  ·  " .. tostring(account.unread or 0), 10, { height = 22 })
  end
  children[#children + 1] = label("rail-inboxes", "INBOXES", 9, { height = 16 })
  for index, box in ipairs(inbox_rows(snapshot)) do
    children[#children + 1] = label("rail-inbox-" .. index,
      clip(box.label or box.id, 16) .. "  ·  " .. tostring(box.unread or box.count or 0), 10, { height = 22 })
  end
  children[#children + 1] = label("rail-tags", "TAGS", 9, { height = 16 })
  for index, tag in ipairs(tag_rows(snapshot)) do
    children[#children + 1] = label("rail-tag-" .. index,
      "#" .. clip(tag.label or tag.id, 15) .. "  ·  " .. tostring(tag.count or 0), 10, { height = 20 })
  end
  return { id = "inbox-rail-body", kind = "container", layout = { flex_grow = 1, direction = "column", gap = 1 }, children = children }
end

local function tree(snapshot)
  local count = snapshot.counts or {}
  local visible = math.min(#(snapshot.threads or {}), 14)
  local wide = W >= 700
  local root = {
    id = "inbox-root", kind = "container",
    layout = { width = W, height = H, direction = "column", gap = 8, padding = { top = 12, right = 22, bottom = 18, left = 22 } },
    children = {
      { id = "inbox-toolbar", kind = "container", layout = { direction = "row", height = 22 },
        children = {
          label("inbox-account", (snapshot.account and (snapshot.account.email .. "  ·  " .. (snapshot.account.sync or ""))) or "", 10, { flex_grow = 1 }),
          label("inbox-counts", string.format("%d unread  ·  %d attention", count.unread or 0, count.urgent or 0), 10, { width = 200 }),
        } },
      { id = "inbox-search", kind = "container", layout = { height = 36, padding = { left = 12, right = 12, top = 10 } },
        children = { label("inbox-search-text", "⌕  Search mail, people, tags", 10, {}) } },
    },
  }
  if wide then
    root.children[#root.children + 1] = {
      id = "inbox-main", kind = "container", layout = { flex_grow = 1, direction = "row", gap = 16 },
      children = {
        { id = "inbox-rail", kind = "container", layout = { width = 150, flex_grow = 0, flex_shrink = 0, direction = "column", padding = { top = 8, right = 8, bottom = 8, left = 8 } }, children = { rail(snapshot) } },
        { id = "inbox-list-pane", kind = "container", layout = { width = 340, flex_grow = 0, flex_shrink = 0, direction = "column", padding = { top = 8, right = 10, bottom = 8, left = 10 } }, children = { list(snapshot) } },
        { id = "inbox-detail-pane", kind = "container", layout = { flex_grow = 1, flex_shrink = 1, direction = "column", padding = { top = 10, right = 18, bottom = 10, left = 18 } }, children = { detail(snapshot) } },
      },
    }
  else
    root.children[#root.children + 1] = { id = "inbox-main", kind = "container", layout = { flex_grow = 1 }, children = { list(snapshot) } }
  end
  return root
end

local function apply_event(snapshot)
  local event = _G.module and _G.module.event
  if not event or event.kind ~= "activate" then return snapshot end
  local target = event.target_id or ""
  local index = tonumber(target:match("^thread%-(%d+)$"))
  if index and (snapshot.threads or {})[index] then
    snapshot.selected_id = snapshot.threads[index].id
    for _, thread in ipairs(snapshot.threads or {}) do thread.selected = thread.id == snapshot.selected_id end
  end
  return snapshot
end

function M.render(snapshot)
  use_viewport()
  snapshot = apply_event(snapshot or {})
  local visible = math.min(#(snapshot.threads or {}), 14)
  local laid = canvas.layout(tree(snapshot), W, H)

  rect("inbox-bg", 0, 0, W, H, "background", 1.0, 0)
  local search = find(laid, "inbox-search")
  if search then rect("inbox-search-bg", search.rect.x, search.rect.y, search.rect.width, search.rect.height, "surface", 0.72, 8) end
  local rail_pane = find(laid, "inbox-rail")
  if rail_pane then rect("inbox-rail-bg", rail_pane.rect.x, rail_pane.rect.y, rail_pane.rect.width, rail_pane.rect.height, "surface", 0.22, 10) end
  local list_pane = find(laid, "inbox-list-pane")
  if list_pane then rect("inbox-list-bg", list_pane.rect.x, list_pane.rect.y, list_pane.rect.width, list_pane.rect.height, "surface", 0.28, 10) end
  local detail_pane = find(laid, "inbox-detail-pane")
  if detail_pane then rect("inbox-detail-bg", detail_pane.rect.x, detail_pane.rect.y, detail_pane.rect.width, detail_pane.rect.height, "surface", 0.28, 10) end
  local brief = find(laid, "detail-brief")
  if brief then rect("inbox-brief-bg", brief.rect.x, brief.rect.y, brief.rect.width, brief.rect.height, "primary", 0.08, 8) end
  local actions = find(laid, "detail-actions")
  if actions then
    for _, id in ipairs({ "detail-reply", "detail-reply-all", "detail-assign", "detail-resolve", "detail-archive" }) do
      local node = find(laid, id)
      if node then rect("action-bg-" .. id, node.rect.x, node.rect.y, node.rect.width, node.rect.height, id == "detail-reply" and "primary" or "surface", id == "detail-reply" and 0.75 or 0.65, 7) end
    end
  end
  for index = 1, visible do
    local row = find(laid, "thread-" .. index)
    if row then
      rect("thread-bg-" .. index, row.rect.x, row.rect.y, row.rect.width, row.rect.height, "surface", 0.10, 6)
      if index < visible then
        rect("thread-rule-" .. index, row.rect.x + 10, row.rect.y + row.rect.height - 1, row.rect.width - 20, 1, "muted", 0.05, 0)
      end
    end
  end
  local _, selected_index = selected_thread(snapshot)
  local selected = find(laid, "thread-" .. tostring(selected_index))
  if selected then rect("thread-selected", selected.rect.x, selected.rect.y, selected.rect.width, selected.rect.height, "primary", 0.14, 7) end

  for _, node in ipairs(laid.nodes or {}) do
    if node.kind == "text" and node.style then
      local ink = "text"
      if node.id:find("account") or node.id:find("range") or node.id:find("sort") or node.id:find("meta") or node.id:find("brief%-label") or node.id:find("thread%-label") or node.id:find("msg%-from") or node.id:find("%-preview$") then ink = "muted" end
      if node.id == "detail-title" then ink = "primary" end
      if node.id:find("^account%-%d+%-name$") and node.text ~= "" then ink = "text" end
      if node.id:find("^tag%-%d+%-name$") then ink = "primary" end
      if node.id == "detail-brief-text" then ink = "text" end
      if node.id == "detail-reply" then ink = "background" end
      text(node.id, node.text, node.rect.x, node.rect.y, node.style.size, ink)
    end
  end

  if wide then
    for index = 1, visible do
      local row = find(laid, "thread-" .. index)
      if row then
        hit { id = "thread-" .. index, label = "hit",
          x = nx(row.rect.x), y = ny(row.rect.y), width = sw(row.rect.width), height = sh(row.rect.height) }
      end
    end
  end
end

return M
