-- fungos/ui — declarative UI for Spore views.
--
-- A view returns a tree; this library measures it, lays it out in flow, and
-- emits Canvas primitives. Everything comes from `module.theme`, so a theme
-- change moves the whole interface and no view names a colour or a pixel.
--
-- Two rules keep it honest:
--   1. The view is pure. It receives a snapshot and returns a tree. It holds no
--      state; state lives in the Spore and arrives in the snapshot.
--   2. Interaction is an *action*, not a callback. `on_click = ui.action("save")`
--      emits an intent into the scene; the Spore decides what it means. A view
--      that could mutate state would be a second authority.

local ui = {}

-- ---- user style ---------------------------------------------------------

ui.style = {
  density = "comfortable",
  radius = nil,
  spacing = { xs = 4, sm = 8, md = 16, lg = 28, xl = 44 },
  type = {},
  theme = {},
  surfaces = { panel = 0.42, selected = 0.16 },
  app = {},
}

local function copy(value)
  if type(value) ~= "table" then return value end
  local out = {}
  for key, item in pairs(value) do out[key] = copy(item) end
  return out
end

local function merge(base, override)
  if type(override) ~= "table" then return base end
  for key, value in pairs(override) do
    if type(value) == "table" and type(base[key]) == "table" then
      merge(base[key], value)
    else
      base[key] = copy(value)
    end
  end
  return base
end

--- Apply the host/user style. `overrides` is data, never code with authority.
function ui.configure(overrides, app_id)
  ui.style = copy(ui.style)
  merge(ui.style, overrides or {})
  if app_id and overrides and overrides.app and overrides.app[app_id] then
    merge(ui.style, overrides.app[app_id])
  end
  return ui.style
end

--- Resolve a theme with user type/color/radius overrides.
function ui.theme(base)
  local out = copy(base or {})
  merge(out, ui.style.type)
  merge(out, ui.style.theme)
  if ui.style.radius then out.radius = ui.style.radius end
  return out
end

-- ---- tokens -------------------------------------------------------------

-- 4px base in projected units. The default is a ~1720px-tall surface; the
-- host replaces it with the real viewport height when one is available, so
-- the same view follows a resized or differently shaped surface.
local U = 1 / 215
local function px(n) return n * U end

ui.gap = { none = 0, xs = px(4), sm = px(8), md = px(16), lg = px(28), xl = px(44) }
local gap = ui.gap

local function use_viewport(theme)
  local vp = theme and theme.viewport
  if not vp or not vp.height or vp.height <= 0 then return end
  U = 8 / vp.height
  local density = ({ compact = 0.82, comfortable = 1.0, spacious = 1.25 })[ui.style.density] or 1.0
  gap.none = 0
  for name, pixels in pairs(ui.style.spacing or {}) do
    gap[name] = pixels * U * density
  end
end

ui.leading = 2.8
ui.chars = { title = 34, body = 50, caption = 60 }

local function size_for(theme, style)
  return ({
    display = theme.title,
    title = theme.title,
    heading = theme.large,
    subheading = theme.large,
    body = theme.body,
    caption = theme.micro,
    mono = theme.small,
  })[style or "body"] or theme.body
end

-- ---- nodes --------------------------------------------------------------

local function node(kind, props)
  props.kind = kind
  props.children = props.children or {}
  return props
end

function ui.column(props) return node("column", props or {}) end
function ui.row(props) return node("row", props or {}) end
function ui.card(props) return node("card", props or {}) end
function ui.stack(props) return node("stack", props or {}) end

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
function ui.kbd(label) return node("button", { label = label, quiet = true }) end

-- A horizontal band with a background: title bars, toolbars, status lines.
function ui.bar(props) return node("bar", props or {}) end
function ui.modal(props) return node("modal", props or {}) end

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

