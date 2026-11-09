-- fungos/ui — declarative UI for Spore views.
--
-- A view returns a tree; this library lays it out in a **single pass** and emits
-- Canvas primitives. Everything comes from `module.theme`, so a theme change
-- moves the whole interface and no view names a colour or a pixel.
--
-- Three rules keep it honest:
--   1. The view is pure. It receives a snapshot and returns a tree. It holds no
--      state; state lives in the Spore and arrives in the snapshot.
--   2. Interaction is an *action*, not a callback. `ui.action("save")` emits an
--      intent into the scene; the Spore decides what it means.
--   3. Layout is one pass. Every node returns its height and its primitives
--      together, so a parent never re-measures what a child already drew. Two
--      passes drift apart the moment a wrap decision differs, and the drift
--      shows up as overlapping text.
--
-- Tokens are the only vocabulary: spacing, radii, text styles, and ink roles are
-- named, and an unknown name is an error rather than a silent zero.

local ui = {}

-- ---- tokens -------------------------------------------------------------

-- Tokens are **logical pixels**. The host projects an 8-unit square onto the
-- surface, so units are per-axis: 8 units across the width, 8 down the height.
-- Converting here, once, is what keeps a themed 26px label 26px tall on any
-- window; the previous code divided by a constant calibrated to one screen.
local SPACE = { none = 0, xs = 4, sm = 8, md = 16, lg = 28, xl = 44 }
local RADIUS = { none = 0, sm = 4, md = 8, lg = 12, pill = 999 }
local STYLE = {
  display = "title", title = "title", heading = "large",
  subheading = "large", body = "body", caption = "micro", mono = "small",
}
local INK = {
  background = true, surface = true, surface_raised = true, surface_sunken = true,
  border = true, divider = true, text = true, muted = true,
  primary = true, accent = true, success = true, warning = true, danger = true,
  info = true,
}
-- Canvas's Lua scene ABI has a smaller semantic palette than the UI vocabulary.
-- Keep the richer token names for component authors, but normalize them before
-- they reach canvas.rect/canvas.text. Otherwise a tree that previews correctly
-- in the native test harness is rejected by the compositor.
local INK_ALIAS = {
  accent = "info",
  surface_raised = "surface",
  surface_sunken = "background",
  border = "muted",
  divider = "muted",
}

ui.tokens = {
  space = {},
  radius = {},
  style = STYLE,
  ink = INK,
  chrome = { titlebar = 28, caption = 18, grip = 8, inset = 4 },
  motion = { fast = 120, base = 180, slow = 280 },
}
ui.tokens.space = SPACE
ui.tokens.radius = RADIUS
ui.gap = ui.tokens.space
ui.radius = ui.tokens.radius
ui.leading = 1.55

local function fail(message, level)
  error(message, (level or 2) + 1)
end

-- Rule 1: spacing is a token, or a multiple of the 4px base. Nothing else.
local function space(value, field)
  if value == nil then return 0 end
  if type(value) == "string" then
    local token = ui.gap[value]
    if not token then
      fail(("%s: %q is not a space token (none/xs/sm/md/lg/xl)"):format(field, value), 3)
    end
    return token
  end
  local steps = value / 4
  if math.abs(steps - math.floor(steps + 0.5)) > 1e-6 then
    fail(("%s: %g is not a multiple of the 4px base; use a token"):format(field, value), 3)
  end
  return value
end

-- Rule 1, radii: a token name, or a value that is exactly a token.
local function radius(value, field)
  if value == nil then return 0 end
  if type(value) == "string" then
    local token = ui.radius[value]
    if not token then
      fail(("%s: %q is not a radius token (none/sm/md/lg/pill)"):format(field, value), 3)
    end
    return token
  end
  for _, token in pairs(ui.radius) do
    if math.abs(token - value) < 1e-9 then return value end
  end
  fail(("%s: %g is not a radius token (none/sm/md/lg/pill)"):format(field, value), 3)
end

