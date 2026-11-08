-- Worklog view: a descending timeline of normalized events.
--
-- The snapshot arrives already ordered newest-arrival-first and already filtered
-- (the producer drops lease heartbeats). This file paints it. Re-sorting here
-- would be a second opinion about order, and two opinions is how a timeline
-- starts disagreeing with itself.
--
-- Host-side layout: the view returns a tree of semantic nodes and the host
-- resolves geometry. Canvas has neither a layout service nor hit targets, so
-- both have fallbacks below (see docs/interaction-gap.md).

local M = {}
M.name = "worklog"
M.version = "0.1.0"

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

-- Canvas rejects ink roles it does not know rather than falling back to body
-- text. The theme documents `surface` and `background`; Canvas accepts neither,
-- so both are spelled with `muted`, which it does accept. Fixing this properly
-- means a host palette with one name per role, not another alias here.
local INK = { surface = "muted", background = "muted" }
local function ink(name)
  return INK[name] or name
end

local function rect(id, x, y, w, h, role, opacity, radius)
  canvas.rect {
    id = id, ink = ink(role or "surface"), opacity = opacity or 1.0,
    radius = radius or 8,
    translation = { nx(x), ny(y), Z },
    scale = { sw(math.max(1, w)), sh(math.max(1, h)), 1 },
  }
end

