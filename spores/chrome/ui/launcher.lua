-- Spotlight: logical pixels, shared desktop primitives, no dashboard chrome.
--
-- A launcher is a list you type into. Everything here serves that: a place to
-- read the query, rows tall enough to hit, a mark so two apps are distinguishable
-- before you read them, and a footer that says what the keys do. The host owns the
-- hit targets and the selection; this file only has to make the selection visible.
local i, t = module.inputs, module.theme
local w, h = i.width or 600, i.height or 440
local p = widget.pixel_space(w, h)
local k = widget.tokens()

-- The sheet is behind everything it contains, so it says so: a ground that shares
-- a depth with the rows can end up in front of them.
widget.panel({ id = "ground", x = -4, y = 4, width = 8, height = 8, ink = "surface", opacity = 1, radius = t.radius, z = -1 })

if i["window-cycle"] then
  -- Alt-Tab: the same tiles the switcher region draws, in the launcher's window.
  local count = 0
  for n = 0, 5 do if i["row" .. n .. "-title"] then count = count + 1 end end
  count = math.max(1, count)
  local gap, inset = 8, 16
  local tile = (w - 2 * inset - gap * (count - 1)) / count
  for n = 0, count - 1 do
    local key = "row" .. n
    local title = i[key .. "-title"]
    if title then
      local left = inset + n * (tile + gap)
      widget.icon_button({
        id = key, label = title, icon = "window", icon_size = 64, icon_caption = true,
        action = "select", body = { origin = "desktop-launcher" },
        x = p.x(left), y = p.y(16), width = p.w(tile), height = p.h(h - 32),
        enabled = i[key .. "-available"] == true,
      })
    end
  end
  return
end

local scope = i.scope or "all"
local heading, hint
if scope == "workspaces" then
  heading, hint = "Workspaces", "Search workspaces"
elseif scope == "native" then
  heading, hint = "Native apps", "Search native apps"
elseif scope == "canvas" then
  heading, hint = "Canvas apps", "Search Canvas apps"
else
  heading, hint = "Applications", "Search apps, windows, workspaces"
end

