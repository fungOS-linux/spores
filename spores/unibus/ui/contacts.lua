-- Records contacts view.
--
-- The Canvas host measures glyphs and lays out the tree; this file owns only
-- structure, copy, and the local selection event.

local M = {}
M.name = "contacts"
M.version = "0.1.0"

local W, H = 1128, 1766
local Z = 0

local function theme()
  return (_G.module and _G.module.theme) or { body = 16 }
end

local function base() return (theme().body or 16) end
local function nx(px) return -4 + 8 * px / W end
local function ny(px) return 4 - 8 * px / H end
local function sw(px) return 8 * px / W end
local function sh(px) return 8 * px / H end
local function ts(px) return px / ((base() or 16) * 1.25) end

-- See docs/interaction-gap.md: `widget.hit` has no host, so calling it crashed
-- the module in Canvas. Use it when present, otherwise keep the spec addressable.
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

-- Canvas has no layout service (see docs/interaction-gap.md). Prefer the host's
-- `canvas.layout` when one provides it, otherwise lay the tree out in Lua.
local function lay(tree, width, height)
  if canvas and canvas.layout then return canvas.layout(tree, width, height) end
  return ui.layout(tree, width, height)
end

-- Canvas rejects ink roles it does not know rather than falling back to body
-- text. A surface the theme does not name has to be spelled with one it does.
local INK = { surface = "muted", background = "muted" }
local function ink(name)
  return INK[name] or name
end

local function rect(id, x, y, w, h, role, opacity, radius)
  canvas.rect {
    id = id, ink = ink(role or "surface"), opacity = opacity or 1.0,
    radius = radius or 12,
    translation = { nx(x), ny(y), Z },
    scale = { sw(math.max(1, w)), sh(math.max(1, h)), 1 },
  }
end

local function text(id, value, x, y, px, role, z)
  canvas.text {
    id = id, text = tostring(value or ""), ink = ink(role or "text"),
    translation = { nx(x), ny(y), z or (Z + 0.1) },
    scale = { ts(px), ts(px), 1 },
  }
end

local function measure(value, px)
  local ok, size = pcall(canvas.measure_text, tostring(value or ""), {
    size = px, line_height = math.max(px + 3, px * 1.25), wrap = false,
  })
  if ok and size then return size.width or 0, size.height or 0 end
  return #tostring(value or "") * px * 0.55, px * 1.25
end

local function find(laid, id)
  for _, node in ipairs(laid.nodes or {}) do
    if node.id == id then return node end
  end
end

local function selected_contact(snapshot)
  for index, contact in ipairs(snapshot.contacts or {}) do
    if contact.id == snapshot.selected_id then return contact, index end
  end
  return (snapshot.contacts or {})[1], 1
end

