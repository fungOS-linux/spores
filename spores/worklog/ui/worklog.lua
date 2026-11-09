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
-- Explicit draw layers. Canvas sorts scene nodes by z and breaks ties by
-- insertion order, so "I appended it later" is how a chip ends up under the
-- panel it sits on. Every primitive names its layer instead.
local Z = 0
local Z_PANEL = Z + 0.05
local Z_CHIP = Z + 0.10
local Z_TEXT = Z + 0.20

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

-- Input the host owns and the view merely reads. A module is re-executed for
-- every render, so it cannot remember a scroll position of its own: the wheel
-- moves the host's offset, the host hands it back here, and the view windows the
-- log to it. `hover` is the id of the control under the pointer, or nil.
local function scroll_offset()
  local scroll = _G.module and _G.module.scroll
  local offset = scroll and tonumber(scroll.offset)
  if offset and offset > 0 then return offset end
  return 0
end

local function hover_id()
  local hover = _G.module and _G.module.hover
  return type(hover) == "string" and hover or nil
end
local function base() return (theme().body or 16) end
local function nx(px) return -4 + 8 * px / W end
local function ny(px) return 4 - 8 * px / H end
local function sw(px) return 8 * px / W end
local function sh(px) return 8 * px / H end
local function ts(px) return px / ((base() or 16) * 1.25) end

-- Canvas resolves every semantic role the theme publishes: text, muted,
-- primary, surface, background, success, warning, danger, info. This table used
-- to alias `surface` and `background` to `muted` on the belief that Canvas
-- accepted neither, which painted the window and every panel the same flat
-- grey and left body text sitting on its own background colour.
local function ink(name)
  return name
end

local function rect(id, x, y, w, h, role, opacity, radius, layer)
  canvas.rect {
    id = id, ink = ink(role or "surface"), opacity = opacity or 1.0,
    radius = radius or 8,
    translation = { nx(x), ny(y), layer or Z },
    scale = { sw(math.max(1, w)), sh(math.max(1, h)), 1 },
  }
end

local function text(id, value, x, y, px, role, z)
  canvas.text {
    id = id, text = tostring(value or ""), ink = ink(role or "text"),
    translation = { nx(x), ny(y), z or Z_TEXT },
    scale = { ts(px), ts(px), 1 },
  }
end

