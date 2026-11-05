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

ui.gap = { xs = px(4), sm = px(8), md = px(16), lg = px(28), xl = px(44) }
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
  return #n.value * size * 0.48 * U
end

local function text_lines(n, theme)
  local chars = ui.chars[n.style or "body"] or ui.chars.body
  return wrap(n.value, n.chars or chars)
end

local function measure(n, theme, width)
  if n.kind == "text" then
    local size = n.px or size_for(theme, n.style)
    return #text_lines(n, theme) * px(size * ui.leading)
  elseif n.kind == "rule" then
    return px(10)
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
      height = height + #wrap(item, ui.chars.body) * px(theme.body * ui.leading)
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
    local tallest = 0
    for _, child in ipairs(n.children) do
      tallest = math.max(tallest, measure(child, theme, width))
    end
    return tallest
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
    for _, line in ipairs(text_lines(n, theme)) do
      text_at(line, size, n.ink or (n.style == "title" and "primary" or "text"))
      y = y - px(size * ui.leading)
    end
    return y

  elseif n.kind == "rule" then
    rect(string.format("r%d", #out + 1), x, y + px(4), width, 0.003, "muted", 0.26, 0)
    return y - px(10)

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
      for _, line in ipairs(wrap(item, ui.chars.body)) do
        text_at(line, theme.body, "text", 0.34)
        y = y - px(theme.body * ui.leading)
      end
    end
    return y

  elseif n.kind == "quote" then
    local lines = wrap(n.value, 47)
    local height = #lines * px(theme.body * ui.leading)
    rect(string.format("q%d", #out + 1), x, y, 0.008, height + px(4), "primary", 0.5, 2)
    for _, line in ipairs(lines) do
      text_at(line, theme.body, "muted", 0.20)
      y = y - px(theme.body * ui.leading)
    end
    return y

  elseif n.kind == "row" then
    -- Rows place aligned pairs: first child left, last child right. Free
    -- horizontal flow would need text metrics, which the host does not expose
    -- and the library refuses to guess beyond a character-width estimate.
    local tallest = 0
    for _, child in ipairs(n.children) do
      tallest = math.max(tallest, measure(child, theme, width))
    end
    local last = #n.children
    for index, child in ipairs(n.children) do
      local child_x = x
      if index == last and last > 1 then
        child_x = x + width - estimate_width(child, theme)
      end
      emit(child, theme, child_x, y, width, out)
    end
    return y - tallest

  elseif n.kind == "card" or n.kind == "column" then
    local top = y
    local inner = x
    if n.kind == "card" then
      local height = measure(n, theme, width)
      rect(string.format("k%d", #out + 1), x - 0.16, y + px(6), width + 0.32, height - px(6), "surface", 0.9, theme.radius)
      inner = x + gap.sm
      y = y - gap.sm
    end
    for index, child in ipairs(n.children) do
      if index > 1 then y = y - gap[n.gap or "sm"] end
      y = emit(child, theme, inner, y, width - (inner - x), out)
    end
    if n.kind == "card" then y = y - gap.sm end
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
  emit(tree, theme, -3.45, 3.4, 6.9, out)
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