local function text(id, value, x, y, px, role, z)
  canvas.text {
    id = id, text = tostring(value or ""), ink = ink(role or "text"),
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
-- library and Soil exposes no hit targets. Call it when a host provides it,
-- otherwise keep the spec so the row stays addressable by whatever consumes the
-- scene.
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

local function layout(tree, width, height)
  if canvas and canvas.layout then return canvas.layout(tree, width, height) end
  return ui.layout(tree, width, height)
end

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

local function title(snapshot) return snapshot.title or "WORKLOG" end

local function selected_event(snapshot)
  for index, event in ipairs(snapshot.events or {}) do
    if event.selected or event.id == snapshot.selected_id then return event, index end
  end
  return (snapshot.events or {})[1], 1
end

-- The kind is the first thing a skimmer reads, so it leads the row in a fixed
-- column and is never abbreviated differently in two places.
local function kind_word(kind)
  return string.upper(tostring(kind or "?"))
end

local function event_row(event, index)
  return {
    id = "event-" .. index, kind = "container",
    layout = { height = 56, direction = "column", gap = 2, padding = { top = 7, right = 12, bottom = 7, left = 12 } },
    children = {
      {
        id = "event-" .. index .. "-head", kind = "container", layout = { direction = "row", height = 14 },
        children = {
          label("event-" .. index .. "-kind", kind_word(event.kind), 9, { width = 82 }),
          label("event-" .. index .. "-source", event.source or "", 9, { width = 64 }),
          label("event-" .. index .. "-actor", event.actor or "", 9, { flex_grow = 1 }),
          label("event-" .. index .. "-age", event.age or "", 9, { width = 40 }),
        },
      },
      label("event-" .. index .. "-summary", clip(event.summary, 76), 12, { height = 17 }),
      label("event-" .. index .. "-subject", clip(event.subject, 56), 9, { height = 12 }),
    },
  }
end

local function list(snapshot)
  local events = snapshot.events or {}
  local counts = snapshot.counts or {}
  local range = (#events == 0) and "0 of 0" or string.format("1–%d of %d", #events, counts.total or #events)
  local children = {
    {
      id = "worklog-head", kind = "container", layout = { direction = "row", height = 20 },
      children = {
        label("worklog-range", title(snapshot) .. "  ·  " .. range, 10, { flex_grow = 1 }),
        label("worklog-sort", snapshot.sort or "Newest first", 10, { width = 92 }),
      },
    },
  }
  if #events == 0 then
    children[#children + 1] = label("worklog-empty", "Nothing has arrived yet.", 12, { height = 24 })
  end
  for index, event in ipairs(events) do
    children[#children + 1] = event_row(event, index)
  end
  return { id = "worklog-list", kind = "container", layout = { flex_grow = 1, direction = "column", gap = 0 }, children = children }
end

local function detail(snapshot)
  local event = selected_event(snapshot)
  if not event then
    return { id = "worklog-detail", kind = "container", layout = {}, children = {
      label("worklog-detail-empty", "Select an event", 12, {}) } }
  end
  local children = {
    label("worklog-detail-label", "EVENT", 9, { height = 14 }),
    label("worklog-detail-summary", event.summary or "", 15, { height = 40, wrap = true }),
    label("worklog-detail-when", (event.at_text or "") .. "  ·  " .. (event.age or ""), 9, { height = 14 }),
    label("worklog-detail-who", (event.actor or "") .. "  ·  " .. (event.source or ""), 9, { height = 14 }),
    label("worklog-detail-kind", kind_word(event.kind), 9, { height = 14 }),
    label("worklog-detail-open-label", "OPEN", 9, { height = 16 }),
  }
  local links = event.links or {}
  if #links == 0 then
    children[#children + 1] = label("worklog-detail-nolinks", "no locators", 9, { height = 14 })
  end
  for index, link in ipairs(links) do
    children[#children + 1] = label("worklog-detail-link-" .. index,
      clip(link.locator, 44), 9, { height = 16 })
  end
  return { id = "worklog-detail-page", kind = "container", layout = { flex_grow = 1, direction = "column", gap = 6 }, children = children }
end

local function sources(snapshot)
  local rows = snapshot.sources or {}
  local parts = {}
  for _, source in ipairs(rows) do
    parts[#parts + 1] = string.format("%s %d", source.label or source.id or "?", source.count or 0)
  end
  return table.concat(parts, "  ·  ")
end

local function tree(snapshot)
  local counts = snapshot.counts or {}
  local wide = W >= 620
  local root = {
    id = "worklog-root", kind = "container",
    layout = { width = W, height = H, direction = "column", gap = 8, padding = { top = 12, right = 22, bottom = 18, left = 22 } },
    children = {
      {
        id = "worklog-toolbar", kind = "container", layout = { direction = "row", height = 22 },
        children = {
          label("worklog-title", title(snapshot), 12, { width = 120 }),
          label("worklog-sources", sources(snapshot), 10, { flex_grow = 1 }),
          label("worklog-today", string.format("%d today", counts.today or 0), 10, { width = 90 }),
        },
      },
    },
  }
  if wide then
    root.children[#root.children + 1] = {
      id = "worklog-main", kind = "container", layout = { flex_grow = 1, direction = "row", gap = 16 },
      children = {
        { id = "worklog-list-pane", kind = "container", layout = { flex_grow = 1, flex_shrink = 1, direction = "column", padding = { top = 8, right = 10, bottom = 8, left = 10 } }, children = { list(snapshot) } },
        { id = "worklog-detail-pane", kind = "container", layout = { width = 300, flex_grow = 0, flex_shrink = 0, direction = "column", padding = { top = 10, right = 14, bottom = 10, left = 14 } }, children = { detail(snapshot) } },
      },
    }
  else
    root.children[#root.children + 1] = { id = "worklog-main", kind = "container", layout = { flex_grow = 1 }, children = { list(snapshot) } }
  end
  return root
end

-- A row is addressable and opening it selects it. Canvas delivers activation
-- through `module.event`; without that the snapshot's own selection stands.
local function apply_event(snapshot)
  local event = _G.module and _G.module.event
  if not event or event.kind ~= "activate" then return snapshot end
  local index = tonumber(tostring(event.target_id or ""):match("^event%-(%d+)$"))
  local target = index and (snapshot.events or {})[index]
  if not target then return snapshot end
  snapshot.selected_id = target.id
  for _, candidate in ipairs(snapshot.events or {}) do
    candidate.selected = candidate.id == target.id
  end
  return snapshot
end

-- Exported for preview tools and fixtures: the tree is the view's whole
-- structure, and walking it is cheaper than re-deriving it from the drawn scene.
M.tree = tree

function M.render(snapshot)
  use_viewport()
  snapshot = apply_event(snapshot or {})
  local events = snapshot.events or {}
  local wide = W >= 620
  local laid = layout(tree(snapshot), W, H)

  rect("worklog-bg", 0, 0, W, H, "background", 1.0, 0)
  local list_pane = find(laid, "worklog-list-pane")
  if list_pane then rect("worklog-list-bg", list_pane.rect.x, list_pane.rect.y, list_pane.rect.width, list_pane.rect.height, "surface", 0.22, 10) end
  local detail_pane = find(laid, "worklog-detail-pane")
  if detail_pane then rect("worklog-detail-bg", detail_pane.rect.x, detail_pane.rect.y, detail_pane.rect.width, detail_pane.rect.height, "surface", 0.22, 10) end

  for index = 1, #events do
    local row = find(laid, "event-" .. index)
    if row then
      rect("event-bg-" .. index, row.rect.x, row.rect.y, row.rect.width, row.rect.height, "surface", 0.10, 6)
      rect("event-rule-" .. index, row.rect.x + 10, row.rect.y + row.rect.height - 1, row.rect.width - 20, 1, "muted", 0.05, 0)
    end
  end
  local _, selected_index = selected_event(snapshot)
  local selected = find(laid, "event-" .. tostring(selected_index))
  if selected then
    rect("event-selected", selected.rect.x, selected.rect.y, selected.rect.width, selected.rect.height, "primary", 0.14, 7)
  end

  for _, node in ipairs(laid.nodes or {}) do
    if node.kind == "text" and node.style then
      local ink_role = "text"
      if node.id:find("%-age$") or node.id:find("%-subject$") or node.id:find("%-actor$")
        or node.id:find("worklog%-range") or node.id:find("worklog%-sort")
        or node.id:find("worklog%-sources") or node.id:find("worklog%-today")
        or node.id:find("%-detail%-when") or node.id:find("%-detail%-who")
        or node.id:find("%-detail%-kind") or node.id:find("%-detail%-link")
        or node.id:find("%-detail%-open%-label") or node.id:find("%-detail%-label")
        or node.id:find("%-detail%-nolinks") or node.id:find("worklog%-empty")
        or node.id:find("%-detail%-empty") then ink_role = "muted" end
      if node.id:find("%-kind$") then ink_role = "primary" end
      if node.id == "worklog-detail-summary" then ink_role = "primary" end
      text(node.id, node.text, node.rect.x, node.rect.y, node.style.size, ink_role)
    end
  end

  for index = 1, #events do
    local row = find(laid, "event-" .. index)
    if row then
      hit {
        id = "event-" .. index, label = "hit",
        x = nx(row.rect.x), y = ny(row.rect.y), width = sw(row.rect.width), height = sh(row.rect.height),
      }
    end
  end
  M.wide = wide
end

return M