-- Rule 3: colour is a role, never a value.
local function ink(value, field)
  if value == nil then return nil end
  if not INK[value] then
    local names = {}
    for name in pairs(INK) do names[#names + 1] = name end
    table.sort(names)
    fail(("%s: %q is not an ink role (%s)"):format(field, value, table.concat(names, "/")), 3)
  end
  return INK_ALIAS[value] or value
end

-- Rule 2: text uses a named style.
local function style(value)
  value = value or "body"
  if not STYLE[value] then
    local names = {}
    for name in pairs(STYLE) do names[#names + 1] = name end
    table.sort(names)
    fail(("unknown text style %q (%s)"):format(value, table.concat(names, "/")), 3)
  end
  return value
end

local function size_for(theme, name)
  return theme[STYLE[style(name)]]
end

-- ---- nodes --------------------------------------------------------------

local function node(kind, props)
  props.kind = kind
  props.children = props.children or {}
  -- Validate eagerly: a tree that cannot be laid out should fail where it was
  -- written, not where it is drawn.
  if props.ink then props.ink = ink(props.ink, kind .. ".ink") end
  if props.style then props.style = style(props.style) end
  if props.gap then props.gap = props.gap; space(props.gap, kind .. ".gap") end
  if props.pad then props.pad = space(props.pad, kind .. ".pad") end
  if props.h and type(props.h) == "number" then space(props.h, kind .. ".h") end
  if props.radius then props.radius = radius(props.radius, kind .. ".radius") end
  return props
end

function ui.column(props) return node("column", props or {}) end
function ui.row(props) return node("row", props or {}) end
function ui.card(props) return node("card", props or {}) end
function ui.stack(props) return node("stack", props or {}) end

-- A ring of slices.
--
-- `slices` is a list of `{ id, label, available, body }`. The ring has no
-- polygon primitive to draw with, so each slice is a short run of rotated rects
-- laid along its arc: Canvas takes a real quaternion per node, so this is
-- geometry rather than a picture of geometry, and a slice can be hovered and
-- pressed like any other control.
--
-- `diameter` is in pixels. `inner` is the fraction of the radius that is hole
-- (0.46 is a ring; 0 is a pie). `selected` is a 1-based index. `segments` trades
-- nodes for smoothness; every slice costs `segments + 1` nodes plus one hit
-- region. A scene defaults to 8192 nodes and refuses past 65536, so a radial
-- menu is nowhere near the ceiling and a dense grid is comfortable.
function ui.radial(slices, props)
  props = props or {}
  props.slices = slices or {}
  return node("radial", props)
end

function ui.text(value, props)
  props = props or {}
  props.value = value
  return node("text", props)
end

function ui.rule(props) return node("rule", props or {}) end

-- Glyphs, not an icon font. The host has no icon primitive and Canvas has no
-- symbol set, so the library draws well-supported Unicode and keeps the names
-- stable: when the renderer grows real icons, only this table changes.
ui.glyph = {
  search = "⌕", share = "↑", more = "⋯", close = "✕",
  bold = "B", italic = "I", code = "</>", link = "∞",
  doc = "▤", files = "▣", grid = "▦", list = "☰", columns = "◫",
  chevron = "▸", check = "✓", dot = "●", eye = "◉",
}

function ui.icon(name, props)
  props = props or {}
  props.glyph = ui.glyph[name] or name
  return node("icon", props)
end

function ui.icon_button(name, props)
  props = props or {}
  props.glyph = ui.glyph[name] or name
  return node("icon_button", props)
end

-- A pressable-looking control. It draws a control; the host decides whether a
-- press reaches it. Until input is routed to Lua modules, this is an affordance,
-- not a behaviour.
function ui.button(label, props)
  props = props or {}
  props.label = label
  return node("button", props)
end

-- A keyboard hint. Same drawing as a button, quieter and monospaced in spirit.
function ui.kbd(label) return node("button", { label = label, small = true, quiet = true }) end

-- A horizontal band with a background: title bars, toolbars, status lines.
function ui.bar(props) return node("bar", props or {}) end

-- A raw rectangle in token colours. The escape hatch for dividers and
-- highlights; it takes units, not pixels, so it still follows the surface.
function ui.fill(props) return node("fill", props or {}) end
function ui.spacer(props) return node("spacer", props or {}) end
function ui.code(language, source) return node("code", { language = language, source = source }) end
function ui.diagram(format, source) return node("diagram", { format = format, source = source }) end

function ui.bullets(items, props)
  props = props or {}
  props.items = items
  return node("bullets", props)
end

function ui.quote(value) return node("quote", { value = value }) end

-- Dashboard primitives. These are ordinary tree nodes: the one-pass layout
-- owns their geometry, the token vocabulary owns their colour and spacing, and
-- ui.render emits the same canvas.rect/text primitives as every other node.
--
-- `stat`: label, value, unit, decimals, delta, delta_ink, status, action.
-- `gauge`: title, value, min, max, unit, decimals, segments, thresholds, status.
-- `table`: title, subtitle, status, columns, rows, row_h, row_action, row_key.
-- `chart`: title, subtitle, status, series, x/y bounds, threshold, selection,
--          columns, decimate, max_points, caps, legend_gap, legend_action,
--          plot_action.
--          All action fields are optional and use host-owned hit regions; the
--          view never receives a callback.
function ui.stat(props) return node("stat", props or {}) end
function ui.gauge(props) return node("gauge", props or {}) end
function ui.table(props) return node("table", props or {}) end
function ui.chart(series, props)
  props = props or {}
  props.series = series or props.series or {}
  return node("chart", props)
end

-- A graph of nodes and edges. Positions are data: every node carries `x`/`y` in
-- 0..1, and the layout engine (site rings, force, Barnes-Hut) stays outside the
-- library. A node without a position falls back to a ring, which is a place to
-- start rather than a second layout engine — the same rule `fungos.graph`
-- follows, so one snapshot draws the same shape on every host.
--
-- `resource` places a host-owned GPU surface over the plot instead of marks: the
-- native GraphGPU producer draws the same nodes and edges, and the view never
-- sees a pixel. Without it the node draws rotated rects for edges and marks for
-- nodes, which is the portable fallback every host can render.
function ui.graph(nodes, edges, props)
  props = props or {}
  props.nodes = nodes or props.nodes or {}
  props.edges = edges or props.edges or {}
  return node("graph", props)
end

-- A host-owned surface placed in the Lua layout. `resource` names a manifest
-- grant; the host resolves it to a scene, texture, or device surface. The view
-- never sees a texture handle or pixels, and the library never draws over it.
function ui.host_surface(resource, props)
  if type(resource) ~= "string" or resource == "" then
    fail("ui.host_surface needs a non-empty resource alias", 2)
  end
  props = props or {}
  props.resource = resource
  return node("host_surface", props)
end

-- An interaction intent. The Spore receives it; the view never runs it.
function ui.action(name, payload)
  return { action = name, payload = payload }
end

-- The host speaks a narrower language than this kit's nodes: `action` is a
-- *name*, the payload rides in `body`, and appearance is the host's vocabulary,
-- not ours. `ui.action` carries both halves in one table, so a control prim
-- unwraps it here rather than at each of the four sites that draw one.
local function control_prim(spec)
  local action, body = spec.action, spec.body
  if type(action) == "table" then
    action, body = action.action, action.payload or body
  end
  return {
    button = {
      id = spec.id,
      label = spec.label,
      action = action,
      body = body or {},
      enabled = spec.enabled ~= false,
      appearance = spec.appearance or "hit",
    },
    translation = spec.translation or { 0, 0, 0 },
    scale = spec.scale or { 1, 1, 1 },
  }
end

-- ---- text ---------------------------------------------------------------

-- Byte length is not character count: a glyph is one character and three bytes,
-- and a wrap budget that counts bytes breaks every non-ASCII label.
local function ulen(value)
  if utf8 and utf8.len then
    local count = utf8.len(value)
    if count then return count end
  end
  return #value
end

local function wrap(text, chars)
  local lines, line = {}, ""
  for word in text:gmatch("%S+") do
    if #line == 0 then
      line = word
    elseif ulen(line) + 1 + ulen(word) <= chars then
      line = line .. " " .. word
    else
      lines[#lines + 1] = line
      line = word
    end
  end
  if #line > 0 then lines[#lines + 1] = line end
  return lines
end

-- Advance estimate, not metrics: enough to sequence a row and right-align a
-- status label, never used to decide where a line breaks.
local function estimate_width(value, size, S)
  return ulen(value) * size * 0.62 * S.ux
end

-- Cut a label to the width its column gave it. A cell that draws its full text
-- into the next column is not a cell; the whole value stays in the row and in
-- the action body, so only the pixels are lost.
local function fit_text(value, size, S, width)
  local text = tostring(value or "")
  if width <= 0 or estimate_width(text, size, S) <= width then return text end
  for count = ulen(text) - 1, 1, -1 do
    local head = text
    if utf8 and utf8.offset then
      local offset = utf8.offset(text, count)
      if offset then head = string.sub(text, 1, offset - 1) end
    else
      head = string.sub(text, 1, count)
    end
    local candidate = head .. "…"
    if estimate_width(candidate, size, S) <= width then return candidate end
  end
  return "…"
end

-- A wrap budget comes from the column in *pixels*, not from a global constant:
-- the same view on a wider surface fits more characters.
local function budget(S, size, width)
  local pixels = width / S.ux
  return math.max(8, math.floor(pixels / (size * 0.52)))
end

-- ---- primitives ---------------------------------------------------------

local function text_prim(value, size, S, color, x, y)
  -- Canvas sizes text in pixels and multiplies by the node's depth scale, which
  -- is 1.25 at the z a flat label sits at. Ask for 1/1.25 of the target so the
  -- rendered glyph is the size the layout reserved.
  local scale = math.sqrt(size / (S.theme.body * S.depth))
  return { text = value, ink = INK_ALIAS[color] or color,
    translation = { x, y, 0 }, scale = { scale, scale, 1 } }
end

local function rect_prim(color, opacity, r, x, y, w, h)
  -- The alias table exists so a component can name `surface_sunken`; normalizing
  -- here is what makes that true for the primitives the library emits itself
  -- (`card`, `table` zebra rows, dividers) and not only for callers who happen
  -- to go through `ink()`.
  return { ink = INK_ALIAS[color] or color, opacity = opacity, radius = r or 0,
    translation = { x, y, -0.5 }, scale = { w, h, 1 } }
end

local function append(out, prims)
  for _, prim in ipairs(prims) do out[#out + 1] = prim end
end

-- Translate a child's primitives into the parent's frame.
local function place(prims, dx, dy)
  for _, prim in ipairs(prims) do
    prim.translation[1] = prim.translation[1] + dx
    prim.translation[2] = prim.translation[2] + dy
  end
  return prims
end

-- A rotation about Z, as the quaternion Canvas takes (xyzw). Only Z is offered:
-- a scene node tilted out of the surface plane is a 3D transform this library
-- has no vocabulary for, and silently accepting a table would hide that.
local function quat_z(angle)
  local half = angle * 0.5
  return { 0, 0, math.sin(half), math.cos(half) }
end

-- ---- layout -------------------------------------------------------------
--
-- One pass, bottom-up. `lay` returns `{ h, p }`: the height this node consumes
-- and the primitives it draws, both in the node's own frame (origin at its
-- top-left, content flowing toward -y). A parent places children by shifting
-- those primitives, so nothing is ever measured twice.

local lay

-- Width in scene units for a pixel count, and height likewise. Two functions,
-- not one: the host's 8-unit square is stretched to the surface, so a theme at
-- 1200x1920 has different horizontal and vertical density.
local function wx(S, n) return n * S.ux end
local function hy(S, n) return n * S.uy end

local function text_lines(n, S, width)
  local size = n.px or size_for(S.theme, n.style)
  return wrap(n.value, n.chars or budget(S, size, width))
end

-- Intrinsic width of a leaf, in units. Used to sequence a row without metrics.
local function intrinsic(n, S)
  local theme = S.theme
  if n.kind == "button" then
    -- The same formula `lay` uses, not a second opinion: two width estimates for
    -- one control is how a row ends up overlapping its neighbour.
    local size = n.small and theme.small or theme.body
    -- `size * S.ux` is `wx(size)`: theme sizes are pixels, and a scene length
    -- is a fraction, so the two forms are the same number. Kept spelled out
    -- beside the per-character advance so the whole estimate reads in one unit.
    return ulen(n.label) * size * 0.95 * S.ux + size * 0.52 * S.ux * 2
  elseif n.kind == "text" then
    return estimate_width(n.value, n.px or size_for(theme, n.style), S)
  elseif n.kind == "icon" then
    return (n.size and theme[n.size] or theme.body) * S.ux
  elseif n.kind == "icon_button" then
    local size = n.size and theme[n.size] or theme.body
    return size * 1.9 * S.ux
  end
  return 0
end

-- Interactive controls are host-owned: the module declares *what* the control
-- is and which action it means, and Canvas owns the pixels, the hit region, the
-- hover state and the press. A view that drew its own pill could not be pressed.
local function control_id(n)
  if n.id then return n.id end
  local action = tostring(n.action)
  return (action:gsub("[^%w%-_]", "-")):sub(1, 48)
end

-- A hit region is scene data, not a second renderer. Composite dashboard nodes
-- use the same host-owned control path as `button`: draw the card, then put an
-- invisible control over it so a Spore receives an action instead of a click
-- callback. Keep this optional so headless previews and non-Canvas hosts still
-- render the same tree.
local function hit_region(prims, spec)
  if not (spec and spec.action and widget and widget.button) then return end
  local w = tonumber(spec.w) or 0
  local h = tonumber(spec.h) or 0
  if w <= 0 or h <= 0 then return end
  prims[#prims + 1] = control_prim {
    id = spec.id,
    label = spec.label,
    action = spec.action,
    body = spec.body,
    enabled = spec.enabled,
    translation = { spec.x or 0, spec.y or 0, 0 },
    scale = { w, h, 1 },
  }
end

local function lay_text(n, S, width)
  local name = style(n.style)
  local size = n.px or S.theme[STYLE[name]]
  local lines = text_lines(n, S, width)
  -- Theme sizes are logical pixels: Canvas hands `metrics.micro` in pixels and
  -- every theme file here carries pixels. A scene position is a fraction of the
  -- 8-unit square, so a pixel length goes through `wx`/`hy` exactly once — and
  -- this is that once. Advancing by the bare pixel count put the first title 42
  -- scene units below the top edge instead of 42 pixels.
  local advance = hy(S, size * ui.leading)
  local color = n.ink or (name == "title" and "primary" or "text")
  local prims = {}
  for index, line in ipairs(lines) do
    prims[index] = text_prim(line, size, S, color, 0, -(index - 1) * advance)
  end
  return { h = #lines * advance, p = prims }
end

local function lay_children(n, S, width)
  local heights, parts, used = {}, {}, 0
  for index, child in ipairs(n.children) do
    parts[index] = lay(child, S, width)
    heights[index] = parts[index].h
    used = used + parts[index].h
  end
  return heights, parts, used
end

-- A ring of slices, laid out in one pass like anything else.
--
-- One radius for both axes, deliberately: a rotated rect only stays a rect in a
-- square frame, so sizing the ring by the smaller axis keeps it circular in
-- pixels instead of letting the surface's aspect ratio shear every slice.
local function lay_radial(n, S)
  local theme = S.theme
  local slices = n.slices or {}
  local count = #slices
  local diameter = n.diameter or theme.body * 46
  local radius = math.min(wx(S, diameter * 0.5), hy(S, diameter * 0.5))
  local inner = math.max(0, math.min(0.85, n.inner or 0.46))
  local band = radius * (1 - inner)
  local middle_radius = radius * (1 + inner) * 0.5
  local height = radius * 2 + band
  local cx, cy = radius, -(radius + band * 0.5)
  if count == 0 then
    return { h = height, p = {} }
  end

  local step = (n.sweep or (math.pi * 2)) / count
  local segments = math.max(1, math.min(12, n.segments or 6))
  local sub = step / segments
  -- Slight overlap so neighbouring segments meet at the seam rather than
  -- leaving a hairline the eye reads as a gap between slices.
  local segment_length = middle_radius * sub * 1.35
  local prims = {}

  for index, slice in ipairs(slices) do
    local start = -math.pi / 2 + (index - 1) * step
    local middle = start + step * 0.5
    local available = slice.available ~= false
    local selected = n.selected == index
    local ink = selected and "primary" or "surface"
    local opacity
    if not available then
      opacity = 0.18
    else
      opacity = selected and 0.95 or 0.72
    end

    for segment = 1, segments do
      local angle = start + (segment - 0.5) * sub
      prims[#prims + 1] = {
        ink = ink, opacity = opacity, radius = 0,
        translation = {
          cx + math.cos(angle) * middle_radius,
          cy + math.sin(angle) * middle_radius,
          -0.5,
        },
        scale = { segment_length, band, 1 },
        rotation = quat_z(angle + math.pi / 2),
      }
    end

    local size = selected and theme.small or theme.micro
    local label = tostring(slice.label or "")
    local label_ink = "text"
    if not available then
      label_ink = "muted"
    elseif selected then
      label_ink = "background"
    end
    prims[#prims + 1] = text_prim(
      label, size, S, label_ink,
      cx + math.cos(middle) * middle_radius - estimate_width(label, size, S) * 0.5
        + wx(S, size * 0.35),
      cy + math.sin(middle) * middle_radius - hy(S, size * 0.35)
    )

    if n.action and widget and widget.button then
      prims[#prims + 1] = control_prim {
        id = slice.id,
        label = label,
        action = n.action,
        body = slice.body or { id = slice.id, value = slice.id },
        enabled = available,
        translation = {
          cx + math.cos(middle) * middle_radius - segment_length * 0.5,
          cy + math.sin(middle) * middle_radius + band * 0.5,
          0,
        },
        scale = { segment_length, band, 1 },
      }
    end
  end

  return { h = height, p = prims }
end

-- ---- dashboard nodes ----------------------------------------------------

local function status_ink(value, fallback)
  local name = tostring(value or ""):lower()
  if name == "ok" or name == "healthy" or name == "live" or name == "ready"
    or name == "success" or name == "active" then
    return ink("success", "status.ink")
  elseif name == "warn" or name == "warning" or name == "degraded"
    or name == "pending" or name == "lagging" then
    return ink("warning", "status.ink")
  elseif name == "error" or name == "critical" or name == "crit" or name == "down"
    or name == "failed" or name == "failing" then
    return ink("danger", "status.ink")
  end
  return ink(fallback or "muted", "status.ink")
end

local function threshold_ink(value, steps, base)
  local name = base or "success"
  for _, step in ipairs(steps or {}) do
    if value >= (tonumber(step.value) or 0) then name = step.ink or name end
  end
  return ink(name, "threshold.ink")
end

local function value_format(value, decimals, unit)
  local out
  if type(value) == "number" then
    out = string.format("%." .. tostring(math.max(0, math.floor(decimals or 0))) .. "f", value)
  else
    out = tostring(value or "")
  end
  if unit and unit ~= "" then out = out .. tostring(unit) end
  return out
end

local function point_xy(point)
  if type(point) ~= "table" then return nil end
  local x, y = point.x or point[1], point.y or point[2]
  x, y = tonumber(x), tonumber(y)
  if x == nil or y == nil then return nil end
  return x, y
end

local function sample_points(points, count)
  local out = {}
  if #(points or {}) == 0 then return out end
  for index = 1, count do
    local at = 1
    if count > 1 and #points > 1 then
      at = 1 + math.floor((index - 1) * (#points - 1) / (count - 1) + 0.5)
    end
    local x, y = point_xy(points[at])
    if x ~= nil then out[#out + 1] = { x = x, y = y } end
  end
  return out
end

--- Largest-Triangle-Three-Buckets downsampling for dense series.
--
-- The chart keeps the endpoints and the visually dominant point in each bucket,
-- so a downsampled incident still shows its spike instead of losing it between
-- uniform samples. The returned points are the caller's own point objects.
--
-- @derives-from github:fungOS-linux/fungos-sdk@c104db3 -- LTTB ported from lua/fungos/chart.lua
function ui.lttb(points, threshold)
  points = points or {}
  local count = #points
  threshold = math.floor(tonumber(threshold) or count)
  if count < 3 or threshold < 3 or threshold >= count then return points end
  local data = {}
  for index, point in ipairs(points) do
    local x, y = point_xy(point)
    if x == nil then return points end
    data[index] = { x = x, y = y, source = point }
  end
  local sampled = { data[1] }
  local every = (count - 2) / (threshold - 2)
  local anchor = 1
  for bucket = 1, threshold - 2 do
    local range_start = math.floor((bucket - 1) * every) + 2
    local range_end = math.min(math.floor(bucket * every) + 2, count - 1)
    local avg_start = math.floor(bucket * every) + 2
    local avg_end = math.min(math.floor((bucket + 1) * every) + 2, count)
    local avg_x, avg_y, avg_n = 0, 0, 0
    for index = avg_start, avg_end do
      avg_x = avg_x + data[index].x
      avg_y = avg_y + data[index].y
      avg_n = avg_n + 1
    end
    if avg_n > 0 then avg_x, avg_y = avg_x / avg_n, avg_y / avg_n end
    local ax, ay = data[anchor].x, data[anchor].y
    local best_area, best_index = -1, range_start
    for index = range_start, range_end do
      local area = math.abs((ax - avg_x) * (data[index].y - ay)
        - (ax - data[index].x) * (avg_y - ay)) * 0.5
      if area > best_area then best_area, best_index = area, index end
    end
    sampled[#sampled + 1] = data[best_index]
    anchor = best_index
  end
  sampled[#sampled + 1] = data[count]
  local out = {}
  for index, point in ipairs(sampled) do out[index] = point.source end
  return out
end

--- Keep the newest `count` points without mutating the snapshot. Streaming is
-- a Spore concern; this is the pure windowing helper a live view calls first.
function ui.chart_window(points, count)
  points = points or {}
  count = math.max(1, math.floor(tonumber(count) or 240))
  local out = {}
  for index = math.max(1, #points - count + 1), #points do
    out[#out + 1] = points[index]
  end
  return out
end

local function lay_stat(n, S, width)
  local theme = S.theme
  local height = hy(S, n.h or 136)
  local pad = wx(S, space(n.pad or "md", "stat.pad"))
  local prims = {
    rect_prim(n.ink or "surface", 0.9, radius(n.radius or "md", "stat.radius"),
      -wx(S, 4), 0, width + wx(S, 8), height),
  }
  local label = tostring(n.label or "Metric")
  prims[#prims + 1] = text_prim(label, theme.small, S, "muted", pad, -hy(S, 12))
  if n.status and tostring(n.status) ~= "" then
    local status = tostring(n.status)
    local status_w = estimate_width(status, theme.micro, S)
    prims[#prims + 1] = text_prim(status, theme.micro, S,
      status_ink(n.status, n.status_ink), width - pad - status_w, -hy(S, 12))
  end
  local value = value_format(n.value, n.decimals)
  prims[#prims + 1] = text_prim(value, theme.title, S,
    ink(n.value_ink or "text", "stat.value_ink"), pad, -hy(S, 30))
  if n.unit and tostring(n.unit) ~= "" then
    prims[#prims + 1] = text_prim(tostring(n.unit), theme.micro, S, "muted",
      pad, -height + hy(S, 38))
  end
  if n.delta and tostring(n.delta) ~= "" then
    local delta_ink = n.delta_ink or ((tonumber(n.delta) or 0) >= 0 and "success" or "danger")
    prims[#prims + 1] = text_prim(tostring(n.delta), theme.micro, S,
      ink(delta_ink, "stat.delta_ink"), pad, -height + hy(S, 18))
  end
  hit_region(prims, {
    id = control_id(n), action = n.action, body = n.body, label = label,
    x = -wx(S, 4), y = 0, w = width + wx(S, 8), h = height,
    enabled = n.enabled,
  })
  return { h = height, p = prims }
end

local function lay_gauge(n, S, width)
  local theme = S.theme
  local height = hy(S, n.h or 184)
  local pad = wx(S, space(n.pad or "md", "gauge.pad"))
  local value = tonumber(n.value) or 0
  local lo = tonumber(n.min) or 0
  local hi = tonumber(n.max) or 100
  if hi == lo then hi = lo + 1 end
  local prims = {
    rect_prim(n.ink or "surface", 0.9, radius(n.radius or "md", "gauge.radius"),
      -wx(S, 4), 0, width + wx(S, 8), height),
  }
  if n.title and tostring(n.title) ~= "" then
    prims[#prims + 1] = text_prim(tostring(n.title), theme.small, S, "text", pad, -hy(S, 12))
  end
  if n.status and tostring(n.status) ~= "" then
    local status = tostring(n.status)
    local status_w = estimate_width(status, theme.micro, S)
    prims[#prims + 1] = text_prim(status, theme.micro, S,
      status_ink(n.status, n.status_ink), width - pad - status_w, -hy(S, 12))
  end
  local track_y = -height + hy(S, 30)
  local track_x = pad
  local track_w = width - pad * 2
  local track_h = hy(S, 10)
  local track_ink = threshold_ink(value, n.thresholds)
  prims[#prims + 1] = text_prim(value_format(value, n.decimals, n.unit), theme.large, S,
    track_ink, pad, -hy(S, 34))
  prims[#prims + 1] = rect_prim(ink("surface_sunken", "gauge.track.ink"), 0.9, radius("sm", "gauge.track"),
    track_x, track_y, track_w, track_h)
  local segments = math.max(2, math.floor(tonumber(n.segments) or 12))
  local seg_w = track_w / segments
  for index = 1, segments do
    local mid = lo + (hi - lo) * (index - 0.5) / segments
    if mid <= value then
      prims[#prims + 1] = rect_prim(threshold_ink(mid, n.thresholds), 0.92,
        radius("sm", "gauge.segment"), track_x + (index - 1) * seg_w,
        track_y, math.max(wx(S, 1), seg_w - wx(S, 1)), track_h)
    end
  end
  prims[#prims + 1] = text_prim(value_format(lo, n.decimals, n.unit), theme.micro, S,
    "muted", track_x, track_y - hy(S, 14))
  local max_text = value_format(hi, n.decimals, n.unit)
  prims[#prims + 1] = text_prim(max_text, theme.micro, S, "muted",
    track_x + track_w - estimate_width(max_text, theme.micro, S), track_y - hy(S, 14))
  hit_region(prims, {
    id = control_id(n), action = n.action, body = n.body,
    label = n.title and tostring(n.title) or "gauge",
    x = -wx(S, 4), y = 0, w = width + wx(S, 8), h = height,
    enabled = n.enabled,
  })
  return { h = height, p = prims }
end

local function lay_table(n, S, width)
  local theme = S.theme
  local pad = wx(S, space(n.pad or "md", "table.pad"))
  local gap = wx(S, space(n.gap or "sm", "table.gap"))
  local rows = n.rows or {}
  local columns = n.columns or {}
  if #columns == 0 then
    return { h = hy(S, n.h or 80), p = {
      rect_prim(n.ink or "surface", 0.9, radius(n.radius or "md", "table.radius"),
        -wx(S, 4), 0, width + wx(S, 8), hy(S, n.h or 80)),
    } }
  end
  local row_px = space(n.row_h or 24, "table.row_h")
  local header = hy(S, 20)
  local heading = (n.title and hy(S, theme.small * 1.5) or 0)
    + (n.subtitle and hy(S, theme.micro * 1.5) or 0)
  local computed = hy(S, 16) * 2 + heading + header + #rows * hy(S, row_px) + hy(S, 10)
  local height = n.h and hy(S, n.h) or computed
  local prims = {
    rect_prim(n.ink or "surface", 0.9, radius(n.radius or "md", "table.radius"),
      -wx(S, 4), 0, width + wx(S, 8), height),
  }
  local y = -hy(S, 16)
  local header_y = y
  if n.title and tostring(n.title) ~= "" then
    prims[#prims + 1] = text_prim(tostring(n.title), theme.small, S, "text", pad, y)
    y = y - hy(S, theme.small * 1.5)
  end
  if n.subtitle and tostring(n.subtitle) ~= "" then
    prims[#prims + 1] = text_prim(tostring(n.subtitle), theme.micro, S, "muted", pad, y)
    y = y - hy(S, theme.micro * 1.5)
  end
  if n.status and tostring(n.status) ~= "" then
    local status = tostring(n.status)
    prims[#prims + 1] = text_prim(status, theme.micro, S,
      status_ink(n.status, n.status_ink),
      width - pad - estimate_width(status, theme.micro, S), header_y)
  end
  local total = 0
  for _, column in ipairs(columns) do total = total + (tonumber(column.width) or 1) end
  local usable = width - pad * 2 - gap * (#columns - 1)
  local xs, widths = {}, {}
  local cursor = pad
  for index, column in ipairs(columns) do
    xs[index] = cursor
    widths[index] = usable * (tonumber(column.width) or 1) / total
    cursor = cursor + widths[index] + gap
  end
  for index, column in ipairs(columns) do
    prims[#prims + 1] = text_prim(
      fit_text(tostring(column.label or column.key or ""), theme.micro, S, widths[index]),
      theme.micro, S, "muted", xs[index], y)
  end
  y = y - header
  prims[#prims + 1] = rect_prim(ink("divider", "table.divider.ink"), 0.35, 0,
    pad, y, width - pad * 2, hy(S, 1))
  y = y - hy(S, 6)
  local available = math.max(0, math.floor((height + y + hy(S, 14)) / hy(S, row_px)))
  for row_index = 1, math.min(#rows, available) do
    local row = rows[row_index]
    if n.zebra ~= false and row_index % 2 == 0 then
      prims[#prims + 1] = rect_prim(ink("surface_raised", "table.row.ink"), 0.28, radius("sm", "table.row"),
        pad - wx(S, 4), y, width - pad * 2 + wx(S, 8), hy(S, row_px))
    end
    for index, column in ipairs(columns) do
      local value = row[column.key]
      if column.kind == "status" then
        local status = fit_text(tostring(value or ""), theme.micro, S,
          math.max(0, widths[index] - wx(S, 12)))
        if status ~= "" then
          prims[#prims + 1] = rect_prim(status_ink(status, column.status_ink), 1,
            radius("pill", "table.status"), xs[index], y + hy(S, 5), wx(S, 6), hy(S, 6))
          prims[#prims + 1] = text_prim(status, theme.micro, S,
            status_ink(status, column.status_ink), xs[index] + wx(S, 10), y)
        end
      else
        local text = fit_text(tostring(value or ""), theme.micro, S,
          math.max(0, widths[index] - wx(S, 8)))
        if text ~= "" then
          local x = xs[index]
          if column.align == "right" then
            x = x + widths[index] - estimate_width(text, theme.micro, S) - wx(S, 8)
          end
          prims[#prims + 1] = text_prim(text, theme.micro, S,
            ink(column.ink or "text", "table.ink"), x, y)
        end
      end
    end
    if n.row_action then
      local key = n.row_key and row[n.row_key] or row_index
      local row_id = (n.id or "table") .. "-row-" .. row_index
      if key ~= nil then
        row_id = row_id .. "-" .. (tostring(key):gsub("[^%w%-_]", "-"):sub(1, 48))
      end
      hit_region(prims, {
        id = row_id, action = n.row_action, body = row,
        label = tostring(key or row_index), x = pad - wx(S, 4), y = y,
        w = width - pad * 2 + wx(S, 8), h = hy(S, row_px),
        enabled = row.enabled,
      })
    end
    y = y - hy(S, row_px)
  end
  return { h = height, p = prims }
end

local function lay_chart(n, S, width)
  local theme = S.theme
  local height = hy(S, n.h or 420)
  local pad = wx(S, space(n.pad or "md", "chart.pad"))
  local prims = {
    rect_prim(n.ink or "surface", 0.9, radius(n.radius or "md", "chart.radius"),
      -wx(S, 4), 0, width + wx(S, 8), height),
  }
  local y = -hy(S, 12)
  local header_y = y
  if n.title and tostring(n.title) ~= "" then
    prims[#prims + 1] = text_prim(tostring(n.title), theme.small, S, "text", pad, y)
    y = y - hy(S, theme.small * 1.5)
  end
  if n.subtitle and tostring(n.subtitle) ~= "" then
    prims[#prims + 1] = text_prim(tostring(n.subtitle), theme.micro, S, "muted", pad, y)
    y = y - hy(S, theme.micro * 1.5)
  end
  if n.status and tostring(n.status) ~= "" then
    local status = tostring(n.status)
    prims[#prims + 1] = text_prim(status, theme.micro, S,
      status_ink(n.status, n.status_ink),
      width - pad - estimate_width(status, theme.micro, S), header_y)
  end
  local left = pad + wx(S, 30)
  local right = width - pad
  local bottom = -height + hy(S, 52)
  local top = y - hy(S, 4)
  local plot_w = math.max(wx(S, 16), right - left)
  local plot_h = math.max(hy(S, 16), top - bottom)
  prims[#prims + 1] = rect_prim(ink("surface_sunken", "chart.plot.ink"), 0.8, radius("sm", "chart.plot"),
    left, top, plot_w, plot_h)
  local series = n.series or {}
  local x_min, x_max = tonumber(n.x_min), tonumber(n.x_max)
  local y_min, y_max = tonumber(n.y_min), tonumber(n.y_max)
  for _, item in ipairs(series) do
    for _, point in ipairs(item.points or {}) do
      local px, py = point_xy(point)
      if px ~= nil then
        x_min = x_min and math.min(x_min, px) or px
        x_max = x_max and math.max(x_max, px) or px
        y_min = y_min and math.min(y_min, py) or py
        y_max = y_max and math.max(y_max, py) or py
      end
    end
  end
  x_min, x_max = x_min or 0, x_max or 1
  y_min, y_max = y_min or 0, y_max or 1
  if x_min == x_max then x_max = x_min + 1 end
  if y_min == y_max then y_max = y_min + 1 end
  local function sx(value) return left + (value - x_min) / (x_max - x_min) * plot_w end
  local function sy(value) return bottom + (value - y_min) / (y_max - y_min) * plot_h end
  for index = 0, 3 do
    local gy = top - plot_h * index / 3
    prims[#prims + 1] = rect_prim("muted", 0.08, 0, left, gy, plot_w, hy(S, 1))
  end
  for index = 0, 2 do
    local value = y_min + (y_max - y_min) * index / 2
    local text = value_format(value, n.y_decimals, n.unit)
    prims[#prims + 1] = text_prim(text, theme.micro, S, "muted",
      left - estimate_width(text, theme.micro, S) - wx(S, 6), sy(value) - hy(S, 4))
  end
  for index = 0, 2 do
    local value = x_min + (x_max - x_min) * index / 2
    -- The bottom-left x tick and the bottom y tick share a corner. The y tick
    -- carries the scale and the x tick carries nothing extra at x_min, so keep
    -- the former and drop the latter instead of drawing two labels on top.
    if not (index == 0 and x_min == 0) then
      local text = value_format(value, n.x_decimals)
      prims[#prims + 1] = text_prim(text, theme.micro, S, "muted",
        sx(value) - estimate_width(text, theme.micro, S) / 2, bottom - hy(S, 14))
    end
  end
  if n.selection then
    local from = math.max(x_min, math.min(x_max, tonumber(n.selection.x0) or x_min))
    local to = math.max(x_min, math.min(x_max, tonumber(n.selection.x1) or x_max))
    if from > to then from, to = to, from end
    prims[#prims + 1] = rect_prim("primary", 0.12, 0, sx(from), top,
      math.max(wx(S, 1), sx(to) - sx(from)), plot_h)
  end
  if n.threshold then
    local value = tonumber(n.threshold.value)
    if value ~= nil then
      local ty = sy(math.max(y_min, math.min(y_max, value)))
      local threshold_ink_name = ink(n.threshold.ink or "warning", "chart.threshold.ink")
      prims[#prims + 1] = rect_prim(threshold_ink_name, 0.7, 0,
        left, ty, plot_w, hy(S, 1))
      if n.threshold.label and tostring(n.threshold.label) ~= "" then
        local text = tostring(n.threshold.label)
        prims[#prims + 1] = text_prim(text, theme.micro, S, threshold_ink_name,
          right - estimate_width(text, theme.micro, S), ty - hy(S, 10))
      end
    end
  end
  local columns = math.max(2, math.floor(tonumber(n.columns) or 12))
  local target = columns
  if n.max_points then
    target = math.min(target, math.max(3, math.floor(tonumber(n.max_points) or target)))
  end
  local col_w = plot_w / columns
  for series_index, item in ipairs(series) do
    local ink_name = ink(item.ink or "primary", "chart.series.ink")
    local points = item.points or {}
    if n.decimate ~= false and item.decimate ~= false and #points > target then
      points = ui.lttb(points, target)
    end
    for column, point in ipairs(sample_points(points, columns)) do
      local value = math.max(y_min, math.min(y_max, point.y))
      local py = sy(value)
      local bar_h = math.max(hy(S, 1), py - bottom)
      prims[#prims + 1] = rect_prim(ink_name, tonumber(item.opacity) or 0.26, 0,
        left + (column - 1) * col_w, py, math.max(wx(S, 1), col_w - wx(S, 1)), bar_h)
      if n.caps ~= false and item.caps ~= false then
        prims[#prims + 1] = rect_prim(ink_name, 1, radius("sm", "chart.cap"),
          left + (column - 1) * col_w, py, math.max(wx(S, 1), col_w - wx(S, 1)), hy(S, 2))
      end
    end
  end
  hit_region(prims, {
    id = (n.id or "chart") .. "-plot", action = n.plot_action,
    body = { x_min = x_min, x_max = x_max, selection = n.selection },
    label = n.title and tostring(n.title) or "chart",
    x = left, y = top, w = plot_w, h = plot_h, enabled = n.enabled,
  })
  local legend_x = pad
  for index, item in ipairs(series) do
    local label = tostring(item.label or ("series " .. index))
    local ink_name = ink(item.ink or "primary", "chart.legend.ink")
    prims[#prims + 1] = text_prim(label, theme.micro, S, ink_name, legend_x, bottom - hy(S, 34))
    local legend_w = estimate_width(label, theme.micro, S)
    hit_region(prims, {
      id = (n.id or "chart") .. "-legend-" .. index,
      action = n.legend_action,
      body = { series = item.id or index, label = label, ink = item.ink },
      label = label, x = legend_x, y = bottom - hy(S, 34),
      w = legend_w + wx(S, 8), h = hy(S, theme.micro * 1.8),
      enabled = item.enabled,
    })
    legend_x = legend_x + legend_w + wx(S, n.legend_gap or 22)
  end
  return { h = height, p = prims }
end

-- ---- graph --------------------------------------------------------------

-- Positions are data. A node without one is not an error: a snapshot that only
-- knows its topology still draws, and a ring is the only layout this library
-- will invent. Site rings, force and Barnes-Hut belong to the producer, which
-- knows what the graph means.
local function graph_positions(nodes)
  local placed, missing = {}, false
  for _, item in ipairs(nodes) do
    if tonumber(item.x) == nil or tonumber(item.y) == nil then missing = true end
  end
  local count = math.max(1, #nodes)
  for index, item in ipairs(nodes) do
    local x, y = tonumber(item.x), tonumber(item.y)
    if missing then
      local angle = -math.pi / 2 + (index - 1) * math.pi * 2 / count
      x, y = 0.5 + math.cos(angle) * 0.34, 0.5 + math.sin(angle) * 0.34
    end
    placed[tostring(item.id)] = { x = x, y = y }
  end
  return placed
end

-- A graph card. Edges are rotated rects, which is a line the host already knows
-- how to draw on every surface. A node is a dot (with an optional halo), so a
-- producer that wants one GPU primitive per colour has the ink it needs.
local function lay_graph(n, S, width)
  local theme = S.theme
  local nodes, edges = n.nodes or {}, n.edges or {}
  local height = hy(S, n.h or 420)
  local pad = wx(S, space(n.pad or "md", "graph.pad"))
  local prims = {
    rect_prim(n.ink or "surface", 0.9, radius(n.radius or "md", "graph.radius"),
      -wx(S, 4), 0, width + wx(S, 8), height),
  }
  local y = -hy(S, 12)
  local header_y = y
  if n.title and tostring(n.title) ~= "" then
    prims[#prims + 1] = text_prim(tostring(n.title), theme.small, S, "text", pad, y)
    y = y - hy(S, theme.small * 1.5)
  end
  if n.subtitle and tostring(n.subtitle) ~= "" then
    prims[#prims + 1] = text_prim(tostring(n.subtitle), theme.micro, S, "muted", pad, y)
    y = y - hy(S, theme.micro * 1.5)
  end
  if n.status and tostring(n.status) ~= "" then
    local status = tostring(n.status)
    prims[#prims + 1] = text_prim(status, theme.micro, S, status_ink(n.status, n.status_ink),
      width - pad - estimate_width(status, theme.micro, S), header_y)
  end
  local left, right = pad, width - pad
  local legend = n.legend or {}
  local bottom = -height + hy(S, (#legend > 0) and 44 or 28)
  local top = y - hy(S, 6)
  local plot_w = math.max(wx(S, 16), right - left)
  local plot_h = math.max(hy(S, 16), top - bottom)

  if n.resource and tostring(n.resource) ~= "" then
    -- The producer owns every pixel after this point, including the labels.
    prims[#prims + 1] = {
      host = { id = n.id or ("graph-" .. tostring(n.resource)), resource = n.resource },
      translation = { left, top, 0 }, scale = { plot_w, plot_h, 1 },
    }
    return { h = height, p = prims }
  end

  prims[#prims + 1] = rect_prim("surface_sunken", 0.55, radius("sm", "graph.plot"),
    left, top, plot_w, plot_h)
  local placed = graph_positions(nodes)
  local function px(x) return left + x * plot_w end
  local function py(graph_y) return bottom + graph_y * plot_h end
  -- Rows first: a layered graph is a table with dots, and the rail is the line
  -- that makes nine rows read as one shape rather than nine coincidences.
  for _, item in ipairs(nodes) do
    local point = placed[tostring(item.id)]
    if item.rail and point then
      prims[#prims + 1] = rect_prim("muted", 0.07, 0, left, py(point.y), plot_w, hy(S, 1))
    end
  end

  local edge_width = hy(S, n.edge_width or 1.4)
  for _, edge in ipairs(edges) do
    local a, b = placed[tostring(edge.source)], placed[tostring(edge.target)]
    if a and b then
      local x1, y1, x2, y2 = px(a.x), py(a.y), px(b.x), py(b.y)
      local dx, dy = x2 - x1, y2 - y1
      local length = math.sqrt(dx * dx + dy * dy)
      if length > 0.0001 then
        local stale = edge.state == "stale"
        prims[#prims + 1] = {
          ink = ink(edge.ink or n.edge_ink or "muted", "graph.edge.ink"),
          opacity = tonumber(edge.opacity) or (stale and 0.08 or n.edge_opacity or 0.24),
          radius = radius("pill", "graph.edge"),
          translation = { x1, y1, -0.6 },
          scale = { length, edge_width, 1 },
          -- `math.atan2` was folded into `math.atan(y, x)` in Lua 5.3; a
          -- host on 5.4 has no atan2 at all.
          rotation = quat_z(math.atan(dy, dx)),
        }
      end
    end
  end

  local size = tonumber(n.node_size) or 7
  local halo = tonumber(n.halo_size) or 2.6
  local sx, sy = wx(S, size), hy(S, size)
  local target_w, target_h = wx(S, 18), hy(S, 18)
  local label_size = theme[n.label_size or "micro"] or theme.micro
  for _, item in ipairs(nodes) do
    local point = placed[tostring(item.id)]
    if point then
      local x, y = px(point.x), py(point.y)
      local state = tostring(item.state or "live")
      local color = ink(item.ink or n.node_ink or "primary", "graph.node.ink")
      local opacity = tonumber(item.opacity) or (state == "stale" and 0.4 or 0.95)
      -- A halo is one extra node per dot, so it is opt-in: a graph of thirty
      -- peers pays for the glow on the ones that are live, not on all of them.
      if item.halo then
        prims[#prims + 1] = rect_prim(color, opacity * 0.16, radius("pill", "graph.halo"),
          x - sx * halo * 0.5, y + sy * halo * 0.5, sx * halo, sy * halo)
      end
      prims[#prims + 1] = rect_prim(color, opacity, radius("pill", "graph.node"),
        x - sx * 0.5, y + sy * 0.5, sx, sy)
      if n.labels ~= false and item.label and tostring(item.label) ~= "" then
        local label = tostring(item.label)
        local label_w = estimate_width(label, label_size, S)
        -- A layered graph puts hosts in a row, so their labels hang below the
        -- dot; a star graph puts them beside it. The producer knows which
        -- shape it drew, and one rule for both is how labels land on top of
        -- their neighbours.
        local position = item.label_position or (point.x > 0.5 and "left" or "right")
        local lx, ly
        if position == "below" then
          lx, ly = x - label_w * 0.5, y - hy(S, label_size * 1.7)
        elseif position == "left" then
          lx, ly = x - sx - label_w, y - hy(S, label_size * 0.45)
        else
          lx, ly = x + sx, y - hy(S, label_size * 0.45)
        end
        prims[#prims + 1] = text_prim(label, label_size, S,
          state == "stale" and "muted" or "text",
          math.max(left, math.min(right - label_w, lx)),
          math.max(bottom + hy(S, 2), ly))
      end
      if n.node_action and widget and widget.button then
        prims[#prims + 1] = control_prim {
          id = (n.id or "graph") .. ":" .. tostring(item.id),
          label = item.label and tostring(item.label) or tostring(item.id),
          action = n.node_action,
          body = item.body or { id = item.id, label = item.label, state = item.state },
          enabled = item.enabled ~= false,
          translation = { x - target_w * 0.5, y + target_h * 0.5, 0 },
          scale = { target_w, target_h, 1 },
        }
      end
    end
  end

  if #nodes == 0 then
    local text = tostring(n.empty or "no nodes")
    prims[#prims + 1] = text_prim(text, theme.micro, S, "muted",
      left + plot_w * 0.5 - estimate_width(text, theme.micro, S) * 0.5,
      bottom + plot_h * 0.5)
  end

  local legend_y = -height + hy(S, 18)
  local lx = pad
  for _, item in ipairs(legend) do
    local dot = wx(S, 6)
    local label = tostring(item.label or "")
    prims[#prims + 1] = rect_prim(ink(item.ink or "muted", "graph.legend.ink"), 0.9,
      radius("pill", "graph.legend"), lx, legend_y, dot, hy(S, 6))
    prims[#prims + 1] = text_prim(label, theme.micro, S, "muted",
      lx + dot + wx(S, 5), legend_y + hy(S, 1))
    lx = lx + dot + wx(S, 5) + estimate_width(label, theme.micro, S) + wx(S, 14)
  end
  local caption = n.caption or (("%d nodes · %d edges"):format(#nodes, #edges))
  prims[#prims + 1] = text_prim(caption, theme.micro, S, "muted",
    right - estimate_width(caption, theme.micro, S), legend_y + hy(S, 1))
  return { h = height, p = prims }
end

-- The invariant that would have caught the toolbar bug: a node's primitives must
-- stay inside the box it claimed. Checked only when `ui.debug` is set, because
-- the message costs bytes in every module that never trips it.
local lay_node

lay = function(n, S, width)
  local result = lay_node(n, S, width)
  if ui.debug then
    local top, bottom = 0, 0
    for _, prim in ipairs(result.p) do
      local y = prim.translation[2]
      -- A rect's scale is a length; a text's scale is a font multiplier, so its
      -- drawn height has to be recovered from the size it asked for.
      local extent
      if prim.opacity ~= nil or prim.button or prim.host then
        extent = prim.scale[2]
      else
        extent = hy(S, prim.scale[1] * prim.scale[1] * S.theme.body * S.depth)
      end
      top = math.max(top, y)
      bottom = math.min(bottom, y - extent)
    end
    local tolerance = hy(S, 1)
    if top > tolerance or bottom < -result.h - tolerance then
      fail(("%s(%q) claims h=%.3f but draws %.3f..%.3f (w=%.3f)"):format(
        tostring(n.kind), tostring(n.value or n.label or n.glyph or ""),
        result.h, bottom, top, width), 3)
    end
  end
  return result
end

lay_node = function(n, S, width)
  local kind = n.kind
  local theme = S.theme

  if kind == "text" then
    return lay_text(n, S, width)

  elseif kind == "radial" then
    return lay_radial(n, S)

  elseif kind == "rule" then
    return { h = hy(S, 10), p = { rect_prim("muted", 0.26, 0, 0, hy(S, 4), width, hy(S, 1)) } }

  elseif kind == "fill" then
    local h = n.h and hy(S, n.h) or 0
    return { h = h, p = { rect_prim(n.ink or "surface", n.opacity or 0.6,
                           radius(n.radius, "fill.radius"), 0, 0, n.w and wx(S, n.w) or width, h) } }

  elseif kind == "host_surface" then
    local h = n.h and hy(S, n.h) or hy(S, n.default_h or 240)
    local id = n.id or ("host-" .. tostring(n.resource))
    return { h = h, p = { {
      host = { id = id, resource = n.resource },
      translation = { 0, 0, 0 }, scale = { width, h, 1 },
    } } }

  elseif kind == "icon" then
    local size = n.size and theme[n.size] or theme.body
    return { h = hy(S, size * ui.leading),
             p = { text_prim(n.glyph, size, S, n.ink or "muted", 0, 0) } }

  elseif kind == "icon_button" then
    local size = n.size and theme[n.size] or theme.body
    local side = hy(S, size * 1.9)
    local wide = wx(S, size * 1.9)
    local color = n.quiet and "surface" or "primary"
    local opacity = n.quiet and 0.5 or (n.selected and 1.0 or 0.85)
    local prims = {
      rect_prim(color, opacity, radius("md", "icon_button"), 0, 0, wide, side),
      text_prim(n.glyph, size, S, n.quiet and "muted" or "background",
                wx(S, size * 0.45), -hy(S, size * 0.42)),
    }
    if n.action and widget and widget.button then
      prims[#prims + 1] = control_prim {
        id = control_id(n),
        label = n.glyph,
        action = n.action,
        body = n.body,
        enabled = n.enabled,
        translation = { 0, 0, 0 }, scale = { wide, side, 1 },
      }
    end
    return { h = side, p = prims }

  elseif kind == "button" then
    local size = n.small and theme.small or theme.body
    -- A pill is text plus padding: height follows the type size, and the label
    -- is centred in it. Width stays an estimate until the host exposes metrics.
    local text_h = hy(S, size)
    local vpad = hy(S, size * 0.45)
    local hpad = wx(S, size * 0.52)
    local height = text_h + vpad * 2
    local label_w = estimate_width(n.label, size, S) + hpad * 2
    local color = n.quiet and "surface" or "primary"
    local opacity = n.quiet and 0.55 or (n.selected and 1.0 or 0.9)
    local prims = {
      rect_prim(color, opacity, radius(n.radius or "md", "button"),
                0, 0, label_w, height),
      text_prim(n.label, size, S, n.quiet and "muted" or "background",
                hpad, -vpad),
    }
    -- Draw the control *and* register a press region on top of it. The host
    -- draws nothing for an invisible control, so the view keeps its own look
    -- while Canvas still owns the hit area and the press.
    if n.action and widget and widget.button then
      prims[#prims + 1] = control_prim {
        id = control_id(n),
        label = n.label,
        action = n.action,
        body = n.body,
        enabled = n.enabled,
        translation = { 0, 0, 0 }, scale = { label_w, height, 1 },
      }
    end
    return { h = height, p = prims }

  elseif kind == "bar" then
    local pad = space(n.pad or "sm", "bar.pad")
    local heights, parts, used = lay_children(n, S, width - wx(S, pad) * 2)
    local height = math.max(n.h and hy(S, n.h) or 0, used + hy(S, pad) * 2)
    local bleed = n.bleed and wx(S, n.bleed) or 0
    local prims = {
      rect_prim(n.ink or "surface", n.opacity or 0.9, radius(n.radius, "bar.radius"),
                -bleed, n.dy and hy(S, n.dy) or 0, width + bleed * 2, height),
    }
    if n.border then
      local top = n.border_top and -height or 0
      prims[#prims + 1] = rect_prim("muted", 0.22, 0, -bleed, top, width + bleed * 2, hy(S, 1))
    end
    -- Children start at the top padding, and the band grows to fit them, so a
    -- bar can never be shorter than its contents.
    local y = -hy(S, pad)
    for index, part in ipairs(parts) do
      append(prims, place(part.p, wx(S, pad), y))
      y = y - (heights[index] or 0)
    end
    return { h = height, p = prims }

  elseif kind == "spacer" then
    return { h = hy(S, space(n.size or "md", "spacer.size")), p = {} }

  elseif kind == "code" then
    local lines = {}
    for line in (n.source .. "\n"):gmatch("(.-)\n") do lines[#lines + 1] = line end
    if lines[#lines] == "" then table.remove(lines) end
    local advance = hy(S, theme.small * 1.8)
    local height = advance * (#lines + 1) + hy(S, 8)
    local bleed = wx(S, 4)
    local prims = {
      rect_prim("surface", 0.95, radius("md", "code"), -bleed, 0, width + bleed * 2, height),
    }
    if n.language and n.language ~= "" then
      prims[#prims + 1] = text_prim(n.language, theme.micro, S, "muted", width - wx(S, 40), -hy(S, 4))
    end
    for index, line in ipairs(lines) do
      prims[#prims + 1] = text_prim(line, theme.small, S, "muted", 0, -hy(S, 4) - (index - 1) * advance)
    end
    return { h = height, p = prims }

  elseif kind == "diagram" then
    local height = hy(S, theme.small * 1.8 + 24)
    local bleed = wx(S, 4)
    return { h = height + hy(S, ui.gap.xs), p = {
      rect_prim("surface", 0.55, radius("md", "diagram"), -bleed, 0, width + bleed * 2, height),
      text_prim("[" .. n.format .. "]  " .. n.source, theme.small, S, "muted", wx(S, 8), -hy(S, 8)),
    } }

  elseif kind == "bullets" then
    local advance = hy(S, theme.body * ui.leading)
    local indent = wx(S, 26)
    local prims, height = {}, 0
    for _, item in ipairs(n.items) do
      prims[#prims + 1] = text_prim("•", theme.body, S, "muted", wx(S, 8), -height)
      for _, line in ipairs(wrap(item, n.chars or budget(S, theme.body, width - indent))) do
        prims[#prims + 1] = text_prim(line, theme.body, S, "text", indent, -height)
        height = height + advance
      end
    end
    return { h = height, p = prims }

  elseif kind == "quote" then
    local advance = hy(S, theme.body * ui.leading)
    local indent = wx(S, 14)
    local lines = wrap(n.value, n.chars or budget(S, theme.body, width - indent))
    local height = #lines * advance
    local prims = { rect_prim("primary", 0.5, radius("sm", "quote"), 0, 0, wx(S, 2), height) }
    for index, line in ipairs(lines) do
      prims[#prims + 1] = text_prim(line, theme.body, S, "muted", indent, -(index - 1) * advance)
    end
    return { h = height, p = prims }

  elseif kind == "stat" then
    return lay_stat(n, S, width)

  elseif kind == "gauge" then
    return lay_gauge(n, S, width)

  elseif kind == "table" then
    return lay_table(n, S, width)

  elseif kind == "chart" then
    return lay_chart(n, S, width)

  elseif kind == "graph" then
    return lay_graph(n, S, width)

  elseif kind == "row" then
    -- Rows place aligned pairs, or weighted columns when `widths` is given.
    local spacing = wx(S, space(n.gap or "none", "row.gap"))
    local count = #n.children
    local prims, cursor, tallest = {}, 0, 0

    if n.widths then
      local total = width - spacing * (count - 1)
      for index, child in ipairs(n.children) do
        local child_width = total * n.widths[index]
        local part = lay(child, S, child_width)
        append(prims, place(part.p, cursor, 0))
        cursor = cursor + child_width + spacing
        if part.h > tallest then tallest = part.h end
      end
    else
      local own, used = {}, spacing * (count - 1)
      for index, child in ipairs(n.children) do
        own[index] = intrinsic(child, S)
        used = used + own[index]
      end
      cursor = n.align == "end" and (width - used - wx(S, 4)) or 0
      for index, child in ipairs(n.children) do
        local remaining = width - cursor
        local part = lay(child, S, math.max(own[index], remaining))
        append(prims, place(part.p, cursor, 0))
        cursor = cursor + own[index] + spacing
        if part.h > tallest then tallest = part.h end
      end
    end
    return { h = tallest, p = prims }

  elseif kind == "column" or kind == "card" or kind == "stack" then
    local spacing = hy(S, space(n.gap or "sm", kind .. ".gap"))
    local pad = n.pad or 0
    if kind == "card" then pad = pad > 0 and pad or ui.gap.sm end
    local padx, pady = wx(S, pad), hy(S, pad)
    local inner_width = width - padx * 2
    local heights, parts, used = lay_children(n, S, inner_width)
    used = used + spacing * (#n.children - 1)
    local content = used

    -- `grow` children absorb whatever the parent's fixed height leaves over, so
    -- a footer stays pinned to the bottom instead of floating after the text.
    local extra = 0
    local growers = 0
    for _, child in ipairs(n.children) do
      if child.grow then growers = growers + 1 end
    end
    if n.h and growers > 0 then
      extra = math.max(0, hy(S, n.h) - pady * 2 - content) / growers
    end

    local height = math.max(n.h and hy(S, n.h) or 0, content + pady * 2)
    local prims = {}
    if kind == "card" then
      prims[1] = rect_prim("surface", 0.9, radius("md", "card"), -wx(S, 4), 0, width + wx(S, 8), height)
    end
    local y = -pady
    for index, child in ipairs(n.children) do
      if index > 1 then y = y - spacing end
      append(prims, place(parts[index].p, padx, y))
      y = y - (heights[index] or 0) - (child.grow and extra or 0)
    end
    return { h = height, p = prims }
  end

  -- Unknown kinds fail loudly: a view that draws nothing is a bug, not a style.
  fail(("unknown node kind %q"):format(tostring(kind)))
end

-- ---- render -------------------------------------------------------------

-- A Spore that is handed a surface size directly (a document pane, a test) can
-- set it here instead of declaring host inputs it does not own.
function ui.viewport(width, height)
  ui.viewport_width = tonumber(width)
  ui.viewport_height = tonumber(height)
end

-- One pixel horizontally and one vertically, for the surface actually on
-- screen. There is no single "unit": Canvas stretches its 8-unit square to the
-- surface, so horizontal and vertical density differ on every window that is
-- not square, and a text size is a pixel count while a position is a fraction.
--
-- The size comes from `module.inputs`, which a widget module declares. A
-- document Spore does not, so it may pass `ui.viewport(w, h)` itself; absent
-- both, the layout assumes a portrait document surface and says so here rather
-- than silently shrinking with the window.
local function context(theme)
  local inputs = module and module.inputs
  local w = math.max(ui.viewport_width or (inputs and inputs.width) or 1200, 1)
  local h = math.max(ui.viewport_height or (inputs and inputs.height) or 1800, 1)
  return {
    theme = theme,
    ux = 8 / w,
    uy = 8 / h,
    -- Canvas multiplies text by scene_node_scale(z); a flat label sits at 1.25.
    depth = 1.25,
  }
end

local function layout_prims(tree, theme)
  local S = context(theme)
  local body = lay(tree, S, 8.0)
  local prims = place(body.p, -4.0, 4.0)
  -- Opaque base: a surface is a window, not a hole in the wallpaper.
  table.insert(prims, 1, {
    ink = "background", opacity = 1, radius = 0,
    translation = { -4, 4, -1 }, scale = { 8, 8, 1 },
  })
  return prims
end

-- Controls first: the host owns their pixels, so there is nothing to draw
-- under them, and their hit region has to be registered either way.
local function emit_controls(prims)
  for _, prim in ipairs(prims) do
    if prim.button then
      if not (widget and widget.button) then
        fail("a button needs widget.button from the host", 3)
      end
      local spec = prim.button
      widget.button({
        id = spec.id,
        label = spec.label,
        action = spec.action,
        body = spec.body or {},
        x = prim.translation[1], y = prim.translation[2],
        width = prim.scale[1], height = prim.scale[2],
        enabled = spec.enabled,
        appearance = spec.appearance,
      })
    elseif prim.host then
      if not (canvas and canvas.host_resource) then
        fail("host_surface needs canvas.host_resource; this host is too old", 3)
      end
      local spec = prim.host
      canvas.host_resource({
        id = spec.id,
        resource = spec.resource,
        translation = prim.translation,
        scale = prim.scale,
      })
    end
  end
end

local function emit_canvas(prims)
  local index = 0
  for _, prim in ipairs(prims) do
    index = index + 1
    local id = prim.id or string.format("e%d", index)
    if prim.button or prim.host then
      -- already registered above
    elseif prim.opacity ~= nil then
      canvas.rect({ id = id, ink = prim.ink, opacity = prim.opacity, radius = prim.radius,
                    translation = prim.translation, scale = prim.scale,
                    rotation = prim.rotation })
    else
      canvas.text({ id = id, text = prim.text, ink = prim.ink,
                    translation = prim.translation, scale = prim.scale })
    end
  end
end

-- Canvas stores rotations as quaternions; soil-scene stores Euler XYZ. The
-- view vocabulary only has a Z rotation, so a non-Z quaternion is a bug in the
-- caller rather than something to guess at.
local function euler(rotation)
  if rotation == nil then return { 0, 0, 0 } end
  if type(rotation) ~= "table" or #rotation ~= 4 then
    fail("rotation must be a quaternion {x,y,z,w}", 3)
  end
  local x, y, z, w = rotation[1], rotation[2], rotation[3], rotation[4]
  local length = math.sqrt(x * x + y * y + z * z + w * w)
  if length == 0 or length ~= length then
    fail("rotation quaternion is not finite", 3)
  end
  x, y, z, w = x / length, y / length, z / length, w / length
  if math.abs(x) > 1e-6 or math.abs(y) > 1e-6 then
    fail("soil documents carry Z rotation only", 3)
  end
  return { 0, 0, 2 * math.atan(z, w) }
end

-- The Lua peer of soil_scene::Present. A node is flat: the one-pass layout has
-- already placed every primitive in the surface's 8-unit space, so there is no
-- second layout tree for a host to interpret. Buttons and host surfaces are
-- host-owned controls, not scene content; they remain in emit_controls.
local function document_from(prims)
  local nodes = {}
  local index = 0
  for _, prim in ipairs(prims) do
    index = index + 1
    if not prim.button and not prim.host then
      local node = {
        id = prim.id or string.format("e%d", index),
        transform = {
          translation = prim.translation,
          rotation = euler(prim.rotation),
          scale = prim.scale,
        },
      }
      if prim.opacity ~= nil then
        node.type = "rect"
        node.ink = prim.ink
        node.opacity = prim.opacity
        node.radius = prim.radius or 0
      elseif prim.text ~= nil then
        node.type = "text"
        node.text = prim.text
        node.ink = prim.ink
        node.opacity = 1
      else
        fail("unknown UI primitive in soil document", 3)
      end
      nodes[#nodes + 1] = node
    end
  end
  return { kind = "scene", schema = 1, nodes = nodes }
end

-- Legacy Canvas primitive path. Kept as the compatibility backend and as the
-- fallback for hosts that do not grant the soil document capability yet.
function ui.render(tree, theme)
  local prims = layout_prims(tree, theme)
  emit_controls(prims)
  emit_canvas(prims)
end

-- The pure document. Same `soil_scene::Present` shape the Rust SDK sends:
-- {"kind":"scene","schema":1,"nodes":[...]}. No host calls, so a test or a
-- non-Canvas host can inspect or forward it.
function ui.document(tree, theme)
  return document_from(layout_prims(tree, theme))
end

-- Present a document this library did not build. This is the Lua peer of
-- `soil_sdk_canvas::Surface::present`.
function ui.present_document(document)
  local present = canvas and canvas.present
  if present == nil and soil and soil.canvas then
    present = soil.canvas.present
  end
  if not present then
    fail("this host has no soil document capability (canvas.present)", 2)
  end
  return present(document)
end

-- Present through the granted host capability. `canvas.present` is Canvas's
-- `soil:canvas` binding; a Soil Lua host can provide the same function under
-- `soil.canvas.present`. If neither exists, use the legacy primitive path so a
-- view keeps rendering on an older Canvas instead of going blank.
function ui.present(tree, theme)
  local prims = layout_prims(tree, theme)
  emit_controls(prims)
  local document = document_from(prims)
  if (canvas and canvas.present) or (soil and soil.canvas and soil.canvas.present) then
    return ui.present_document(document)
  end
  emit_canvas(prims)
end

return ui
