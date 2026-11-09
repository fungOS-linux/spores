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

-- A text node carries a scale, and the host turns it into pixels with
-- `max(|scale_y| * theme.body * depth(z), theme.micro)` where
-- `depth(z) = 0.5 + ((z + 4) / 8) * 1.5`. Two consequences, both invisible from
-- here until the wall disagreed with the preview:
--
--   * dividing by `body * 1.25` made every label 20 % smaller than its number.
--   * the floor is `theme.micro` — 15 px on the wall's 1.5x theme — so 9, 10 and
--     12 px all came out as 15. Measured off a real Canvas frame, every label was
--     15 px wide-body and the 16 px one was 16.5: the view had no hierarchy at
--     all, which is why it read as one undifferentiated wall of text.
--
-- So the depth factor is divided back out, and a number in this file is the
-- number of pixels the wall draws. The type scale itself comes from the theme
-- (see `use_type`), because the theme is what the host will honour.
local function depth(z) return 0.5 + (((z or 0) + 4) / 8) * 1.5 end
local function ts(px) return px / (base() * depth(Z_TEXT)) end

-- One line of type occupies 30 % more than its size. The layout reserves with
-- this and the emit path draws with it, so the box a label was given is the box
-- its ink fills.
local function line_height(size) return math.floor(size * 1.3) end

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
    -- The host maps this straight onto the renderer's border radius, in pixels.
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

-- One mark and one colour per producer, so a row says where it came from without
-- spending a column on the word. These are characters the system font has: a
-- Nerd Font icon would be prettier and does not draw here, because `canvas.text`
-- carries no font and the host's own font has no fallback for private-use
-- codepoints (measured: every nerd glyph renders as a tofu box).
local SOURCE_MARK = {
  marbles = { mark = "\u{25c6}", ink = "primary" },     -- the tracker's marble
  github = { mark = "#", ink = "success" },             -- the number a PR is known by
  confluence = { mark = "\u{2261}", ink = "warning" }, -- a page of lines
  unibus = { mark = "\u{25e6}", ink = "info" },
}

local function source_mark(source)
  return SOURCE_MARK[tostring(source or "")] or { mark = "\u{2022}", ink = "muted" }
end

-- Characters a line holds at this size. The host measures text; a view that
-- cannot ask must estimate. Measured off a real Canvas frame — "listeners as"
-- and "myceliumd/services.r" at 15 px, and the timestamp line at 27 chars — the
-- advance runs 0.41 em for narrow lowercase to 0.67 em for capitals, averaging
-- 0.51. Too far above the average and every line ends in an ellipsis with a hole
-- beside it; too far below and the line runs under the column next to it.
local GLYPH_W = 0.52

local function measure(value, size)
  return #tostring(value or "") * size * GLYPH_W
end

-- Capitals run about 0.7 em per character — a third wider than the mixed-case
-- average above. A masthead or a KIND column sized with the average runs past
-- its own box, and the host does not clip: it draws over the next column.
local function measure_caps(value, size)
  return #tostring(value or "") * size * 0.7
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
    style = { size = size, line_height = line_height(size), wrap = layout.wrap == true },
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

-- One type scale and one spacing scale for the whole view. The sizes come from
-- the host's own tokens: a theme scaled for a wall hands over micro/small/body/
-- large in pixels, and a view that invents 9/10/12/16 falls under the host's
-- floor (see `ts`), which is how every label on the wall became one size. The
-- numbers here are the fallback for a host that publishes no type scale.
local FALLBACK_TYPE = { micro = 10, caption = 12, body = 16, title = 18 }
local TYPE = { micro = 10, caption = 12, body = 16, title = 18 }
local GAP = { xs = 4, sm = 8, md = 16 }

-- A token is a length in pixels. Anything smaller than a word or larger than a
-- paragraph is a fraction or a mistake, and either way it is not a font size, so
-- the fallback is used instead of drawing the view at 0.145 px.
local function token(value, fallback)
  local number = tonumber(value)
  if number and number >= 4 and number <= 200 then return number end
  return fallback