widget.label({ id = "heading", text = heading, x = p.x(24), y = p.y(18), size = math.min(t.title, 30), ink = "text" })
if i.target and i.target ~= "" then
  -- Right-aligned by measurement: "This screen" and "Display 2 (portrait)" are not
  -- the same width, and a fixed offset made one of them collide with the heading.
  local label = tostring(i.target)
  local width = math.min(#label, 22) * (k.type.small * 0.6)
  widget.label({
    id = "target", text = label:sub(1, 22), x = p.x(w - 24 - width), y = p.y(26),
    size = k.type.small, ink = "muted",
  })
end

-- The field is the one place the eye should land: it carries the caret, it is the
-- widest element on the sheet, and its fill is the only recessed surface here.
widget.panel({
  id = "search-bg", space = p, x = 20, y = 58, width = w - 40, height = 40,
  ink = "muted", opacity = 0.10, radius = k.radius.md,
})
local query = i.query or ""
local field = query == "" and hint or (query .. "▏")
widget.label({
  id = "search", text = field, x = p.x(34), y = p.y(58 + (40 - k.type.body) * 0.5),
  size = k.type.body, ink = query == "" and "muted" or "text",
})

-- A row is a hit target with a mark; the fill behind a selected row is what makes
-- arrow keys visible at all, and it is the only thing on this sheet that moves
-- without typing.
local row_height = 44
local mark = 26
local top = 112
local function tint(value)
  if type(value) ~= "string" then return nil end
  return value:match("^#%x%x%x%x%x%x$") and value or nil
end

local rows = 0
for n = 0, 5 do
  local key = "row" .. n
  local title = i[key .. "-title"]
  if title then
    rows = rows + 1
    local y = top + (n) * row_height
    local fill = tint(i[key .. "-tint"])
    local selected = i[key .. "-selected"] == true
    if selected then
      widget.panel({
        id = key .. "-selection", space = p, x = 16, y = y + 2, width = w - 32,
        height = row_height - 4, ink = "text", opacity = 0.10, radius = k.radius.md,
        transition_ms = k.motion.fast,
      })
      -- A two-pixel bar on the leading edge: on a row with a mark, the fill alone
      -- reads as "this one is different", not as "this one is chosen".
      widget.panel({
        id = key .. "-accent", space = p, x = 16, y = y + 8, width = 2,
        height = row_height - 16, ink = "primary", opacity = 0.95, radius = 1,
        transition_ms = k.motion.fast,
      })
    end
    -- The mark is a rounded tile in the app's own tint, with its monogram punched
    -- out of it. A row without a tint still gets a tile, so the column reads as a
    -- column of apps rather than as text that happens to start in two places.
    widget.tile({
      id = key .. "-mark", space = p, x = 24, y = y + (row_height - mark) * 0.5,
      size = mark, radius = mark * 0.32,
      text = i[key .. "-badge"] or "",
      size_text = k.type.small, rgb = fill, ink = "muted", opacity = fill and 0.92 or 0.16,
      text_ink = fill and "text" or "muted", z = 0.9,
    })
    -- The title is drawn here, and the hit target is invisible above it. Letting
    -- the host draw the text meant the row's alignment was the host's business:
    -- a selected row centred its own label while its neighbours sat left, because
    -- the two styles disagree about what a label inside a full-width box means.
    -- A row is a hit target with text on it, not a button that happens to be wide.
    -- The title gets the room the detail does not need: a row that overlaps its
    -- own metadata is worse than a truncated name, because you cannot read either.
    local detail = tostring(i[key .. "-detail"] or "")
    -- The metadata is capped at what is worth reading: reserving room for a
    -- 34-character detail cut every title in half to make space for a suffix
    -- nobody finishes.
    local shown_detail = detail:sub(1, 22)
    local detail_room = shown_detail == "" and 0 or #shown_detail * (k.type.micro * 0.58) + 22
    local title_room = w - 66 - detail_room - 16
    local title_chars = math.max(8, math.floor(title_room / (k.type.body * 0.52)))
    local title_text = tostring(title)
    if #title_text > title_chars then title_text = title_text:sub(1, title_chars - 1) .. "…" end
    widget.label({
      id = key .. "-title", text = title_text,
      x = p.x(66), y = p.y(y + (row_height - k.type.body) * 0.5),
      size = k.type.body, ink = selected and "text" or "text", z = 1,
    })
    widget.button({
      id = key, label = tostring(title):sub(1, 24),
      action = "select", body = { origin = "desktop-launcher" },
      appearance = "quiet",
      x = p.x(8), y = p.y(y + 2), width = p.w(w - 16), height = p.h(row_height - 4),
      enabled = i[key .. "-available"] == true,
    })
    if detail ~= "" then
      detail = shown_detail
      widget.label({
        id = key .. "-detail", text = detail,
        x = p.x(w - 28 - #detail * (k.type.micro * 0.6)), y = p.y(y + 16),
        size = k.type.micro, ink = "muted",
      })
    end
  end
end

if (i.count or 0) == 0 then
  widget.label({
    id = "empty", text = i.live and "No matches" or "Connecting…",
    x = p.x(28), y = p.y(top + 12), size = k.type.body, ink = "muted",
  })
end

-- The footer is an instruction, not decoration. Two chords, then the scope switch.
widget.label({
  id = "keys", text = "↑↓ choose · ⏎ open · esc close",
  x = p.x(24), y = p.y(h - 30), size = k.type.micro, ink = "muted",
})
if scope ~= "workspaces" then
  local other = scope == "native" and "canvas-apps" or "native-apps"
  widget.button({
    id = other, label = scope == "native" and "Canvas apps · F2" or "Native apps · F2",
    appearance = "quiet", action = "select", body = { origin = "desktop-launcher" },
    x = p.x(w - 168), y = p.y(h - 40), width = p.w(144), height = p.h(28),
    enabled = i.live == true,
  })
end
if not i.live then
  widget.label({
    id = "health", text = i.health or "Connecting…",
    x = p.x(24), y = p.y(h - 48), size = k.type.micro, ink = "warning",
  })
end
