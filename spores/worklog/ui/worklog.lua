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

-- The surface size a host does not tell us. This matches `tend`'s default so the
-- hot loop lays out exactly what ships — a view that assumes a tall document
-- while the window is landscape draws rows below the bottom edge. A host that
-- passes `module.inputs` overrides it; the real fix is for the host to always
-- pass it, which is a Canvas question, not a view one.
local W, H = 1200, 900
local Z = 0

local function use_viewport()
  -- The host says how big the surface is; the view must not assume. `inputs` is
  -- what a widget module declares, `viewport` is what a host hands a document
  -- pane; the built-in default is only for a host that says neither.
  local vp = (_G.module and _G.module.viewport)
    or (_G.module and _G.module.inputs)
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

-- Pressable controls go through `widget.button`, the primitive a host actually
-- implements; `widget.hit` is kept only for rows, which predate it. A locator
-- rendered as a chip that cannot be pressed is decoration.
local pending_presses = {}
local function press(spec)
  if type(widget) == "table" and type(widget.button) == "function" then
    widget.button(spec)
  else
    pending_presses[#pending_presses + 1] = spec
  end
end
M.pending_presses = pending_presses
M.press = press

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

-- A node laid out past the surface cannot be hit: its normalized coordinates
-- fall outside the frame and the host is right to reject them. The fake row
-- layout used to keep everything inside by accident; real Taffy does not, which
-- is the point of running it.
local function on_surface(rect)
  return rect.y >= -0.5 and rect.y + rect.height <= H + 0.5
    and rect.x >= -0.5 and rect.x + rect.width <= W + 0.5
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

-- One type scale and one spacing scale for the whole view. Four sizes and three
-- gaps, named once: every number below is one of these, so changing the rhythm
-- is one edit rather than a hunt through magic constants.
local TYPE = { micro = 9, caption = 10, body = 12, title = 16 }
local GAP = { xs = 4, sm = 8, md = 16 }

-- Characters a line holds at this size. The host measures text; a view that
-- cannot ask must estimate. A short estimate only wraps early — an absent one
-- is what reserved 44 px for a paragraph and laid the rest of the column on top
-- of it.
local function chars_per_line(width, size)
  return math.max(8, math.floor(width / (size * 0.52)))
end

local function wrapped_height(value, width, size, max_lines)
  local lines = math.ceil(#tostring(value or "") / chars_per_line(width, size))
  lines = math.max(1, math.min(max_lines, lines))
  return lines * (size + 4) -- the same line height `label` declares
end

-- Rows have a fixed height, so the count a pane can hold is arithmetic, not a
-- constant. Drawing every event in a 40-event snapshot put a third of them past
-- the bottom edge of the surface (2,309 px of ink on a 1,569 px surface): they
-- were painted, invisible, and counted in "1–40 of 40".
local ROW_PAD = 6
local ROW_HEAD_H = 13
local ROW_SUBJECT_H = 12
local ROW_GAP_H = 2
local ROW_SUMMARY_LINES = 2
-- A row is its parts added up. Written as a sum so that changing a type size
-- moves the row with it: a row height that is one number somebody liked is how
-- the summary ended up with two lines of text in a box sized for one.
local ROW_H = ROW_PAD * 2 + ROW_HEAD_H + ROW_GAP_H
  + ROW_SUMMARY_LINES * (TYPE.body + 4) + ROW_GAP_H + ROW_SUBJECT_H
local CHROME = 120
-- The detail column is a fixed slice of the surface: wide enough for a summary at
-- title size, narrow enough to leave the list the room its rows need.
local PANE_W = 300
-- Eight lines is a summary, not a document. Past that the column stops being a
-- column and the log stops being scannable.
local SUMMARY_LINES = 8

local function visible_count(snapshot)
  local total = #(snapshot.events or {})
  -- An empty log shows zero rows, not the max(1, …) floor: there is no first
  -- event to draw.
  if total == 0 then return 0 end
  local fits = math.floor((H - CHROME) / ROW_H)
  return math.max(1, math.min(total, fits))
end

local function event_row(event, index, show_source)
  local head = {
    label("event-" .. index .. "-kind", kind_word(event.kind), TYPE.micro, { width = 88 }),
  }
  -- One source means the column is the same word on every row: it costs a
  -- column and tells the reader nothing.
  if show_source then
    head[#head + 1] = label("event-" .. index .. "-source", event.source or "", TYPE.micro, { width = 64 })
  end
  head[#head + 1] = label("event-" .. index .. "-actor", event.actor or "", TYPE.micro, { flex_grow = 1 })
  head[#head + 1] = label("event-" .. index .. "-age", event.age or "", TYPE.micro, { width = 40 })

  return {
    id = "event-" .. index, kind = "container",
    layout = { height = ROW_H, direction = "column", gap = ROW_GAP_H,
      padding = { top = ROW_PAD, right = 12, bottom = ROW_PAD, left = 12 } },
    children = {
      { id = "event-" .. index .. "-head", kind = "container", layout = { direction = "row", height = ROW_HEAD_H }, children = head },
      -- Two wrapped lines instead of one clipped one: Marbles' detail is often
      -- the whole point of the row, and `…` at 76 characters threw it away.
      label("event-" .. index .. "-summary", clip(event.summary, 160), TYPE.body,
        { height = ROW_SUMMARY_LINES * (TYPE.body + 4), wrap = true }),
      label("event-" .. index .. "-subject", clip(event.subject, 56), TYPE.micro, { height = ROW_SUBJECT_H }),
    },
  }
end

local function list(snapshot, shown, show_source)
  local events = snapshot.events or {}
  local counts = snapshot.counts or {}
  local range = (#events == 0) and "0 of 0"
    or string.format("1–%d of %d", shown, counts.total or #events)
  local children = {
    {
      id = "worklog-head", kind = "container", layout = { direction = "row", height = 20 },
      children = {
        label("worklog-range", title(snapshot) .. "  ·  " .. range, TYPE.caption, { flex_grow = 1 }),
        label("worklog-sort", snapshot.sort or "Newest first", TYPE.caption, { width = 92 }),
      },
    },
  }
  if #events == 0 then
    children[#children + 1] = label("worklog-empty", "Nothing has arrived yet.", TYPE.body, { height = 24 })
  end
  for index = 1, shown do
    children[#children + 1] = event_row(events[index], index, show_source)
  end
  return { id = "worklog-list", kind = "container", layout = { flex_grow = 1, direction = "column", gap = 0 }, children = children }
end

local function detail(snapshot)
  local event = selected_event(snapshot)
  if not event then
    return { id = "worklog-detail", kind = "container", layout = {}, children = {
      label("worklog-detail-empty", "Select an event", 12, {}) } }
  end
  local detail_summary = clip(event.summary or "",
    SUMMARY_LINES * chars_per_line(PANE_W - GAP.md * 2, TYPE.title))
  local children = {
    label("worklog-detail-label", "EVENT", TYPE.micro, { height = 14 }),
    -- A wrapped label needs the height its lines actually take. The layout gives
    -- a node exactly what it asks for, so asking for one line of a paragraph
    -- does not clip it: the ink continues over whatever was laid out next.
    label("worklog-detail-summary", detail_summary, TYPE.title,
      { height = wrapped_height(detail_summary, PANE_W - GAP.md * 2, TYPE.title, SUMMARY_LINES),
        wrap = true }),
    label("worklog-detail-when", (event.at_text or "") .. "  ·  " .. (event.age or ""), TYPE.micro, { height = 14 }),
    label("worklog-detail-who", (event.actor or "") .. "  ·  " .. (event.source or ""), TYPE.micro, { height = 14 }),
    label("worklog-detail-kind", kind_word(event.kind), TYPE.micro, { height = 14 }),
    label("worklog-detail-open-label", "OPEN", TYPE.micro, { height = 16 }),
  }
  local links = event.links or {}
  if #links == 0 then
    children[#children + 1] = label("worklog-detail-nolinks", "no locators", TYPE.micro, { height = 14 })
  end
  for index, link in ipairs(links) do
    children[#children + 1] = label("worklog-detail-link-" .. index,
      clip(link.locator, 44), TYPE.micro, { height = 16 })
  end
  return { id = "worklog-detail-page", kind = "container", layout = { flex_grow = 1, direction = "column", gap = GAP.sm }, children = children }
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
  local show_source = #(snapshot.sources or {}) > 1
  local root = {
    id = "worklog-root", kind = "container",
    layout = { width = W, height = H, direction = "column", gap = GAP.sm, padding = { top = GAP.md, right = GAP.md, bottom = GAP.md, left = GAP.md } },
    children = {
      {
        id = "worklog-toolbar", kind = "container", layout = { direction = "row", height = 24 },
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
        { id = "worklog-list-pane", kind = "container", layout = { flex_grow = 1, flex_shrink = 1, direction = "column", padding = { top = GAP.sm, right = GAP.sm, bottom = GAP.sm, left = GAP.sm } }, children = { list(snapshot, visible_count(snapshot), show_source) } },
        { id = "worklog-detail-pane", kind = "container", layout = { width = PANE_W, flex_grow = 0, flex_shrink = 0, direction = "column", padding = { top = GAP.sm, right = GAP.md, bottom = GAP.sm, left = GAP.md } }, children = { detail(snapshot) } },
      },
    }
  else
    root.children[#root.children + 1] = { id = "worklog-main", kind = "container", layout = { flex_grow = 1 }, children = { list(snapshot, visible_count(snapshot), show_source) } }
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
  local shown = visible_count(snapshot)
  local wide = W >= 620
  local laid = layout(tree(snapshot), W, H)

  rect("worklog-bg", 0, 0, W, H, "background", 1.0, 0)
  local list_pane = find(laid, "worklog-list-pane")
  if list_pane then rect("worklog-list-bg", list_pane.rect.x, list_pane.rect.y, list_pane.rect.width, list_pane.rect.height, "surface", 0.22, 10) end
  local detail_pane = find(laid, "worklog-detail-pane")
  if detail_pane then rect("worklog-detail-bg", detail_pane.rect.x, detail_pane.rect.y, detail_pane.rect.width, detail_pane.rect.height, "surface", 0.22, 10) end

  for index = 1, shown do
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

  -- Locators are drawn as chips behind their label, so a link reads as a thing
  -- you can open rather than as a line of monospaced text.
  local opened = events[selected_index] or events[1]
  local pressed = {}
  for index, link in ipairs((opened and opened.links) or {}) do
    local node = find(laid, "worklog-detail-link-" .. index)
    if node then
      rect("worklog-link-chip-" .. index, node.rect.x - 4, node.rect.y - 2,
        node.rect.width + 8, node.rect.height + 4, "surface", 0.4, 5)
      pressed[#pressed + 1] = {
        id = "worklog-link-" .. index,
        label = link.locator,
        locator = link.locator, rel = link.rel,
        rect = node.rect,
      }
    end
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

  for index = 1, shown do
    local row = find(laid, "event-" .. index)
    if row and on_surface(row.rect) then
      hit {
        id = "event-" .. index, label = "hit",
        x = nx(row.rect.x), y = ny(row.rect.y), width = sw(row.rect.width), height = sh(row.rect.height),
      }
    end
  end

  -- Opening a locator is the host's job: it owns the scheme. The view names the
  -- intent and hands over the locator; it does not resolve, fetch, or launch.
  for _, spec in ipairs(pressed) do
    if on_surface(spec.rect) then
      press {
        id = spec.id, label = spec.label,
        action = "worklog.open_locator",
        body = { locator = spec.locator, rel = spec.rel },
        x = nx(spec.rect.x), y = ny(spec.rect.y),
        width = sw(spec.rect.width), height = sh(spec.rect.height),
      }
    end
  end
  M.wide = wide
end

return M
