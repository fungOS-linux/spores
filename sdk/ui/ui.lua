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
  return value
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

-- An interaction intent. The Spore receives it; the view never runs it.
function ui.action(name, payload)
  return { action = name, payload = payload }
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
  return { text = value, ink = color, translation = { x, y, 0 }, scale = { scale, scale, 1 } }
end

local function rect_prim(color, opacity, r, x, y, w, h)
  return { ink = color, opacity = opacity, radius = r or 0, translation = { x, y, -0.5 }, scale = { w, h, 1 } }
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
    -- Theme sizes are scene units, so padding is `size * S.ux`, not `wx(size)`:
    -- the second form converts a unit value as if it were pixels and collapsed
    -- to zero, which is how a row of buttons ended up stacked on one spot.
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

local function lay_text(n, S, width)
  local name = style(n.style)
  local size = n.px or S.theme[STYLE[name]]
  local lines = text_lines(n, S, width)
  -- Theme sizes are scene units (a fraction of the 8-unit square), so the
  -- advance is too: `hy` converts pixels, and feeding it a unit value made a
  -- body line advance 8 px while its glyphs were 14 px, so stacked text
  -- overlapped by a third of a line.
  local advance = size * ui.leading
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
      prims[#prims + 1] = {
        button = {
          id = slice.id,
          label = label,
          action = n.action,
          body = slice.body or { id = slice.id, value = slice.id },
          enabled = available,
          appearance = "invisible",
        },
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
      if prim.opacity ~= nil or prim.button then
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

  elseif kind == "icon" then
    local size = n.size and theme[n.size] or theme.body
    return { h = size * ui.leading,
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
      prims[#prims + 1] = {
        button = {
          id = control_id(n),
          label = n.glyph,
          action = n.action,
          body = n.body,
          enabled = n.enabled ~= false,
          appearance = "invisible",
        },
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
      prims[#prims + 1] = {
        button = {
          id = control_id(n),
          label = n.label,
          action = n.action,
          body = n.body,
          enabled = n.enabled ~= false,
          appearance = "invisible",
        },
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
    local advance = theme.body * ui.leading
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
    local advance = theme.body * ui.leading
    local indent = wx(S, 14)
    local lines = wrap(n.value, n.chars or budget(S, theme.body, width - indent))
    local height = #lines * advance
    local prims = { rect_prim("primary", 0.5, radius("sm", "quote"), 0, 0, wx(S, 2), height) }
    for index, line in ipairs(lines) do
      prims[#prims + 1] = text_prim(line, theme.body, S, "muted", indent, -(index - 1) * advance)
    end
    return { h = height, p = prims }

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

function ui.render(tree, theme)
  local S = context(theme)
  local body = lay(tree, S, 8.0)
  local prims = place(body.p, -4.0, 4.0)
  -- Opaque base: a surface is a window, not a hole in the wallpaper.
  table.insert(prims, 1, {
    ink = "background", opacity = 1, radius = 0,
    translation = { -4, 4, -1 }, scale = { 8, 8, 1 },
  })

  -- Controls first: the host owns their pixels, so there is nothing to draw
  -- under them, and their hit region has to be registered either way.
  local index = 0
  for _, prim in ipairs(prims) do
    index = index + 1
    if prim.button then
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
    end
  end

  -- Rects next (behind), then text.
  for _, prim in ipairs(prims) do
    index = index + 1
    local id = prim.id or string.format("e%d", index)
    if prim.button then
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

return ui
