-- Agent approval window.
--
-- Geometry comes from the Canvas host: this view describes rows, text and
-- buttons, and the host measures glyphs with the same font it draws. The view
-- never guesses character widths or wraps a string by character count.

local M = {}

M.name = "approval"
M.version = "0.2.0"

-- The approval surface is placed at 0.56 x 0.26 of a 1200 x 1920 frame. Layout
-- in that logical pixel space so text sizes and hit targets are predictable.
local DESIGN_W, DESIGN_H = 672, 499
local Z = 0

local DEFAULT_THEME = {
  body = 16,
  radius = 0.02,
  primary = { 0.38, 0.65, 0.98 },
}

local function theme()
  return (_G.module and _G.module.theme) or DEFAULT_THEME
end

local function base()
  local t = theme()
  return t.body or 16
end

-- Canvas has no layout service (see docs/interaction-gap.md). Prefer the host's
-- `canvas.layout` when one provides it, otherwise lay the tree out in Lua.
local function lay(tree, width, height)
  if canvas and canvas.layout then return canvas.layout(tree, width, height) end
  return ui.layout(tree, width, height)
end

local function nx(px) return -4 + 8 * px / DESIGN_W end
local function ny(px) return 4 - 8 * px / DESIGN_H end
local function sw(px) return 8 * px / DESIGN_W end
local function sh(px) return 8 * px / DESIGN_H end

local function text_scale(px)
  -- scene_text_size uses the y scale and a 1.25 z-depth at z=0.
  return px / ((base() or 16) * 1.25)
end

-- Canvas rejects ink roles it does not know rather than falling back to body
-- text. A surface the theme does not name has to be spelled with one it does.
local INK = { surface = "muted", background = "muted" }
local function ink(name)
  return INK[name] or name
end

local function rect(id, x, y, w, h, role, opacity, radius)
  canvas.rect {
    id = id,
    ink = ink(role or "surface"),
    opacity = opacity or 1.0,
    radius = radius or 12,
    translation = { nx(x), ny(y), Z },
    scale = { sw(math.max(1, w)), sh(math.max(1, h)), 1 },
  }
end

local function label(id, value, x, y, px, role, z)
  canvas.text {
    id = id,
    text = tostring(value or ""),
    ink = ink(role or "text"),
    translation = { nx(x), ny(y), z or (Z + 0.1) },
    scale = { text_scale(px), text_scale(px), 1 },
  }
end

local function measured(value, px)
  local ok, size = pcall(canvas.measure_text, tostring(value or ""), {
    size = px,
    line_height = math.max(px + 3, px * 1.25),
    wrap = false,
  })
  if ok and size then return size.width or 0, size.height or 0 end
  return #tostring(value or "") * px * 0.55, px * 1.25
end

local function find_node(laid, id)
  for _, node in ipairs(laid.nodes or {}) do
    if node.id == id then return node end
  end
end

local function draw_button(laid, id, label_text, variant, z)
  local node = find_node(laid, id)
  if not node then return end
  local r = node.rect
  local radius = 12

  if variant == "primary" or variant == "danger" then
    rect(id .. "-fill", r.x, r.y, r.width, r.height, variant, 1.0, radius)
  elseif variant == "secondary" then
    rect(id .. "-border", r.x, r.y, r.width, r.height, "primary", 0.55, radius)
    rect(id .. "-fill", r.x + 1.5, r.y + 1.5, r.width - 3, r.height - 3, "surface", 1.0, radius - 1)
  else
    rect(id .. "-fill", r.x, r.y, r.width, r.height, "surface", 0.55, radius)
  end

  local ink = (variant == "primary" or variant == "danger") and "background" or "primary"
  if variant == "neutral" then ink = "text" end
  local tw, th = measured(label_text, 14)
  label(
    id .. "-label",
    label_text,
    r.x + math.max(0, (r.width - tw) / 2),
    r.y + math.max(0, (r.height - th) / 2),
    14,
    ink,
    z
  )