-- `widget.hit` has no host implementation; see docs/interaction-gap.md. Call it
-- when a host provides it, otherwise keep the spec addressable rather than
-- crashing the module inside Canvas, where `widget` is the theming library.
ui.pending_hits = {}
function ui.hit(spec)
  if type(widget) == "table" and type(widget.hit) == "function" then
    widget.hit(spec)
  else
    ui.pending_hits[#ui.pending_hits + 1] = spec
  end
end

-- ---- layout -------------------------------------------------------------
--
-- Canvas has no layout service. Its scene API takes absolute transforms and its
-- module API draws at positions the module computes, so a view that asks the host
-- to lay out its tree and hand back rects cannot run there. `canvas.layout` is
-- provided only by the development hosts (tend's fake host, this fixture).
--
-- `ui.layout` is the pure-Lua fallback for that absence. It honours the layout
-- hints the views actually declare (direction, gap, padding, fixed sizes and
-- flex_grow) and returns the same shape the host does: a flat node list where
-- every node carries a pixel `rect`. It is deliberately not a stub: the fixture
-- and Canvas draw the same scene through it. This section is load-bearing and
-- lives on its own so the composer can inline it without the rest of the kit.
-- BEGIN-LAYOUT

local function layout_padding(node)
  local padding = node.layout and node.layout.padding
  local top, right, bottom, left = 0, 0, 0, 0
  if type(padding) == "table" then
    top = padding.top or 0
    right = padding.right or 0
    bottom = padding.bottom or 0
    left = padding.left or 0
  end
  return top, right, bottom, left
end

local function layout_fixed(node, direction)
  local spec = node.layout or {}
  local value = direction == "row" and spec.width or spec.height
  return type(value) == "number" and value or nil
end

local function layout_default(node)
  if node.kind == "text" then
    local size = node.style and node.style.size
    return type(size) == "number" and (size + 4) or 20
  end
  if node.kind == "rule" then return 2 end
  if node.kind == "spacer" then return 8 end
  return nil
end

local function layout_assign(node, x, y, width, height, out)
  node.rect = { x = x, y = y, width = width, height = height }
  out[#out + 1] = node

  local children = node.children
  if type(children) ~= "table" or #children == 0 then return end

  local spec = node.layout or {}
  local direction = spec.direction or "column"
  local gap = spec.gap or 0
  local pad_top, pad_right, pad_bottom, pad_left = layout_padding(node)
  local content_x, content_y = x + pad_left, y + pad_top
  local content_w = width - pad_left - pad_right
  local content_h = height - pad_top - pad_bottom

  local count = #children
  local total = direction == "row" and content_w or content_h
  local gaps = gap * math.max(0, count - 1)

  local sizes, fixed, flex_total = {}, 0, 0
  for index, child in ipairs(children) do
    local size = layout_fixed(child, direction) or layout_default(child)
    if size ~= nil then
      sizes[index] = size
      fixed = fixed + size
    else
      sizes[index] = false
      local grow = child.layout and child.layout.flex_grow or 0
      if type(grow) == "number" and grow > 0 then flex_total = flex_total + grow end
    end
  end

  local remaining = math.max(0, total - fixed - gaps)
  for index, child in ipairs(children) do
    if sizes[index] == false then
      local grow = child.layout and child.layout.flex_grow or 0
      if flex_total > 0 and type(grow) == "number" and grow > 0 then
        sizes[index] = remaining * (grow / flex_total)
      else
        sizes[index] = remaining / math.max(1, count)
      end
    end
  end

  local cursor = 0
  for index, child in ipairs(children) do
    local size = sizes[index]
    if direction == "row" then
      layout_assign(child, content_x + cursor, content_y, size, content_h, out)
    else
      layout_assign(child, content_x, content_y + cursor, content_w, size, out)
    end
    cursor = cursor + size + gap
  end
end

--- Lay out `tree` in a `width` x `height` surface.
---
--- Returns `{ root = <rect>, nodes = <flat list> }`, matching the host's shape so
--- a view can reach for either without noticing.
function ui.layout(tree, width, height)
  assert(type(tree) == "table", "ui.layout needs a tree")
  local out = {}
  layout_assign(tree, 0, 0, width or 0, height or 0, out)
  return { root = tree.rect, nodes = out }
end
-- END-LAYOUT

-- ---- measure ------------------------------------------------------------


local function wrap(text, chars)
  local lines, line = {}, ""
  for word in text:gmatch("%S+") do
    if #line == 0 then
      line = word
    elseif #line + 1 + #word <= chars then
      line = line .. " " .. word
    else
      lines[#lines + 1] = line
      line = word
    end
  end
  if #line > 0 then lines[#lines + 1] = line end
  return lines
end

-- Character-count estimate, not metrics. Enough to right-align a status label;
-- never used to decide a wrap.
local function estimate_width(n, theme)
  if n.kind ~= "text" then return 0 end
  local size = n.px or size_for(theme, n.style)
  return #n.value * size * 0.72 * U
end

-- Character budget derived from the *column* width, not a global constant.
-- 8 body characters per projected unit is calibrated against the rendered page;
-- it is an estimate until the host exposes text metrics, so it is named once
-- here rather than sprinkled through views.
local CHARS_PER_UNIT = 11

local function budget(theme, size, width)
  return math.max(8, math.floor(width * CHARS_PER_UNIT * (theme.body / size)))
end

-- Intrinsic width of a leaf, in units. Used to sequence a row without metrics.
local function intrinsic(n, theme)
  if n.kind == "button" then
    return estimate_width({ kind = "text", value = n.label, style = n.small and "caption" or "body" }, theme) + 0.46
  elseif n.kind == "text" then
    return estimate_width(n, theme)
  elseif n.kind == "kbd" then
    return 0.5
  elseif n.kind == "icon" then
    local size = n.size and theme[n.size] or theme.body
    return px(size) * 1.1
  elseif n.kind == "icon_button" then
    local size = n.size and theme[n.size] or theme.body
    return px(size) + px(size * 0.9)
  end
  return 0
end

local function text_lines(n, theme, width)
  local size = n.px or size_for(theme, n.style)
  return wrap(n.value, n.chars or budget(theme, size, width))
end

-- Forward declaration: row_extent measures children, and measure measures rows.
local measure

-- A row's height is the tallest child measured at *its own* width, otherwise
-- the row reports less height than it draws and the next sibling overlaps it.
local function row_extent(n, theme, width)
  local last = #n.children
  local spacing = gap[n.gap or "none"] or 0
  local total = width - spacing * (last - 1)
  local tallest = 0
  for index, child in ipairs(n.children) do
    local child_width = total
    if n.widths then
      child_width = total * n.widths[index]
    else
      local own = intrinsic(child, theme)
      if own > 0 then child_width = own end
    end
    tallest = math.max(tallest, measure(child, theme, child_width))
  end
  return tallest
end

measure = function(n, theme, width)
  if n.kind == "text" then
    local size = n.px or size_for(theme, n.style)
    return #text_lines(n, theme, width) * px(size * ui.leading)
  elseif n.kind == "rule" then
    return px(10)
  elseif n.kind == "fill" then
    return n.h or 0
  elseif n.kind == "button" then
    return px((n.small and theme.small or theme.body) * ui.leading + 14)
  elseif n.kind == "icon" then
    return px((n.size and theme[n.size] or theme.body) * ui.leading)
  elseif n.kind == "icon_button" then
    local size = n.size and theme[n.size] or theme.body
    return px(size) + px(size * 0.9)
  elseif n.kind == "bar" then
    local pad = n.pad or gap.sm
    local child = 0
    for _, c in ipairs(n.children) do child = math.max(child, measure(c, theme, width - pad * 2)) end
    return math.max(n.h or 0, child + pad * 2)
  elseif n.kind == "spacer" then
    return gap[n.size or "md"]
  elseif n.kind == "code" then
    local lines = 0
    for _ in (n.source .. "\n"):gmatch("(.-)\n") do lines = lines + 1 end
    return px(theme.small * 1.8 * (lines + 1) + 8)
  elseif n.kind == "diagram" then
    return px(theme.small * 1.8 + 18)
  elseif n.kind == "bullets" then
    local height = 0
    for _, item in ipairs(n.items) do
      height = height + #wrap(item, n.chars or budget(theme, theme.body, width - 0.34)) * px(theme.body * ui.leading)
    end
    return height
  elseif n.kind == "quote" then
    return #wrap(n.value, 47) * px(theme.body * ui.leading)
  elseif n.kind == "column" or n.kind == "card" then
    local height = 0
    for index, child in ipairs(n.children) do
      if index > 1 then height = height + gap[n.gap or "sm"] end
      height = height + measure(child, theme, width)
    end
    if n.kind == "card" then height = height + gap.md end
    return height
  elseif n.kind == "row" then
    return row_extent(n, theme, width)
  elseif n.kind == "modal" then
    return 0
  end
  return 0
end

-- ---- layout + emit ------------------------------------------------------

local function emit(n, theme, x, y, width, out)

  local function text_at(value, size, ink, indent, z)
    local s = math.sqrt(size / theme.body)
    out[#out + 1] = {
      id = string.format("t%d", #out + 1),
      text = value,
      ink = ink or "text",
      translation = { x + (indent or 0), y, z or 0 },
      scale = { s, s, 1 },
    }
  end

  local function rect(id, rx, ry, rw, rh, ink, opacity, radius, z)
    out[#out + 1] = {
      id = id,
      ink = ink,
      opacity = opacity,
      radius = radius or 0,
      translation = { rx, ry, z or -0.5 },
      scale = { rw, rh, 1 },
    }
  end

  if n.kind == "text" then
    local size = n.px or size_for(theme, n.style)
    for _, line in ipairs(text_lines(n, theme, width)) do
      text_at(line, size, n.ink or (n.style == "title" and "primary" or "text"))
      y = y - px(size * ui.leading)
    end
    return y

  elseif n.kind == "rule" then
    rect(string.format("r%d", #out + 1), x, y + px(4), width, 0.003, "muted", 0.26, 0)
    return y - px(10)

  elseif n.kind == "icon" then
    local size = n.size and theme[n.size] or theme.body
    text_at(n.glyph, size, n.ink or "muted")
    return y - px(size * ui.leading)

  elseif n.kind == "icon_button" then
    local size = n.size and theme[n.size] or theme.body
    local side = px(size) + px(size * 0.9)
    local ink = n.quiet and "surface" or "primary"
    local opacity = n.quiet and 0.5 or (n.selected and 1.0 or 0.85)
    rect(string.format("ib%d", #out + 1), x, y - side, side, side, ink, opacity, theme.radius)
    text_at(n.glyph, size, n.quiet and "muted" or "background", px(size * 0.45))
    out[#out].translation[2] = y - px(size * 0.55)
    return y - side

  elseif n.kind == "button" then
    local size = n.small and theme.small or theme.body
    -- A pill is text plus padding: height follows the type size, and the label
    -- is centred in it. Width stays an estimate until the host exposes metrics.
    local text_h = px(size)
    local vpad = px(size * 0.45)
    local hpad = px(size * 0.52)
    local height = text_h + vpad * 2
    local width_est = #n.label * size * 1.15 * U + hpad * 2
    local ink = n.quiet and "surface" or "primary"
    local opacity = n.quiet and 0.55 or (n.selected and 1.0 or 0.9)
    local button_id = n.id or string.format("btn%d", #out + 1)
    rect(button_id, x, y - height, width_est, height, ink, opacity, theme.radius)
    text_at(n.label, size, n.quiet and "muted" or "background", hpad)
    out[#out].translation[2] = y - vpad - text_h * 0.92
    if y - height >= -4.001 and y <= 4.001 and x >= -4.001 and x + width_est <= 4.001 then
      ui.hit {
        id = button_id .. "-hit",
        label = "hit",
        x = x,
        y = y - height,
        width = width_est,
        height = height,
      }
    end
    return y - height

  elseif n.kind == "fill" then
    rect(n.id or string.format("f%d", #out + 1), x, y, n.w or width, n.h or 0,
         n.ink or "surface", n.opacity or 0.6, n.radius or 0)
    return y - (n.h or 0)

  elseif n.kind == "bar" then
    local pad = n.pad or gap.sm
    local child_h = 0
    for _, c in ipairs(n.children) do child_h = math.max(child_h, measure(c, theme, width - pad * 2)) end
    local height = math.max(n.h or 0, child_h + pad * 2)
    local bleed = n.bleed or 0
    rect(string.format("bar%d", #out + 1), x - bleed, y + (n.dy or 0),
         width + bleed * 2, height, n.ink or "surface", n.opacity or 0.9, n.radius or 0)
    if n.border then
      rect(string.format("barb%d", #out + 1), x - bleed,
           y - height + (n.border_top and height or 0), width + bleed * 2,
           0.0025, "muted", 0.22, 0)
    end
    -- Children start at the top padding, and the band grows to fit them, so a
    -- bar can never be shorter than its contents.
    local inner_y = y - pad
    for _, child in ipairs(n.children) do
      emit(child, theme, x + pad, inner_y, width - pad * 2, out)
    end
    if n.hit and y - height >= -4.001 and y <= 4.001 and x - bleed >= -4.001 and x - bleed + width + bleed * 2 <= 4.001 then
      ui.hit {
        id = n.hit.id or string.format("hit%d", #out + 1),
        label = "hit",
        x = x - bleed,
        y = y - height,
        width = width + bleed * 2,
        height = height,
      }
    end
    return y - height

  elseif n.kind == "modal" then
    local pad = n.pad or gap.md
    rect(string.format("modal-dim%d", #out + 1), -4, 4, 8, 8, "background", 0.72, 0, 1.0)
    rect(string.format("modal-card%d", #out + 1), n.x or -2.2, n.y or 2.4,
         n.w or 4.4, n.h or 2.4, "surface", 1.0, theme.radius, 1.1)
    local inner_y = (n.y or 2.4) - pad
    for _, child in ipairs(n.children) do
      local first = #out + 1
      emit(child, theme, (n.x or -2.2) + pad, inner_y,
           (n.w or 4.4) - pad * 2, out)
      for index = first, #out do
        if out[index].opacity ~= nil then
          out[index].translation[3] = 1.25
        else
          out[index].translation[3] = 1.35
        end
      end
    end
    return y
  elseif n.kind == "spacer" then
    return y - gap[n.size or "md"]

  elseif n.kind == "code" then
    local lines = {}
    for line in (n.source .. "\n"):gmatch("(.-)\n") do lines[#lines + 1] = line end
    if lines[#lines] == "" then table.remove(lines) end
    local height = px(theme.small * 1.8 * (#lines + 1) + 8)
    rect(string.format("c%d", #out + 1), x - 0.16, y + px(4), width + 0.32, height, "surface", 0.95, theme.radius)
    if n.language and n.language ~= "" then
      text_at(n.language, theme.micro, "muted", width - 0.6)
    end
    y = y - px(4)
    for _, line in ipairs(lines) do
      text_at(line, theme.small, "muted")
      y = y - px(theme.small * 1.8)
    end
    return y - px(8)

  elseif n.kind == "diagram" then
    local height = px(theme.small * 1.8 + 18)
    rect(string.format("d%d", #out + 1), x - 0.16, y + px(4), width + 0.32, height, "surface", 0.55, theme.radius)
    text_at("[" .. n.format .. "]  " .. n.source, theme.small, "muted", 0.02)
    return y - height - gap.xs

  elseif n.kind == "bullets" then
    for _, item in ipairs(n.items) do
      text_at("•", theme.body, "muted", gap.sm)
      for _, line in ipairs(wrap(item, n.chars or budget(theme, theme.body, width - 0.34))) do
        text_at(line, theme.body, "text", 0.34)
        y = y - px(theme.body * ui.leading)
      end
    end
    return y

  elseif n.kind == "quote" then
    local lines = wrap(n.value, n.chars or budget(theme, theme.body, width - 0.25))
    local height = #lines * px(theme.body * ui.leading)
    rect(string.format("q%d", #out + 1), x, y, 0.008, height + px(4), "primary", 0.5, 2)
    for _, line in ipairs(lines) do
      text_at(line, theme.body, "muted", 0.20)
      y = y - px(theme.body * ui.leading)
    end
    return y

  elseif n.kind == "row" then
    -- Rows place aligned pairs, or weighted columns when `widths` is given.
    -- Free horizontal flow would need text metrics, which the host does not
    -- expose and the library refuses to guess beyond a character estimate.
    local tallest = row_extent(n, theme, width)
    local last = #n.children
    local spacing = gap[n.gap or "none"] or 0

    if n.widths then
      local total = width - spacing * (last - 1)
      local cursor = x
      for index, child in ipairs(n.children) do
        local child_width = total * n.widths[index]
        emit(child, theme, cursor, y, child_width, out)
        cursor = cursor + child_width + spacing
      end
    else
      local own = {}
      local used = spacing * (last - 1)
      for index, child in ipairs(n.children) do
        own[index] = intrinsic(child, theme)
        used = used + own[index]
      end
      local cursor = n.align == "end" and (x + width - used - gap.xs) or x
      for index, child in ipairs(n.children) do
        local remaining = x + width - cursor
        emit(child, theme, cursor, y, math.max(own[index], remaining), out)
        cursor = cursor + own[index] + spacing
      end
    end
    return y - tallest

  elseif n.kind == "card" or n.kind == "column" then
    local top = y
    local inner = x + (n.pad or 0)
    local inner_width = width - (n.pad or 0) * 2
    if n.pad then y = y - n.pad end
    if n.kind == "card" then
      local height = measure(n, theme, width)
      rect(string.format("k%d", #out + 1), x - 0.16, y + px(6), width + 0.32, height - px(6), "surface", 0.9, theme.radius)
      inner = x + gap.sm
      inner_width = width - gap.sm * 2
      y = y - gap.sm
    end

    -- A column with a fixed height distributes what is left to children marked
    -- `grow`. That is how a footer stays pinned to the bottom of a fixed frame
    -- instead of floating after the last line of content.
    local spacing = gap[n.gap or "sm"] or 0
    local heights, used = {}, 0
    for index, child in ipairs(n.children) do
      heights[index] = measure(child, theme, inner_width)
      used = used + heights[index]
    end
    used = used + spacing * (#n.children - 1)
    local extra = math.max(0, (n.h or 0) - used)

    for index, child in ipairs(n.children) do
      if index > 1 then y = y - spacing end
      local before = y
      y = emit(child, theme, inner, y, inner_width, out)
      if child.grow and n.h then
        y = before - (heights[index] + extra)
      end
    end
    if n.kind == "card" then y = y - gap.sm end
    if n.pad then y = y - n.pad end
    return y
  end

  return y
end

function ui.render(tree, theme)
  use_viewport(theme)
  local out = {}
  -- Opaque base: a surface is a window, not a hole in the wallpaper.
  out[#out + 1] = {
    id = "bg",
    ink = "background",
    opacity = 1,
    radius = 0,
    translation = { -4, 4, -1 },
    scale = { 8, 8, 1 },
  }
  emit(tree, theme, -4.0, 4.0, 8.0, out)
  -- Rects first (behind), then text.
  for _, n in ipairs(out) do
    if n.opacity ~= nil then
      canvas.rect({ id = n.id, ink = n.ink, opacity = n.opacity, radius = n.radius,
                    translation = n.translation, scale = n.scale })
    end
  end
  for _, n in ipairs(out) do
    if n.opacity == nil then
      canvas.text({ id = n.id, text = n.text, ink = n.ink,
                    translation = n.translation, scale = n.scale })
    end
  end
end

return ui
