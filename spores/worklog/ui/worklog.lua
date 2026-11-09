-- Worklog view: a descending timeline of normalized events.
--
-- The snapshot arrives already ordered newest-arrival-first and already filtered
-- (the producer drops lease heartbeats). This file paints it. Re-sorting here
-- would be a second opinion about order, and two opinions is how a timeline
-- starts disagreeing with itself.
--
-- Geometry: this view owns it. The library can hand a tree to a host with a
-- layout service, and Canvas has neither `canvas.layout` nor hit targets, so the
-- view places its own pixels through `ui.surface` — the documented path for a
-- host that draws module primitives (see docs/interaction-gap.md).

-- The library arrives through `require`, which is what the SDK package is for:
-- the composer hands this view a `require` that resolves to the repo's
-- `fungos.ui`, so the view names the library instead of receiving it as an
-- accidental upvalue.
local ui = require("fungos.ui")

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
  -- The host says how big the surface is and the library owns how that is read:
  -- `ui.viewport` resolves `module.viewport`, then the theme's viewport, then the
  -- authoring size. A second opinion here is how a view comes to draw a window it
  -- was not given.
  W, H = ui.viewport(W, H)
end

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

-- The host's search box, as a string. The view decides what matches, because
-- "what counts as a hit" is a fact about the events; the host only owns the text
-- and the caret.
local function query_text()
  local query = _G.module and _G.module.query
  return type(query) == "string" and query or ""
end
-- One surface per render. It owns the pixel-to-scene-unit conversion, the text
-- scale the host applies, and text measurement: doing that arithmetic here (as
-- this view used to) meant a second opinion about the host's own transform, and
-- it was wrong about the text floor for weeks without showing a symptom.
local surface

-- The library's density tokens, read once per render beside the surface.
local tone = { panel = 0.36, selected = 0.16 }

-- How solid a panel, a selected row and a chip are is a design decision the
-- library already publishes (`ui.profile.surfaces`), so the view reads it there
-- instead of picking its own numbers: one density for every Spore, changed in
-- one place. The literal fallbacks are what a profile without surfaces means.
--
-- A hairline is the exception: it is not a density, it is the only way this view
-- separates anything. The host has no border primitive, so a separator is a
-- one-pixel rect of the muted ink, and a table reads as a table without a card.
local HAIRLINE = 0.14

local function surfaces()
  local profile = (ui.profile and ui.profile.surfaces) or {}
  return {
    panel = tonumber(profile.panel) or 0.36,
    selected = tonumber(profile.selected) or 0.16,
  }
end

local function line_height(size) return math.floor(size * 1.3) end

local function rect(id, x, y, w, h, role, opacity, radius, layer)
  surface:rect(id, x, y, w, h, role or "surface", opacity, radius or 8, layer or Z)
end

local function text(id, value, x, y, px, role, z)
  surface:text(id, value, x, y, px, role or "text", z or Z_TEXT)
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


-- How wide text is, in pixels, asked of the surface: the host measures when it
-- can and the library owns the estimate when it cannot. Every caller is on the
-- render path, so `surface` is always the one this frame is drawn into.
local function measure(value, size)
  return surface:measure_text(value, size).width
end

-- Capitals run about a third wider than the mixed-case average the estimate
-- assumes (0.67 em against 0.51, measured off a real Canvas frame), so a masthead
-- or a KIND column reserved at the average ends under the glyphs it labels — and
-- the host does not clip, it draws over the column next to it. One ratio, applied
-- to the one measurement.
local CAPS = 1.3
local function measure_caps(value, size) return measure(value, size) * CAPS end

-- Truncation belongs to the library: `ui.clip` counts code points, so a cut can
-- never land inside a glyph. What this adds is the placeholder an empty field
-- needs, which is a copy decision rather than a text operation.
local function clip(value, limit)
  value = tostring(value or "")
  if value == "" then return "—" end
  return ui.clip(value, limit)
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