end

-- Row and chrome geometry in pixels, derived from the scale rather than fixed: a
-- theme at 1.5x hands over a 24 px body, and a 44 px row cannot hold two lines
-- of it.
local MIN_ROW_H = 44
local MAX_ROW_H = 132
local TALL_ROW_H = 56
local CHROME_PX = 120
local PANE_W = 300
local ROW_H = MIN_ROW_H
-- The pitch is part of the contract: the host scrolls in pixels and the view
-- turns them into rows, so both sides need the same number.
M.row_height = ROW_H

local function use_type()
  local t = theme()
  TYPE.micro = token(t.micro, FALLBACK_TYPE.micro)
  TYPE.caption = token(t.small, FALLBACK_TYPE.caption)
  TYPE.body = token(t.body, FALLBACK_TYPE.body)
  TYPE.title = token(t.large, FALLBACK_TYPE.title)
  GAP.xs = math.max(4, math.floor(TYPE.body / 3))
  GAP.sm = math.max(8, math.floor(TYPE.body / 1.5))
  GAP.md = math.max(16, math.floor(TYPE.body * 1.33))
  -- Two lines need a micro line and a body line plus the gap around them; a
  -- row shorter than that is a one-line row whatever the page would prefer.
  TALL_ROW_H = math.floor(TYPE.micro * 1.3 + TYPE.body * 1.35) + GAP.sm
  MIN_ROW_H = math.max(44, math.floor(TYPE.body * 2.4))
  MAX_ROW_H = math.max(132, math.floor(TYPE.body * 6))
  -- The toolbar, the list header and the pane padding, which is everything above
  -- the first row.
  CHROME_PX = math.floor(TYPE.title * 1.6) + math.floor(TYPE.caption * 2) + GAP.md * 2 + GAP.sm
  PANE_W = math.floor(W * 0.32)
end

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
-- height that is left. A row between one and three lines tall can carry a header
-- and a summary, which is what a log needs to read like one.
-- Eight lines is a summary, not a document. Past that the column stops being a
-- column and the log stops being scannable.
local SUMMARY_LINES = 8
M.summary_lines = SUMMARY_LINES
-- How much of the summary the column will show at all. The same number clips the
-- string and counts the lines it wraps to, because a box measured from a longer
-- string than the one drawn reserves lines that stay empty.
local DETAIL_SUMMARY_CHARS = 240

