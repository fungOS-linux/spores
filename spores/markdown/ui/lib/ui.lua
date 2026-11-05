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

-- ---- tokens -------------------------------------------------------------

-- 4px base, expressed in projected units for a surface ~1720px tall.
local U = 1 / 215
local function px(n) return n * U end

ui.gap = { none = 0, xs = px(4), sm = px(8), md = px(16), lg = px(28), xl = px(44) }
local gap = ui.gap

ui.leading = 2.1
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
  end
  return 0
end

-- ---- layout + emit ------------------------------------------------------

local function emit(n, theme, x, y, width, out)

  local function text_at(value, size, ink, indent)
    local s = math.sqrt(size / theme.body)
    out[#out + 1] = {
      id = string.format("t%d", #out + 1),
      text = value,
      ink = ink or "text",
      translation = { x + (indent or 0), y, 0 },
      scale = { s, s, 1 },
    }
  end

  local function rect(id, rx, ry, rw, rh, ink, opacity, radius)
    out[#out + 1] = {
      id = id,
      ink = ink,
      opacity = opacity,
      radius = radius or 0,
      translation = { rx, ry, -0.5 },
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

  elseif n.kind == "button" then
    local size = n.small and theme.small or theme.body
    -- A pill is text plus padding: height follows the type size, and the label
    -- is centred in it. Width stays an estimate until the host exposes metrics.
    local text_h = px(size)
    local vpad = px(size * 0.45)
    local hpad = px(size * 0.52)
    local height = text_h + vpad * 2
    local width_est = #n.label * size * 0.95 * U + hpad * 2
    local ink = n.quiet and "surface" or "primary"
    local opacity = n.quiet and 0.55 or (n.selected and 1.0 or 0.9)
    rect(string.format("btn%d", #out + 1), x, y - height, width_est, height, ink, opacity, theme.radius)
    text_at(n.label, size, n.quiet and "muted" or "background", hpad)
    out[#out].translation[2] = y - vpad - text_h * 0.92
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
    return y - height

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
      local cursor = n.align == "end" and (x + width - used) or x
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