-- A row placed past the surface cannot be seen or hit: its normalized
-- coordinates fall outside the frame and the host is right to reject them. The
-- plan bounds the row count, but the draw loop re-checks each rect, because the
-- plan works in pixels from a height it was handed and rounding at the bottom
-- edge is exactly where a half row lands.
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
-- The search field's width, before the window gets a say. Narrower than this and
-- a query of two words is unreadable; wider and it eats the legend.
local SEARCH_W = 260
local ROW_H = MIN_ROW_H
-- The pitch is part of the contract: the host scrolls in pixels and the view
-- turns them into rows, so both sides need the same number.
M.row_height = ROW_H
M.max_row_height = MAX_ROW_H

local function use_type()
  local t = ui.theme()
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
  -- A page with four events in it must not inflate each row to fill the height:
  -- past about four lines of body text a row stops reading as a row and starts
  -- reading as a card. A short log ends where it ends and the count says so.
  MAX_ROW_H = math.max(96, math.floor(TYPE.body * 4.2))
  M.max_row_height = MAX_ROW_H
  -- The toolbar, the list header and the pane padding, which is everything above
  -- the first row.
  CHROME_PX = math.floor(TYPE.title * 1.6) + math.floor(TYPE.caption * 2) + GAP.md * 2 + GAP.sm
  PANE_W = math.floor(W * 0.32)
end

-- Where everything goes. The view computes its own geometry because there is no
-- layout service to ask: the SDK's `ui.layout` is a call *into* the host, Canvas
-- has no `canvas.layout`, and a flow engine carried inside one Spore would be a
-- second layout implementation with one caller. A list, two panes and a toolbar
-- are arithmetic.
local function geometry()
  local toolbar_h = math.floor(TYPE.title * 1.5)
  local main_x = GAP.md
  local main_y = GAP.md + toolbar_h + GAP.sm
  local main_w = W - GAP.md * 2
  local main_h = math.max(120, H - main_y - GAP.md)
  local wide = W >= 620
  local list_w = wide and math.max(240, main_w - GAP.md - PANE_W) or main_w
  local box = {
    toolbar = { x = main_x, y = GAP.md, w = main_w, h = toolbar_h },
    list = { x = main_x, y = main_y, w = list_w, h = main_h },
    detail = wide and { x = main_x + list_w + GAP.md, y = main_y, w = PANE_W, h = main_h } or nil,
    wide = wide,
  }
  -- Inside the list: one line of head, then rows on the pitch the plan chose.
  box.head = { x = box.list.x + GAP.sm, y = box.list.y + GAP.sm, w = box.list.w - GAP.sm * 2, h = line_height(TYPE.caption) }
  box.rows = { x = box.head.x, y = box.head.y + box.head.h + GAP.xs, w = box.head.w }
  if box.detail then
    box.detail_inner = {
      x = box.detail.x + GAP.md,
      y = box.detail.y + GAP.sm,
      w = box.detail.w - GAP.md * 2,
    }
  end
  return box
end

-- A label on the surface: the view places it, so it carries where it goes.
local function placed(id, value, x, y, size, role, width)
  return { id = id, text = clip(value, 240), x = x, y = y, size = size, role = role or "text", width = width or 0 }
end

local function chars_per_line(width, size)
  local sample = "the quick brown fox jumps"
  local per_char = measure(sample, size) / #sample
  if per_char <= 0 then return 8 end
  return math.max(8, math.floor(width / per_char))
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

-- The installed Canvas caps one module scene at a combined node budget (rects +
-- text + controls), well below the tree's 256, so how many rows exist is bounded
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
-- panes, the selection and hover bands, the toolbar, the search field and its
-- rule, and the list header.
local CHROME_FIXED_NODES = 11
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
-- The two fixed columns are as wide as the widest word that will be drawn in
-- them, measured over the whole page rather than sampled from one row. The kind
-- column was reserved for "CHANGED" and a DEPENDENCY row drew its last letter
-- through the producer's mark; the actor column was reserved for one person's
-- name. Both are facts about the snapshot, so both are read from it — and from
-- the whole list, not the window, or the columns would jump while scrolling.
local KIND_W, ACTOR_W = 0, 0
local function size_columns(events)
  KIND_W, ACTOR_W = 0, 0
  for _, event in ipairs(events) do
    KIND_W = math.max(KIND_W, measure_caps(kind_word(event.kind), TYPE.micro))
    ACTOR_W = math.max(ACTOR_W, measure(event.actor or "", TYPE.micro))
  end