local function layout_tree(snapshot)
  local b = base()
  local title = math.max(26, b * 1.9)
  local name = math.max(15, b * 0.95)
  local small = math.max(11, b * 0.72)
  local row_h = 62
  local visible = math.min(#(snapshot.contacts or {}), 18)

  local function label(id, value, size, layout)
    return { id = id, kind = "text", text = tostring(value or ""),
      style = { size = size, line_height = size + 5, wrap = false }, layout = layout or {} }
  end

  local function contact_row(contact, index)
    return {
      id = "contact-" .. index,
      kind = "container",
      layout = { height = row_h, padding = { top = 9, right = 10, bottom = 7, left = 10 } },
      children = {
        label("contact-" .. index .. "-name", contact.name, name),
        label("contact-" .. index .. "-org", (contact.org or "") ~= "" and contact.org or ((contact.title or "") ~= "" and contact.title or "—"), small),
      },
    }
  end

  local selected, selected_index = selected_contact(snapshot)
  local left = {
    label("contacts-title", "Contacts", title, { height = title + 10 }),
    label("contacts-subtitle", (snapshot.account and snapshot.account.label or "Records") .. "  ·  " .. (snapshot.account and snapshot.account.sync or ""), small),
    { id = "contacts-search", kind = "container", layout = { height = 44, padding = { left = 12, right = 12, top = 11 } },
      children = { label("contacts-search-text", "Search people, organizations, email", small) } },
    label("contacts-section", string.format("PEOPLE  ·  %d", snapshot.total or #(snapshot.contacts or {})), small, { height = 22 }),
  }
  for index = 1, visible do
    left[#left + 1] = contact_row(snapshot.contacts[index], index)
  end

  local detail_children = {}
  if selected then
    detail_children[1] = label("contacts-detail-name", selected.name, math.max(24, b * 1.7), { height = 42 })
    local role = selected.org or ""
    if (selected.title or "") ~= "" then role = role .. (role ~= "" and "  ·  " or "") .. selected.title end
    detail_children[2] = label("contacts-detail-role", role ~= "" and role or "—", small, { height = 20 })
    detail_children[3] = { id = "contacts-detail-rule", kind = "rule", layout = { height = 1 } }
    detail_children[4] = label("contacts-detail-section", "CONTACT", small, { height = 22 })
    local row = 4
    local function detail(value, ink)
      row = row + 1
      detail_children[row] = label("contacts-detail-" .. row, value, name, { height = 24 })
    end
    for _, value in ipairs(selected.emails or {}) do detail("EMAIL  ·  " .. value, "text") end
    for _, value in ipairs(selected.phones or {}) do detail("PHONE  ·  " .. value, "text") end
    for _, value in ipairs(selected.addresses or {}) do detail("ADDRESS  ·  " .. value, "text") end
    if (selected.birthday or "") ~= "" then detail("BIRTHDAY  ·  " .. selected.birthday, "text") end
    if (selected.note or "") ~= "" then
      row = row + 1
      detail_children[row] = label("contacts-detail-note", selected.note, small, { height = 120 })
    end
  end

  return {
    id = "contacts-root",
    kind = "container",
    layout = { width = W, height = H, direction = "row", gap = 24, padding = { top = 28, right = 28, bottom = 28, left = 28 } },
    children = {
      { id = "contacts-left", kind = "container", layout = { width = 352, direction = "column", gap = 10 }, children = left },
      { id = "contacts-right", kind = "container", layout = { flex_grow = 1, direction = "column", gap = 12 }, children = detail_children },
    },
  }
end

local function apply_event(snapshot)
  local event = _G.module and _G.module.event
  if not event or event.kind ~= "activate" then return snapshot end
  local index = tonumber((event.target_id or ""):match("^contact%-(%d+)$"))
  if index and (snapshot.contacts or {})[index] then
    snapshot.selected_id = snapshot.contacts[index].id
  end
  return snapshot
end

function M.render(snapshot)
  snapshot = apply_event(snapshot or {})
  local laid = lay(layout_tree(snapshot), W, H)
  local b = base()

  rect("contacts-bg", 0, 0, W, H, "background", 1.0, 0)
  rect("contacts-window", 1, 1, W - 2, H - 2, "surface", 1.0, 18)
  rect("contacts-left-bg", 28, 28, 352, H - 56, "surface", 0.55, 14)
  local search = find(laid, "contacts-search")
  if search then rect("contacts-search-bg", search.rect.x, search.rect.y, search.rect.width, search.rect.height, "surface", 1.0, 10) end

  local selected, selected_index = selected_contact(snapshot)
  if selected_index and find(laid, "contact-" .. selected_index) then
    local row = find(laid, "contact-" .. selected_index)
    rect("contacts-selected", row.rect.x, row.rect.y, row.rect.width, row.rect.height, "primary", 0.16, 10)
  end

  for _, node in ipairs(laid.nodes or {}) do
    local r = node.rect
    if node.kind == "rule" then
      rect(node.id .. "-line", r.x, r.y, r.width, math.max(1, r.height), "muted", 0.25, 0)
    elseif node.kind == "text" and node.style then
      local ink = "text"
      if node.id:find("subtitle") or node.id:find("section") or node.id:find("role") or node.id:find("search%-text") then ink = "muted" end
      if node.id:find("contacts%-title") or node.id:find("detail%-name") then ink = "primary" end
      if node.id:find("contacts%-detail%-2$") then ink = "muted" end
      text(node.id, node.text, r.x, r.y, node.style.size, ink)
    end
  end

  for index = 1, math.min(#(snapshot.contacts or {}), 18) do
    local row = find(laid, "contact-" .. index)
    if row then
      hit {
        id = "contact-" .. index,
        label = "hit",
        x = nx(row.rect.x),
        y = ny(row.rect.y),
        width = sw(row.rect.width),
        height = sh(row.rect.height),
      }
    end
  end
end

return M