end

local function layout_tree(snapshot)
  local t = theme()
  local b = base()
  local pad = 26
  local gap = 12
  local title_px = math.max(20, b * 1.35)
  local body_px = math.max(14, b * 0.95)
  local caption_px = math.max(11, b * 0.75)
  local button_h = 48

  local function text(id, value, style, layout)
    return {
      id = id,
      kind = "text",
      text = tostring(value or ""),
      style = style,
      layout = layout or {},
    }
  end

  local function button(id, value)
    return {
      id = id,
      kind = "container",
      layout = { height = button_h, flex_grow = 1.0 },
      children = {},
    }
  end

  return {
    id = "approval",
    kind = "container",
    layout = {
      width = DESIGN_W,
      height = DESIGN_H,
      direction = "column",
      gap = gap,
      padding = { top = pad, right = pad, bottom = pad, left = pad },
    },
    children = {
      text("approval-title", snapshot.title or "Send this reply?", {
        size = title_px, line_height = title_px + 8, wrap = true,
      }),
      text("approval-kicker", "AGENT APPROVAL  ·  ONE DECISION", {
        size = caption_px, line_height = caption_px + 5, wrap = false,
      }, { height = caption_px + 5 }),
      text("approval-to", "To: " .. (snapshot.to or ""), {
        size = body_px, line_height = body_px + 5, wrap = false,
      }, { height = body_px + 5 }),
      { id = "approval-rule", kind = "rule", layout = { height = 1 } },
      text("approval-draft", snapshot.draft or "", {
        size = body_px, line_height = body_px + 7, wrap = true,
      }),
      text("approval-policy", snapshot.policy or "", {
        size = caption_px, line_height = caption_px + 5, wrap = true,
      }),
      { id = "approval-spacer", kind = "spacer", layout = { flex_grow = 1.0 } },
      {
        id = "approval-buttons",
        kind = "container",
        layout = { direction = "row", gap = 10, height = button_h },
        children = {
          button("approval-allow", "Allow"),
          button("approval-always", "Always Allow"),
          button("approval-deny", "Deny"),
        },
      },
    },
  }
end

function M.render(snapshot)
  snapshot = snapshot or {}
  local tree = layout_tree(snapshot)
  local laid = lay(tree, DESIGN_W, DESIGN_H)
  -- Window body and border.
  rect("approval-window", 0, 0, DESIGN_W, DESIGN_H, "surface", 1.0, 16)
  rect("approval-border", 0, 0, DESIGN_W, DESIGN_H, "primary", 0.22, 16)
  rect("approval-inset", 1.5, 1.5, DESIGN_W - 3, DESIGN_H - 3, "surface", 1.0, 15)

  for _, node in ipairs(laid.nodes or {}) do
    local r = node.rect
    if node.kind == "rule" then
      rect(node.id .. "-line", r.x, r.y, r.width, math.max(1, r.height), "muted", 0.25, 0)
    elseif node.kind == "text" and node.id ~= "approval-title" then
      local px = (node.style and node.style.size) or b
      local ink = "text"
      if node.id == "approval-kicker" then ink = "warning" end
      if node.id == "approval-to" or node.id == "approval-policy" then ink = "muted" end
      -- Use the measured rect's baseline position; the host measured this line.
      label(node.id, node.text, r.x, r.y, px, ink)
    end
  end

  -- The title is the same primitive, but kept explicit so its hierarchy is clear.
  local title = find_node(laid, "approval-title")
  if title then
    label("approval-title", snapshot.title or "Send this reply?", title.rect.x, title.rect.y, title.style.size, "text")
  end

  draw_button(laid, "approval-allow", "Allow", "primary", Z + 0.2)
  draw_button(laid, "approval-always", "Always Allow", "secondary", Z + 0.2)
  draw_button(laid, "approval-deny", "Deny", "danger", Z + 0.2)
end

return M