end

-- locator, which is the part a reader wants to click.
local function row_segments(event, show_source)
  -- Column widths are measured from the scale, not remembered from it: 92 px of
  -- kind column is right for a 12 px body and wrong for a 15 px one.
  local segments = {
    -- Neutral caps: colour in a row belongs to the producer, and a kind that
    -- also changed colour would be a second opinion about what green means.
    { key = "kind", text = kind_word(event.kind), ink = "muted", size = TYPE.micro,
      w = KIND_W + GAP.sm, line = 1 },
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
    w = ACTOR_W + GAP.md, line = 1 }
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

-- Row content lives beside the geometry, keyed by row index. The plan needs a
-- row's cost before anything of it exists, and the draw pass needs its segments;
-- a table keyed by index is what both can read without a second derivation.
local ROW_SEGMENTS = {}

local function prepare_rows(snapshot, first, shown, show_source)
  local events = snapshot.events or {}
  for index = first, first + shown - 1 do
    if events[index] then ROW_SEGMENTS[index] = row_segments(events[index], show_source) end
  end
end

local function row_rect(box, index, first)
  return {
    x = box.rows.x,
    y = box.rows.y + (index - first) * ROW_H,
    width = box.rows.w,
    height = ROW_H,
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

-- The producer legend. It carries the same mark the rows carry, so a reader
-- learns which glyph means which producer once instead of inferring it from a
-- colour.
local function sources(snapshot)
  local rows = snapshot.sources or {}
  local parts = {}
  for _, source in ipairs(rows) do
    local mark = source_mark(source.id)
    parts[#parts + 1] = string.format("%s %s %d", mark.mark,
      source.label or source.id or "?", source.count or 0)
  end
  return table.concat(parts, "  ·  ")
end

local function head_labels(snapshot, box, first, shown)
  local events = snapshot.events or {}
  local counts = snapshot.counts or {}
  local last = math.min(#events, first + shown - 1)
  local range
  if #events == 0 then
    range = "0 of 0"
  elseif snapshot.matches then
    -- A filtered list says what it is a filter of: "5 of 40 match" alone would
    -- read as thirty-five events that went missing.
    range = string.format("%d–%d · %d of %d match", first, last, snapshot.matches,
      counts.total or #events)
  else
    range = string.format("%d–%d of %d", first, last, counts.total or #events)
  end
  local today = string.format("%d today", counts.today or 0)
  local sort = snapshot.sort or "Newest first"
  local sort_w = measure(sort, TYPE.caption)
  return {
    placed("worklog-range", range, box.head.x, box.head.y, TYPE.caption, "muted", box.head.w),
    placed("worklog-today", today, box.head.x + measure(range, TYPE.caption) + GAP.md, box.head.y,
      TYPE.caption, "muted", measure(today, TYPE.caption)),
    -- The sort state is right-aligned: it is a property of the whole column, and
    -- it is not what the eye should land on first.
    placed("worklog-sort", sort, box.head.x + box.head.w - sort_w, box.head.y,
      TYPE.caption, "muted", sort_w),
  }
end

local function toolbar_labels(snapshot, box)
  local legend = sources(snapshot)
  local title_w = measure_caps(title(snapshot), TYPE.title)
  local labels = {
    placed("worklog-title", title(snapshot), box.x, box.y, TYPE.title, "text", title_w),
    placed("worklog-sources", legend, box.x + title_w + GAP.md, box.y + (TYPE.title - TYPE.caption),
      TYPE.caption, "muted", measure(legend, TYPE.caption)),
  }
  -- The search box is part of the page, so the view draws it: a host that puts an
  -- input over the canvas shows its text through this, and a host that has no
  -- keyboard still shows what it is filtering by.
  local query = query_text()
  local field_w = math.min(SEARCH_W, math.floor(box.w * 0.4))
  local field_h = line_height(TYPE.caption) + GAP.xs
  local field = { x = box.x + box.w - field_w, y = box.y + math.floor((box.h - field_h) / 2),
    w = field_w, h = field_h }
  labels[#labels + 1] = placed("worklog-search-value", query ~= "" and query or "Search",
    field.x + GAP.sm, field.y + GAP.xs, TYPE.caption, query ~= "" and "text" or "muted",
    field.w - GAP.sm * 2)
  return labels, field
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

-- What a query matches: the words a reader would search by — the sentence, who
-- did it, the kind, the producer, and every locator the event carries. Case
-- insensitive substring, because a search box that demands exact spelling is one
-- nobody opens twice. The view decides this because it is a fact about the
-- events; the host only owns the text and the caret.
local function matches(event, needle)
  local fields = { event.summary, event.actor, event.kind, event.source, event.subject, event.id }
  for _, link in ipairs(event.links or {}) do
    fields[#fields + 1] = link.locator
    fields[#fields + 1] = link.rel
  end
  for _, value in ipairs(fields) do
    if tostring(value or ""):lower():find(needle, 1, true) then return true end
  end
  return false
end
M.matches = matches

-- A filtered snapshot is a new snapshot, not a mutated one: the caller keeps the
-- whole log, and the page can ask for it again by dropping the query.
local function with_query(snapshot, needle)
  if needle == "" then return snapshot end
  local kept = {}
  for _, event in ipairs(snapshot.events or {}) do
    if matches(event, needle) then kept[#kept + 1] = event end
  end
  local filtered = {}
  for key, value in pairs(snapshot) do filtered[key] = value end
  filtered.events = kept
  filtered.matches = #kept
  return filtered
end

-- The detail column is the row's footnote made readable: the whole summary, when
-- it happened in wall-clock terms, who did it, and every locator on the event —
-- not just the two a row has room for.
local function detail_labels(snapshot, box)
  local inner = assert(box.detail_inner, "detail_labels needs a detail pane")
  local event = selected_event(snapshot)
  if not event then
    return { placed("worklog-detail-empty", "Select an event", inner.x, inner.y, TYPE.body, "text", inner.w) }
  end
  local labels, y = {}, inner.y
  local function put(id, value, size, role)
    labels[#labels + 1] = placed(id, value, inner.x, y, size, role, inner.w)
    y = y + line_height(size) + GAP.xs
  end

  put("worklog-detail-label", "EVENT", TYPE.micro, "muted")

  -- The summary reserves the lines it wraps to and no more: the host does not
  -- clip, so a box of one line runs under the next one, and a box of eight
  -- around a two-line summary pushes the rest of the column into a hole.
  local lines = detail_lines(event)
  local summary = placed("worklog-detail-summary", clip(event.summary or "", DETAIL_SUMMARY_CHARS),
    inner.x, y, TYPE.body, "text", inner.w)
  summary.lines = lines
  labels[#labels + 1] = summary
  y = y + lines * line_height(TYPE.body) + GAP.sm

  put("worklog-detail-when", (event.at_text or "") .. "  ·  " .. (event.age or ""), TYPE.micro, "muted")
  -- The mark, not the producer's name: the row already carries the mark, and the
  -- pane is where a reader wants the actor.
  put("worklog-detail-who", source_mark(event.source).mark .. "  " .. (event.actor or ""), TYPE.micro, "muted")
  put("worklog-detail-kind", kind_word(event.kind), TYPE.micro, "muted")

  local links = event.links or {}
  put("worklog-detail-open-label", #links > 0 and "OPEN" or "LOCATORS", TYPE.micro, "muted")
  if #links == 0 then
    put("worklog-detail-nolinks", "no locators", TYPE.micro, "muted")
  end
  for index, link in ipairs(links) do
    local pill_h = math.floor(TYPE.micro * 1.8)
    local label = locator_label(link.locator)
    labels[#labels + 1] = {
      id = "worklog-detail-link-" .. index, text = label, x = inner.x + GAP.sm, y = y + GAP.xs,
      size = TYPE.micro, role = "primary", width = measure(label, TYPE.micro),
      chip = { rect = { x = inner.x, y = y, width = measure(label, TYPE.micro) + GAP.sm * 2, height = pill_h },
               locator = link.locator, rel = link.rel },
    }
    y = y + pill_h + GAP.xs
  end
  return labels
end

local function apply_event(snapshot)
  -- The host routes an activation into this render; `ui.event()` is the
  -- library's way of asking, so the view does not read the host's table shape.
  local event = ui.event()
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
    local chars = chars_per_line(chip_room - used - 10, segment.size)
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
      local chars = chars_per_line(summary_room, segment.size)
      if chars >= 8 then
        text(id, clip(segment.text, chars), x, y, segment.size, segment.ink)
      end
      x = x + summary_room + GAP.xs
    elseif segment.chip then
      local piece = chip_widths[segment]
      if piece then
        rect(id .. "-chip", x - 3, y - 3, piece.width, chip_h, "primary", tone.selected * 0.45, 4, Z_CHIP)
        text(id, piece.label, x + 2, y, segment.size, "primary")
        press {
          id = id .. "-press", label = segment.chip,
          action = "worklog-open-locator",
          body = { locator = segment.chip, rel = segment.rel or "subject" },
          x = surface:nx(x - 3), y = surface:ny(y - 3),
          width = surface:sw(piece.width), height = surface:sh(chip_h),
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
local function draw_row(row, index, segments)
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
  surface = ui.surface(W, H, ui.theme())
  tone = surfaces()
  -- The theme's type scale decides every height below, so it is read before
  -- anything is planned.
  use_type()
  snapshot = apply_event(snapshot or {})
  snapshot = with_query(snapshot, query_text():lower())
  local events = snapshot.events or {}
  local shown = plan_rows(snapshot)
  local first = window_start(snapshot, shown)
  local show_source = #(snapshot.sources or {}) > 1
  local box = geometry()
  size_columns(events)
  prepare_rows(snapshot, first, shown, show_source)

  rect("worklog-bg", 0, 0, W, H, "background", 1.0, 0)
  rect("worklog-list-bg", box.list.x, box.list.y, box.list.w, box.list.h, "surface", tone.panel, 6, Z_PANEL)
  if box.detail then
    rect("worklog-detail-bg", box.detail.x, box.detail.y, box.detail.w, box.detail.h, "surface", tone.panel, 6, Z_PANEL)
  end
  -- A rule under the masthead and one between the columns: the panes are told
  -- apart by their edges, not by two shades of grey meeting in the middle.
  rect("worklog-toolbar-rule", box.toolbar.x, box.toolbar.y + box.toolbar.h + GAP.sm / 2 - 0.5,
    box.toolbar.w, 1, "muted", HAIRLINE, 0, Z_PANEL)
  if box.detail then
    rect("worklog-column-rule", box.list.x + box.list.w + GAP.md / 2 - 0.5, box.list.y, 1, box.list.h,
      "muted", HAIRLINE, 0, Z_PANEL)
  end

  local toolbar, field = toolbar_labels(snapshot, box.toolbar)
  rect("worklog-search", field.x, field.y, field.w, field.h, "surface", tone.panel, 5, Z_CHIP)
  -- A wash alone disappears on a dark ground, and the host has no border: the
  -- field is told apart by the same hairline that separates everything else
  -- here.
  rect("worklog-search-rule", field.x, field.y + field.h, field.w, 1, "muted", HAIRLINE, 0, Z_CHIP)
  for _, item in ipairs(toolbar) do
    text(item.id, item.text, item.x, item.y, item.size, item.role)
  end
  for _, item in ipairs(head_labels(snapshot, box, first, shown)) do
    text(item.id, item.text, item.x, item.y, item.size, item.role)
  end
  if #events == 0 then
    local message = query_text() == "" and "Nothing has arrived yet."
      or string.format("Nothing matches “%s”.", clip(query_text(), 40))
    text("worklog-empty", message, box.rows.x, box.rows.y, TYPE.body, "muted")
  end

  local hovered = hover_id()
  local _, selected_index = selected_event(snapshot)
  if selected_index >= first and selected_index < first + shown then
    local band = row_rect(box, selected_index, first)
    rect("event-selected", band.x, band.y, band.width, band.height, "primary", tone.selected * 0.6, 5, Z_PANEL)
  end

  -- `plan_rows` budgets an average row, so the draw stops when the real cost
  -- reaches the ceiling: the row that would cross it is dropped whole rather
  -- than half-drawn into a scene the host refuses. The first row always draws,
  -- because a page with nothing on it is worse than a page over budget.
  local reserved = CHROME_FIXED_NODES + detail_nodes(selected_event(snapshot))
  local spent, drawn_rows = 0, 0
  for index = first, first + shown - 1 do
    local segments = ROW_SEGMENTS[index]
    local row = segments and row_rect(box, index, first) or nil
    if not (row and on_surface(row)) then break end
    local cost = row_nodes(events[index], show_source)
    if drawn_rows > 0 and reserved + spent + cost > NODE_BUDGET then break end
    -- Hover is the host's pointer, not a guess: the id it names is the control
    -- it hit, so the row that lights up is the row under the cursor.
    if hovered == ("event-" .. index) then
      rect("event-hover-" .. index, row.x, row.y, row.width, row.height, "primary", tone.selected * 0.3, 5, Z_PANEL)
    end
    rect("event-rule-" .. index, row.x + GAP.sm, row.y + row.height - 1, row.width - GAP.sm * 2, 1, "muted", HAIRLINE / 2, 0, Z_PANEL)
    draw_row(row, index, segments)
    spent = spent + cost
    drawn_rows = drawn_rows + 1
  end
  M.rows_drawn, M.rows_planned = drawn_rows, shown
  -- What the page needs to clamp a scroll: how far it got, and how far there is.
  -- The view owns the window, so it is the one that can say where it ends.
  M.window = { first = first, shown = drawn_rows, total = #events }

  if box.detail then
    for _, item in ipairs(detail_labels(snapshot, box)) do
      if item.chip then
        -- A locator is a pill you can press: the label is its short id, and the
        -- full locator travels in the press for whoever owns the scheme.
        local chip = item.chip
        rect(item.id .. "-chip", chip.rect.x, chip.rect.y, chip.rect.width, chip.rect.height,
          "primary", tone.selected * 0.45, 4, Z_CHIP)
        text(item.id, item.text, item.x, item.y, item.size, item.role)
        press {
          id = "worklog-link-" .. item.id:match("(%d+)$"),
          label = chip.locator,
          action = "worklog-open-locator",
          body = { locator = chip.locator, rel = chip.rel },
          x = surface:nx(chip.rect.x), y = surface:ny(chip.rect.y),
          width = surface:sw(chip.rect.width), height = surface:sh(chip.rect.height),
        }
      else
        local lines = wrap_text(item.text, chars_per_line(item.width, item.size))
        local fits = item.lines or 1
        for line_index = 1, math.min(#lines, fits) do
          local id = fits > 1 and (item.id .. "-l" .. line_index) or item.id
          text(id, lines[line_index], item.x, item.y + (line_index - 1) * line_height(item.size),
            item.size, item.role)
        end
      end
    end
  end

  -- The search box is a control the host may put its own input over; the spec is
  -- how it learns where. It is a hit rather than a press: nothing is being asked
  -- of the host except where the field is.
  hit {
    id = "worklog-search", label = "field",
    x = surface:nx(field.x), y = surface:ny(field.y),
    width = surface:sw(field.w), height = surface:sh(field.h),
  }

  for index = first, first + drawn_rows - 1 do
    local row = row_rect(box, index, first)
    if on_surface(row) then
      hit {
        id = "event-" .. index, label = "hit",
        x = surface:nx(row.x), y = surface:ny(row.y),
        width = surface:sw(row.width), height = surface:sh(row.height),
      }
    end
  end

  M.wide = box.wide
end

return M
