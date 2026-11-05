-- The native compositor owns selection, focus and visibility; this file owns how
-- that reads. No actions and no I/O: the host already knows what the keys mean,
-- and a switcher that could act would be a second place to be wrong about focus.
local i, t = module.inputs, module.theme
local w, h = i.width or 920, i.height or 208
local p = widget.pixel_space(w, h)
local k = widget.tokens()

-- 1.0 is a drawn ground: the overlay window is transparent, so without this the
-- desktop shows through the gaps between cards.
widget.glass {
  id = "ground", space = p, x = 0, y = 0, width = w, height = h,
  ink = "surface", opacity = 0.94, radius = k.radius.lg,
  elevation = 0, transition_ms = k.motion.fast,
}

local slots = {}
for n = 0, 5 do
  if i["row" .. n .. "-title"] then slots[#slots + 1] = n end
end
local count = math.max(1, #slots)

-- Rows are a rhythm, not a grid: the same inset on both sides, one gap, and the
-- leftover width shared evenly, so three windows and six windows both look meant.
local inset, gap = 16, 12
local tile = (w - 2 * inset - gap * (count - 1)) / count
local card_h = h - 46
-- Content is centred in the card rather than pinned to the top: a switcher with
-- three windows should not look like it lost the rest of a list.
local mark = 34
local block = mark + 12 + k.type.body
local top = 12 + math.max(k.space.sm, (card_h - block) * 0.5)

local function fit(text, room, size)
  text = tostring(text or "")
  local chars = math.max(1, math.floor(room / (size * 0.52)))
  if utf8.len(text) <= chars then return text end
  local cut = utf8.offset(text, chars + 1)
  return cut and (string.sub(text, 1, cut - 1) .. "…") or text
end

-- A tint per application, so two windows of the same app read as the same app
-- without shipping icons: a hash of the name, not a lookup table that rots.
local tints = { "#6ca8ff", "#7fd0a8", "#e8b25a", "#c99be8", "#e8899b", "#6fd0e0" }
local function tint(name)
  local sum = 0
  for index = 1, #name do sum = sum + name:byte(index) * index end
  return tints[(sum % #tints) + 1]
end

local function monogram(name)
  local ok, code = pcall(utf8.codepoint, name)
  if not ok or not code then return "?" end
  return utf8.char(code):upper()
end

local selected = nil
for index, slot in ipairs(slots) do
  local key = "row" .. slot
  local left = inset + (index - 1) * (tile + gap)
  local app = i[key .. "-detail"] or "Application"
  local is_selected = i[key .. "-selected"] == true
  if is_selected then selected = index end

  widget.glass {
    id = key .. "-card", space = p, x = left, y = 12, width = tile, height = card_h,
    ink = "background", opacity = is_selected and 0.55 or 0.32,
    radius = k.radius.md, elevation = is_selected and k.elevation.raised or 0,
    shadow_opacity = 0.28, transition_ms = k.motion.fast,
  }
  if is_selected then
    -- Selection is an edge, not a wash: a filled card competes with the title it
    -- is supposed to frame, and the scene has no stroke pass to give it an
    -- outline, so the edge is a lit top bar.
    widget.panel {
      id = key .. "-edge", x = p.x(left), y = p.y(12), z = 0.24,
      width = p.w(tile), height = p.h(k.stroke.strong),
      ink = "primary", opacity = 0.95, radius = k.stroke.strong,
    }
  end

  local mark_x = left + k.space.md
  local mark_y = top
  widget.tile {
    id = key .. "-mark", space = p, x = mark_x, y = mark_y, z = 0.26,
    size = mark, radius = mark * 0.5, text = monogram(app),
    size_text = 15, rgb = tint(app), ink = "primary", opacity = 0.22,
    text_ink = "text",
  }
  -- The selection ring: a dotted Lissajous that turns while the switcher is up.
  -- It says "this gesture is live" without claiming what it is loading, and it is
  -- the reason the overlay asks the host for a frame clock at all.
  if is_selected then
    widget.spinner {
      id = key .. "-ring", space = p, x = mark_x - 6, y = mark_y - 6, radius = mark * 0.5 + 6,
      dots = 22, ink = "primary", period_ms = k.motion.spin, opacity = 0.85, z = 0.27,
    }
  end

  widget.label {
    id = key .. "-app", text = fit(app, tile - mark - k.space.md * 3, k.type.micro),
    x = p.x(mark_x + mark + k.space.sm), y = p.y(mark_y + 10), z = 0.28,
    size = k.type.micro, ink = "muted",
  }
  widget.label {
    id = key .. "-title", text = fit(i[key .. "-title"], tile - k.space.md * 2 - 4, k.type.body),
    x = p.x(left + k.space.md), y = p.y(mark_y + mark + 12), z = 0.28,
    size = k.type.body, ink = "text",
  }
end

-- The footer is an instruction, not decoration: three chords, then where you are.
local hint = i.health or "Alt + Tab"
widget.label {
  id = "hint", text = fit(hint, w * 0.62, k.type.micro),
  x = p.x(inset), y = p.y(h - 26), size = k.type.micro, ink = "muted",
}
do
  local position = (selected or 1) .. " / " .. #slots
  widget.label {
    id = "position", text = position,
    x = p.x(w - inset - #position * (k.type.small * 0.6)), y = p.y(h - 28),
    size = k.type.small, ink = "text",
  }
end