-- Canvas caps one module scene at 256 nodes, so how many rows exist is bounded
-- by what a row costs as well as by how much height is left; asking for the rows
-- a tall window could hold produced a scene the host rejected outright and the
-- whole surface became an error card.
--
-- Those costs are counted, not guessed. The estimate this replaces ("about
-- eighty nodes of chrome, nine a row") was five times the chrome's real cost and
-- priced every row at its worst case, which put 17 rows on a 1,895 px surface —
-- a 104 px pitch holding two lines of ink.
--
-- The ceiling is what the installed Canvas accepts, not the tree's default. A page
-- costing 234 combined nodes (rects + text + controls, which is what
-- `render_module` adds up) came back "Canvas module controls exceed combined node
-- budget" and the wall kept its previous scene; the same view at 190 draws. Other
-- threads are raising `DEFAULT_MAX_NODES` in canvas itself (1,024 and 8,192 in two
-- open worktrees), so when that ships this constant is the one line that moves and
-- the row plan follows it back up.
local NODE_BUDGET = 190 -- measured on the wall; the tree's own default is 256
local ROW_BASE_NODES = 5 -- the rule under the row, the kind, the actor, the summary, the age
local SOURCE_NODES = 1 -- the source column, when more than one source is reporting
local CHIP_NODES = 3 -- a chip's pill, its label, and the press target over it
-- The chrome that is there whether or not there are rows: the backdrop, the two
-- panes, the selection and hover bands, the toolbar and the list header.
local CHROME_FIXED_NODES = 10
-- A row carries at most this many locators: one reference is a pointer, two is a
-- join. Past that the row stops being a line you can read at a glance, and the
-- rest of them belong in the detail column, which lists all of them.
local MAX_ROW_CHIPS = 2

local function row_chips(event)
  local seen, chips = {}, 0
  for _, link in ipairs(event.links or {}) do
    if link.locator and not seen[link.locator] then
      seen[link.locator] = true
      chips = chips + 1
    end
  end
  local subject = event.subject or ""
  if subject:find("://", 1, true) and not seen[subject] then chips = chips + 1 end
  return math.min(chips, MAX_ROW_CHIPS)
end

-- One row's node cost, whether it is being counted for the plan or paid for at
-- draw time. Two functions guessing at the same number is how a page came out
-- twenty nodes over the ceiling.
local function row_nodes(event, show_source)
  return ROW_BASE_NODES + (show_source and SOURCE_NODES or 0) + CHIP_NODES * row_chips(event or {})
end

-- How many lines the detail column's summary actually occupies. Counted with the
-- same word wrap the emit path uses, because reserving a box of eight lines for a
-- two-line summary leaves the column's metadata a hundred pixels below the text
-- it describes, and reserving two for a three-line one draws over it.
local function detail_lines(event)
  local per_line = chars_per_line(PANE_W - GAP.md * 2, TYPE.body)
  return math.min(SUMMARY_LINES, #wrap_text(clip(event.summary or "", DETAIL_SUMMARY_CHARS), per_line))
end

-- What the detail column draws: the one part of the chrome whose cost depends on
-- the page, because it wraps the summary it is given and lists every locator.
-- Keep this in step with `detail()`, which draws them.
local function detail_nodes(event)
  if not event then return 1 end -- the "Select an event" placeholder
  local links = #(event.links or {})
  return 5 + detail_lines(event) + (links > 0 and CHIP_NODES * links or 1)
end

-- How many rows the window holds, and how tall each one is. Both answers come
-- from one place: a list that filled the height with rows but kept a short pitch
-- would still leave the page half empty.
local function plan_rows(snapshot)
  local events = snapshot.events or {}
  local total = #events
  -- An empty log shows zero rows, not the max(1, …) floor: there is no first
  -- event to draw.
  if total == 0 then
    ROW_H = MIN_ROW_H
    M.row_height = ROW_H
    return 0
  end
  local chrome = CHROME_FIXED_NODES + detail_nodes(selected_event(snapshot))
  local show_source = #(snapshot.sources or {}) > 1
  local rows_cost = 0
  for _, event in ipairs(events) do rows_cost = rows_cost + row_nodes(event, show_source) end
  -- Average, not worst case: budgeting every row on the page for the densest row
  -- on it is what starved the list. The draw loop stops at the budget anyway, so
  -- an unusually chipped page costs its last row rather than an error card.
  local per_row = rows_cost / total
  local space = math.max(0, H - CHROME_PX)
  local fits = math.max(1, math.floor(space / MIN_ROW_H))
  local affordable = math.max(1, math.floor((NODE_BUDGET - chrome) / per_row))
  local rows = math.min(total, affordable, fits)
  ROW_H = math.max(MIN_ROW_H, math.min(MAX_ROW_H, math.floor(space / rows)))
  M.row_height = ROW_H
  return rows
end

-- A row is one line with things embedded in it: the kind, who did it, the
-- locator as a chip you can press, the summary, and how long ago. Stacked lines
-- spent three rows' worth of height per event and still could not hold a
-- locator, which is the part a reader wants to click.
local function row_segments(event, show_source)
  -- Column widths are measured from the scale, not remembered from it: 92 px of
  -- kind column is right for a 12 px body and wrong for a 15 px one.
  local segments = {
    -- Neutral caps: colour in a row belongs to the producer, and a kind that
    -- also changed colour would be a second opinion about what green means.
    { key = "kind", text = kind_word(event.kind), ink = "muted", size = TYPE.micro,
      w = measure_caps("CHANGED", TYPE.micro) + GAP.sm, line = 1 },
  }
  -- One source means the column is the same word on every row: it costs space
  -- and tells the reader nothing.
  if show_source then
    -- The mark says which producer, in the colour that producer owns.
    local mark = source_mark(event.source)
    segments[#segments + 1] = { key = "source", text = mark.mark, ink = mark.ink, size = TYPE.micro,
      w = measure_caps(mark.mark, TYPE.micro) + GAP.sm, line = 1 }
  end
  segments[#segments + 1] = { key = "actor", text = event.actor or "", ink = "muted", size = TYPE.micro,
    w = measure("ajmwagar", TYPE.micro) + GAP.md, line = 1 }
  -- The summary comes before the locator: the sentence is the line, the chip is
  -- the footnote on it. Put the chip first and a long locator eats the row.
  segments[#segments + 1] = { key = "summary", text = event.summary or "", ink = "text", size = TYPE.body, grow = true, line = ROW_H >= TALL_ROW_H and 2 or 1 }
  -- At most `MAX_ROW_CHIPS` of them, in the order the producer listed them, which
  -- is the order `row_chips` counts. The detail column carries the rest.
  local seen, chips = {}, 0
  for position, link in ipairs(event.links or {}) do
    if chips < MAX_ROW_CHIPS and link.locator and not seen[link.locator] then
      seen[link.locator] = true
      chips = chips + 1
      segments[#segments + 1] = { key = "locator-" .. position, chip = link.locator, rel = link.rel, size = TYPE.micro, line = 1 }
    end
  end
  local subject = event.subject or ""
  if chips < MAX_ROW_CHIPS and subject:find("://", 1, true) and not seen[subject] then
    seen[subject] = true
    chips = chips + 1
    segments[#segments + 1] = { key = "subject", chip = subject, rel = "subject", size = TYPE.micro, line = 1 }
  end
  segments[#segments + 1] = { key = "age", text = event.age or "", ink = "muted", size = TYPE.micro,
    w = measure("22m", TYPE.micro) + GAP.xs, align = "right", line = 1 }
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
      id = "worklog-head", kind = "container", layout = { direction = "row", height = line_height(TYPE.caption) },
      children = {
        -- The title is the toolbar's, not the list's: drawing it twice spent a
        -- line saying nothing the masthead had not already said.
        label("worklog-range", range, TYPE.caption, { flex_grow = 1 }),
        label("worklog-sort", snapshot.sort or "Newest first", TYPE.caption,
          { width = measure("Newest first", TYPE.caption) + GAP.sm }),
      },
    },
  }
  if #events == 0 then
    children[#children + 1] = label("worklog-empty", "Nothing has arrived yet.", TYPE.body, { height = line_height(TYPE.body) })
  end
  for index = first, last do
    children[#children + 1] = event_row(events[index], index, show_source)
  end
  return { id = "worklog-list", kind = "container", layout = { flex_grow = 1, direction = "column", gap = 0 }, children = children }
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
  -- A Confluence locator is `<space>/<page-id>` with no `//`: the space is the
  -- part a reader recognises, so it survives and the id is appended the way a
  -- GitHub number is.
  local space, id = value:match("^confluence:([^/]+)/(.+)$")
  if space then return space .. "#" .. id end
  local repo, number = value:match("^(.+)#(%d+)$")
  if repo then return (repo:match("([^/]+)$") or repo) .. "#" .. number end
  return value
end
M.locator_label = locator_label

-- The detail column is the row's footnote made readable: the whole summary, when
-- it happened in wall-clock terms, who did it, and every locator on the event —
-- not just the two a row has room for.
local function detail(snapshot)
  local event = selected_event(snapshot)
  if not event then
    return { id = "worklog-detail", kind = "container", layout = {}, children = {
      label("worklog-detail-empty", "Select an event", TYPE.body, {}) } }
  end
  local links = event.links or {}
  -- A wrapped label needs the height its lines actually take, and only the lines
  -- it takes. The layout gives a node exactly what it asks for and the host does
  -- not clip: a box of one line lets the ink run over whatever is stacked next,
  -- and a box of eight lines around a two-line summary pushes the rest of the
  -- column a hundred pixels below the text it describes.
  local children = {
    label("worklog-detail-label", "EVENT", TYPE.micro, { height = line_height(TYPE.micro) }),
    label("worklog-detail-summary", clip(event.summary or "", DETAIL_SUMMARY_CHARS), TYPE.body,
      { height = detail_lines(event) * line_height(TYPE.body), wrap = true }),
    label("worklog-detail-when", (event.at_text or "") .. "  ·  " .. (event.age or ""), TYPE.micro, { height = line_height(TYPE.micro) }),
    -- The mark, not the producer's name: the row already carries the mark, and
    -- the pane is where a reader wants the actor.
    label("worklog-detail-who", source_mark(event.source).mark .. "  " .. (event.actor or ""), TYPE.micro,
      { height = line_height(TYPE.micro) }),
    label("worklog-detail-kind", kind_word(event.kind), TYPE.micro, { height = line_height(TYPE.micro) }),
    label("worklog-detail-open-label", #links > 0 and "OPEN" or "LOCATORS", TYPE.micro, { height = line_height(TYPE.micro) }),
  }
  if #links == 0 then
    children[#children + 1] = label("worklog-detail-nolinks", "no locators", TYPE.micro, { height = line_height(TYPE.micro) })
  end
  for index, link in ipairs(links) do
    -- The same rule as a row: a locator draws as its short id and the full URI
    -- travels in the press. The pane is 268 px wide, and the transport in front
    -- of `mycelium-6us5` is what pushed the chip off the right edge.
    children[#children + 1] = label("worklog-detail-link-" .. index,
      locator_label(link.locator), TYPE.micro, { height = math.floor(TYPE.micro * 1.8) })
  end
  return { id = "worklog-detail-page", kind = "container", layout = { flex_grow = 1, direction = "column", gap = GAP.sm }, children = children }
end

local function sources(snapshot)
  local rows = snapshot.sources or {}
  local parts = {}
  for _, source in ipairs(rows) do
    -- The legend carries the same mark the rows carry, so the mapping is read
    -- once and not guessed from a colour.
    local mark = source_mark(source.id)
    parts[#parts + 1] = string.format("%s %s %d", mark.mark,
      source.label or source.id or "?", source.count or 0)
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
        id = "worklog-toolbar", kind = "container", layout = { direction = "row", height = math.floor(TYPE.title * 1.5) },
        children = {
          label("worklog-title", title(snapshot), TYPE.title,
            { width = measure_caps(title(snapshot), TYPE.title) + GAP.sm }),
          label("worklog-sources", sources(snapshot), TYPE.caption, { flex_grow = 1 }),
          label("worklog-today", string.format("%d today", counts.today or 0), TYPE.caption,
            { width = measure("0000 today", TYPE.caption) + GAP.sm }),
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



-- One line of a row: fixed columns first, the age at the far right, chips given
-- a share of what is left, and the summary taking the remainder.
local function draw_line(parts, index, left, right, y, age_w, chip_h, chip_share)
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
  -- The sentence is the line and the chip is the footnote on it. On a one-line
  -- row a chip given half the width left the summary four words and an ellipsis,
  -- which is the wrong trade: the detail column lists every locator anyway.
  local chip_room = math.max(0, math.min(available * (chip_share or 0.5), 320))
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
        rect(id .. "-chip", x - 3, y - 3, piece.width, chip_h, "primary", 0.16, 5, Z_CHIP)
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
  local pad = GAP.sm
  local left = row.x + pad
  local right = row.x + row.width - pad
  local age_w = measure("22m", TYPE.micro)
  local tall = ROW_H >= TALL_ROW_H
  local line_h = tall and (TYPE.micro + 14) or (TYPE.body + 4)
  local lines = tall and 2 or 1
  local top = row.y + (ROW_H - ((lines - 1) * line_h + TYPE.body)) / 2
  local chip_h = line_height(TYPE.micro) + GAP.xs

  local grouped = {}
  for _, segment in ipairs(segments) do
    local line = math.min(segment.line or 1, lines)
    grouped[line] = grouped[line] or {}
    grouped[line][#grouped[line] + 1] = segment
  end
  for line = 1, lines do
    local parts = grouped[line]
    if parts and #parts > 0 then
      draw_line(parts, index, left, right, top + (line - 1) * line_h, age_w, chip_h, tall and 0.5 or 0.32)
    end
  end
end

function M.render(snapshot)
  use_viewport()
  -- The theme's type scale decides every height below, so it is read before
  -- anything is planned.
  use_type()
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
  local _, selected_index = selected_event(snapshot)
  local selected = find(laid, "event-" .. tostring(selected_index))
  if selected then
    rect("event-selected", selected.rect.x, selected.rect.y, selected.rect.width, selected.rect.height, "primary", 0.14, 7, Z_PANEL)
  end

  -- `plan_rows` budgets an average row, so the draw stops when the real cost
  -- reaches the ceiling: the row that would cross it is dropped whole rather
  -- than half-drawn into a scene the host refuses. The first row always draws,
  -- because a page with nothing on it is worse than a page over budget.
  local reserved = CHROME_FIXED_NODES + detail_nodes(selected_event(snapshot))
  local show_source = #(snapshot.sources or {}) > 1
  local spent, drawn_rows = 0, 0
  for index = first, first + shown - 1 do
    local row = find(laid, "event-" .. index)
    local segments = ROW_SEGMENTS[index]
    if not (row and segments and on_surface(row.rect)) then break end
    local cost = row_nodes(events[index], show_source)
    if drawn_rows > 0 and reserved + spent + cost > NODE_BUDGET then break end
    -- Hover is the host's pointer, not a guess: the id it names is the control
    -- it hit, so the row that lights up is the row under the cursor.
    if hovered == ("event-" .. index) then
      rect("event-hover-" .. index, row.rect.x, row.rect.y, row.rect.width, row.rect.height, "primary", 0.08, 6, Z_PANEL)
    end
    -- The hairline is drawn with the row, not in a pass of its own: a rule for a
    -- row the budget dropped is ink with nothing under it.
    rect("event-rule-" .. index, row.rect.x + 10, row.rect.y + row.rect.height - 1, row.rect.width - 20, 1, "muted", 0.05, 0, Z_PANEL)
    draw_row(row, index, segments)
    spent = spent + cost
    drawn_rows = drawn_rows + 1
  end
  M.rows_drawn = drawn_rows
  M.rows_planned = shown

  -- Locators are drawn as chips behind their label, so a link reads as a thing
  -- you can open rather than as a line of monospaced text.
  local opened = events[selected_index] or events[1]
  local pressed = {}
  local pane_right = detail_pane
    and (detail_pane.rect.x + detail_pane.rect.width - GAP.md)
    or W
  for index, link in ipairs((opened and opened.links) or {}) do
    local node = find(laid, "worklog-detail-link-" .. index)
    if node then
      -- The pill wraps its label, not the column: a text node laid out in a
      -- column takes the column's width, and a chip drawn to that width reads as
      -- a text field rather than as a thing you press. Clamped to the pane,
      -- which is how a chip two pixels wider than the pane crossed the edge.
      local chip_x = node.rect.x - 4
      local label = locator_label(link.locator)
      local chip_w = math.min(#label * TYPE.micro * GLYPH_W + 16, pane_right - chip_x)
      chip_w = math.max(24, chip_w)
      rect("worklog-link-chip-" .. index, chip_x, node.rect.y - 2,
        chip_w, node.rect.height + 4, "primary", 0.16, 5, Z_CHIP)
      pressed[#pressed + 1] = {
        id = "worklog-link-" .. index,
        label = link.locator,
        locator = link.locator, rel = link.rel,
        rect = { x = chip_x, y = node.rect.y - 2, width = chip_w, height = node.rect.height + 4 },
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

  for index = first, first + drawn_rows - 1 do
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