-- One drawn line is one node. A `canvas.text` carries no width, so the host
-- neither wraps nor clips it: a label the view *thinks* is two wrapped lines
-- arrives as one long line and paints across whatever sits beside it. Breaking
-- the string here makes the line count the layout reserved and the line count
-- the host draws the same number.
local function wrap_text(value, per_line)
  local lines, line = {}, ""
  for token in tostring(value or ""):gmatch("%S+") do
    local word = token
    local candidate = (line == "") and word or (line .. " " .. word)
    if #candidate <= per_line then
      line = candidate
    else
      if line ~= "" then lines[#lines + 1] = line end
      -- A single word wider than the column still has to break, or one long
      -- locator runs out of the pane it was placed in.
      while #word > per_line do
        lines[#lines + 1] = word:sub(1, per_line)
        word = word:sub(per_line + 1)
      end
      line = word
    end
  end
  if line ~= "" then lines[#lines + 1] = line end
  if #lines == 0 then lines[1] = "" end
  return lines
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
-- cannot ask must estimate, and the estimate has to be the one the host will
-- draw with. Canvas renders a 12 px label at roughly 9.6 px per character
-- (about 0.8 × its nominal size), so an estimate of 0.52 packed ~40 % more
-- text into every line than fitted and the overflow ran into the next column.
-- When the host publishes text metrics this constant goes away.
local GLYPH_W = 0.8

local function chars_per_line(width, size)
  return math.max(8, math.floor(width / (size * GLYPH_W)))
end
-- The wrap budget is the view's own arithmetic, so it is the view's to publish:
-- the fixture asserts lines against this number rather than a second guess.
M.chars_per_line = chars_per_line

-- Rows have a fixed height, so the count a pane can hold is arithmetic, not a
-- constant. Drawing every event in a 40-event snapshot put a third of them past
-- the bottom edge of the surface (2,309 px of ink on a 1,569 px surface): they
-- were painted, invisible, and counted in "1–40 of 40".
-- Rows fill the page. A fixed pitch either leaves the bottom third empty in a
-- tall window or overflows a short one, so the pitch is derived: take the rows
-- that fit between a floor and the node budget, then stretch them over the
-- height that is left. Between 44 and 96 px a row can carry a header line and a
-- summary line, which is what a log needs to read like one.
local MIN_ROW_H = 44
local MAX_ROW_H = 132
local TALL_ROW_H = 56
local ROW_H = MIN_ROW_H
-- The pitch is part of the contract: the host scrolls in pixels and the view
-- turns them into rows, so both sides need the same number.
M.row_height = ROW_H
local CHROME = 120
-- The detail column is a fixed slice of the surface: wide enough for a summary at
-- title size, narrow enough to leave the list the room its rows need.
local PANE_W = 300
-- Eight lines is a summary, not a document. Past that the column stops being a
-- column and the log stops being scannable.
local SUMMARY_LINES = 8

-- Canvas caps one module scene at 256 nodes, so how many rows exist is bounded
-- by what a row costs, not only by how much height is left. Asking for the rows
-- a tall window could hold produced a scene the host rejected outright — the
-- whole surface became an error card.
local NODE_BUDGET = 240 -- the host's 256, less room for the chrome to move
-- Measured, not guessed: the header, the panes and a detail column that can run
-- to eight summary lines cost about eighty nodes before the first row.
local CHROME_NODES = 80
-- A row costs: a rule, the kind, the actor, the summary, the age, and — when it
-- has a locator — a chip, its label and the control over it.
local NODES_PER_ROW = 9
local MAX_ROWS = math.floor((NODE_BUDGET - CHROME_NODES) / NODES_PER_ROW)

-- How many rows the window holds, and how tall each one is. Both answers come
-- from one place: a list that filled the height with rows but kept a short pitch
-- would still leave the page half empty.
local function plan_rows(snapshot)
  local total = #(snapshot.events or {})
  -- An empty log shows zero rows, not the max(1, …) floor: there is no first
  -- event to draw.
  if total == 0 then
    ROW_H = MIN_ROW_H
    M.row_height = ROW_H
    return 0
  end
  local space = math.max(0, H - CHROME)
  local fits = math.max(1, math.floor(space / MIN_ROW_H))
  local rows = math.min(total, MAX_ROWS, fits)
  ROW_H = math.max(MIN_ROW_H, math.min(MAX_ROW_H, math.floor(space / rows)))
  M.row_height = ROW_H
  return rows
end

-- A row is one line with things embedded in it: the kind, who did it, the
-- locator as a chip you can press, the summary, and how long ago. Stacked lines
-- spent three rows' worth of height per event and still could not hold a
-- locator, which is the part a reader wants to click.
local function row_segments(event, show_source)
  local segments = {
    { key = "kind", text = kind_word(event.kind), ink = "primary", size = TYPE.micro, w = 92, line = 1 },
  }
  -- One source means the column is the same word on every row: it costs space
  -- and tells the reader nothing.
  if show_source then
    segments[#segments + 1] = { key = "source", text = event.source or "", ink = "muted", size = TYPE.micro, w = 56, line = 1 }
  end
  segments[#segments + 1] = { key = "actor", text = event.actor or "", ink = "muted", size = TYPE.micro, w = 104, line = 1 }
  -- The summary comes before the locator: the sentence is the line, the chip is
  -- the footnote on it. Put the chip first and a long locator eats the row.
  segments[#segments + 1] = { key = "summary", text = event.summary or "", ink = "text", size = TYPE.body, grow = true, line = ROW_H >= TALL_ROW_H and 2 or 1 }
  local seen = {}
  for position, link in ipairs(event.links or {}) do
    if not seen[link.locator] then
      seen[link.locator] = true
      segments[#segments + 1] = { key = "locator-" .. position, chip = link.locator, rel = link.rel, size = TYPE.micro, line = 1 }
    end
  end
  local subject = event.subject or ""
  if subject:find("://", 1, true) and not seen[subject] then
    seen[subject] = true
    segments[#segments + 1] = { key = "subject", chip = subject, rel = "subject", size = TYPE.micro, line = 1 }
  end
  segments[#segments + 1] = { key = "age", text = event.age or "", ink = "muted", size = TYPE.micro, w = 40, align = "right", line = 1 }
  return segments
end

-- Row content lives beside the tree, keyed by row index. A layout service is
-- free to hand back geometry and nothing else — Taffy does exactly that — so a
-- custom field on a node is not a contract to carry data across the layout.
local ROW_SEGMENTS = {}

local function event_row(event, index, show_source)
  ROW_SEGMENTS[index] = row_segments(event, show_source)
  return {
    id = "event-" .. index, kind = "container",
    layout = { height = ROW_H, direction = "row",
      padding = { top = 0, right = 12, bottom = 0, left = 12 } },
    children = {},
  }
end

-- Which event the window starts at. The host owns the scroll offset and hands
-- back a pixel value; the view turns it into a row, so a wheel notch that lands
-- mid-row still shows a whole row rather than half of two.
local function window_start(snapshot, shown)
  local total = #(snapshot.events or {})
  if total == 0 or shown <= 0 then return 1 end
  local last_start = math.max(1, total - shown + 1)
  local start = math.floor(scroll_offset() / ROW_H) + 1
  return math.max(1, math.min(start, last_start))
end

local function list(snapshot, shown, show_source, first)
  local events = snapshot.events or {}
  local counts = snapshot.counts or {}
  first = first or 1
  local last = math.min(#events, first + shown - 1)
  local range = (#events == 0) and "0 of 0"
    or string.format("%d–%d of %d", first, last, counts.total or #events)
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
  for index = first, last do
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
  local detail_summary = clip(event.summary or "", 400)
  local children = {
    label("worklog-detail-label", "EVENT", TYPE.micro, { height = 14 }),
    -- A wrapped label needs the height its lines actually take. The layout gives
    -- a node exactly what it asks for, so asking for one line of a paragraph
    -- does not clip it: the ink continues over whatever was laid out next.
    label("worklog-detail-summary", detail_summary, TYPE.title,
      { height = SUMMARY_LINES * (TYPE.title + 4), wrap = true }),
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
  local shown = plan_rows(snapshot)
  local first = window_start(snapshot, shown)
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
    -- The list gets what the detail column leaves, named explicitly. Left to
    -- `flex_grow` the kit hands the flexible child the whole row and then also
    -- places the fixed one, so rows were laid out ~350 px wider than the column
    -- they are drawn in: their wrapped lines ran straight through the detail
    -- pane, which is the overlap that looked like a Z-order bug.
    local main_w = W - GAP.md * 2
    local list_w = math.max(240, main_w - GAP.md - PANE_W)
    root.children[#root.children + 1] = {
      id = "worklog-main", kind = "container", layout = { flex_grow = 1, direction = "row", gap = GAP.md },
      children = {
        { id = "worklog-list-pane", kind = "container", layout = { width = list_w, flex_grow = 0, flex_shrink = 0, direction = "column", padding = { top = GAP.sm, right = GAP.sm, bottom = GAP.sm, left = GAP.sm } }, children = { list(snapshot, shown, show_source, first) } },
        { id = "worklog-detail-pane", kind = "container", layout = { width = PANE_W, flex_grow = 0, flex_shrink = 0, direction = "column", padding = { top = GAP.sm, right = GAP.md, bottom = GAP.sm, left = GAP.md } }, children = { detail(snapshot) } },
      },
    }
  else
    root.children[#root.children + 1] = { id = "worklog-main", kind = "container", layout = { flex_grow = 1 }, children = { list(snapshot, shown, show_source, first) } }
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


local function measure(value, size)
  return #tostring(value or "") * size * GLYPH_W
end

-- A locator is an id under the hood. `marbles://local/mycelium-6us5` is Marbles
-- issue `mycelium-6us5`; the reader does not need the transport in front of it,
-- and the full URI still travels with the press for whoever opens it.
local function locator_label(locator)
  local value = tostring(locator or "")
  local _, rest = value:match("^([%w+.-]+)://(.*)$")
  if rest then
    return rest:match("([^/]+)$") or rest
  end
  local repo, number = value:match("^(.+)#(%d+)$")
  if repo then return (repo:match("([^/]+)$") or repo) .. "#" .. number end
  return value
end
M.locator_label = locator_label

-- One line of a row: fixed columns first, the age at the far right, chips given
-- a share of what is left, and the summary taking the remainder.
local function draw_line(parts, index, left, right, y, age_w, chip_h)
  local fixed, chips, has_age = 0, {}, false
  for _, segment in ipairs(parts) do
    if segment.chip then
      chips[#chips + 1] = segment
    elseif segment.align == "right" then
      has_age = true
    elseif not segment.grow then
      fixed = fixed + math.max(segment.w or 0, measure(segment.text, segment.size)) + GAP.xs
    end
  end
  local available = right - left - fixed - (has_age and (age_w + GAP.sm) or 0)
  local chip_room = math.max(0, math.min(available * 0.5, 320))
  local chip_widths, chip_total, used = {}, 0, 0
  for _, segment in ipairs(chips) do
    local chars = math.floor((chip_room - used - 10) / (segment.size * GLYPH_W))
    if chars >= 6 then
      local label = clip(locator_label(segment.chip), chars)
      local width = measure(label, segment.size) + 10
      chip_widths[segment] = { label = label, width = width }
      chip_total = chip_total + width + GAP.xs
      used = used + width + GAP.xs
    end
  end
  local summary_room = math.max(0, available - chip_total)

  local x = left
  for _, segment in ipairs(parts) do
    local id = string.format("event-%d-%s", index, segment.key)
    if segment.align == "right" then
      text(id, segment.text, right - age_w, y, segment.size, segment.ink)
    elseif segment.grow then
      local chars = math.floor(summary_room / (segment.size * GLYPH_W))
      if chars >= 8 then
        text(id, clip(segment.text, chars), x, y, segment.size, segment.ink)
      end
      x = x + summary_room + GAP.xs
    elseif segment.chip then
      local piece = chip_widths[segment]
      if piece then
        rect(id .. "-chip", x - 3, y - 3, piece.width, chip_h, "surface", 0.45, 5, Z_CHIP)
        text(id, piece.label, x + 2, y, segment.size, "primary")
        press {
          id = id .. "-press", label = segment.chip,
          action = "worklog-open-locator",
          body = { locator = segment.chip, rel = segment.rel or "subject" },
          x = nx(x - 3), y = ny(y - 3), width = sw(piece.width), height = sh(chip_h),
        }
        x = x + piece.width + GAP.xs
      end
    else
      text(id, segment.text, x, y, segment.size, segment.ink)
      x = x + math.max(segment.w or 0, measure(segment.text, segment.size)) + GAP.xs
    end
  end
end

-- A row is one or two lines depending on the pitch the page earned: a header of
-- who and what, then the sentence. Rows carry segments instead of child nodes,
-- so the draw pass places them rather than the layout.
local function draw_row(laid_row, index, segments)
  local row = laid_row.rect
  local pad = 12
  local left = row.x + pad
  local right = row.x + row.width - pad
  local age_w = 46
  local tall = ROW_H >= TALL_ROW_H
  local line_h = tall and (TYPE.micro + 14) or (TYPE.body + 4)
  local lines = tall and 2 or 1
  local top = row.y + (ROW_H - lines * line_h) / 2
  local chip_h = TYPE.micro + 10

  local grouped = {}
  for _, segment in ipairs(segments) do
    local line = math.min(segment.line or 1, lines)
    grouped[line] = grouped[line] or {}
    grouped[line][#grouped[line] + 1] = segment
  end
  for line = 1, lines do
    local parts = grouped[line]
    if parts and #parts > 0 then
      draw_line(parts, index, left, right, top + (line - 1) * line_h, age_w, chip_h)
    end
  end
end

function M.render(snapshot)
  use_viewport()
  snapshot = apply_event(snapshot or {})
  local events = snapshot.events or {}
  local shown = plan_rows(snapshot)
  local wide = W >= 620
  local laid = layout(tree(snapshot), W, H)

  rect("worklog-bg", 0, 0, W, H, "background", 1.0, 0)
  local list_pane = find(laid, "worklog-list-pane")
  if list_pane then rect("worklog-list-bg", list_pane.rect.x, list_pane.rect.y, list_pane.rect.width, list_pane.rect.height, "surface", 0.22, 10, Z_PANEL) end
  local detail_pane = find(laid, "worklog-detail-pane")
  if detail_pane then rect("worklog-detail-bg", detail_pane.rect.x, detail_pane.rect.y, detail_pane.rect.width, detail_pane.rect.height, "surface", 0.22, 10, Z_PANEL) end

  local hovered = hover_id()
  local first = window_start(snapshot, shown)
  for index = first, first + shown - 1 do
    local row = find(laid, "event-" .. index)
    if row then
      rect("event-rule-" .. index, row.rect.x + 10, row.rect.y + row.rect.height - 1, row.rect.width - 20, 1, "muted", 0.05, 0, Z_PANEL)
      -- Hover is the host's pointer, not a guess: the id it names is the control
      -- it hit, so the row that lights up is the row under the cursor.
      if hovered == ("event-" .. index) then
        rect("event-hover-" .. index, row.rect.x, row.rect.y, row.rect.width, row.rect.height, "primary", 0.08, 6, Z_PANEL)
      end
    end
  end
  local _, selected_index = selected_event(snapshot)
  local selected = find(laid, "event-" .. tostring(selected_index))
  if selected then
    rect("event-selected", selected.rect.x, selected.rect.y, selected.rect.width, selected.rect.height, "primary", 0.14, 7, Z_PANEL)
  end

  -- One line per event, drawn after its background so the highlight sits under
  -- the text it belongs to.
  for index = first, first + shown - 1 do
    local row = find(laid, "event-" .. index)
    local segments = ROW_SEGMENTS[index]
    if row and segments and on_surface(row.rect) then
      draw_row(row, index, segments)
    end
  end

  -- Locators are drawn as chips behind their label, so a link reads as a thing
  -- you can open rather than as a line of monospaced text.
  local opened = events[selected_index] or events[1]
  local pressed = {}
  for index, link in ipairs((opened and opened.links) or {}) do
    local node = find(laid, "worklog-detail-link-" .. index)
    if node then
      rect("worklog-link-chip-" .. index, node.rect.x - 4, node.rect.y - 2,
        node.rect.width + 8, node.rect.height + 4, "surface", 0.4, 5, Z_CHIP)
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
      local size = node.style.size
      local line_height = node.style.line_height or (size + 4)
      if node.style.wrap then
        local per_line = chars_per_line(node.rect.width, size)
        local lines = wrap_text(node.text, per_line)
        -- The box reserved N lines; draw at most N, because a longer string is
        -- the one case where wrapping alone still overruns the reservation.
        local fits = math.max(1, math.floor(node.rect.height / line_height))
        if #lines > fits then
          local kept = {}
          for index = 1, fits - 1 do kept[index] = lines[index] end
          kept[fits] = clip(lines[fits], per_line)
          lines = kept
        end
        for index, line in ipairs(lines) do
          text(node.id .. "-l" .. index, line, node.rect.x,
            node.rect.y + (index - 1) * line_height, size, ink_role)
        end
      else
        text(node.id, node.text, node.rect.x, node.rect.y, size, ink_role)
      end
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
        action = "worklog-open-locator",
        body = { locator = spec.locator, rel = spec.rel },
        x = nx(spec.rect.x), y = ny(spec.rect.y),
        width = sw(spec.rect.width), height = sh(spec.rect.height),
      }
    end
  end
  M.wide = wide
end

return M
