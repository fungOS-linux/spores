-- Contacts: compact single-column panel for a narrow Canvas window.
--
-- Canvas has no viewport size in the snapshot, so this view is authored for the
-- current 397x1483 contacts-dev placement. Layout is host-measured; hit targets
-- route contact-N back through the module activation path.

local M = {}
M.name = "contacts"
M.version = "0.2.0"

local W, H = 397, 1483
local Z = 0

local function use_viewport()
  local vp = (_G.module and _G.module.viewport)
    or (_G.module and _G.module.theme and _G.module.theme.viewport)
    or nil
  if vp and vp.width and vp.height and vp.width > 0 and vp.height > 0 then
    W, H = vp.width, vp.height
  end
end

local function theme() return (_G.module and _G.module.theme) or { body = 16 } end
local function base() return (theme().body or 16) end
local function nx(px) return -4 + 8 * px / W end
local function ny(px) return 4 - 8 * px / H end
local function sw(px) return 8 * px / W end
local function sh(px) return 8 * px / H end
local function ts(px) return px / ((base() or 16) * 1.25) end

local function rect(id, x, y, w, h, ink, opacity, radius)
  canvas.rect {
    id = id, ink = ink or "surface", opacity = opacity or 1.0,
    radius = radius or 10,
    translation = { nx(x), ny(y), Z },
    scale = { sw(math.max(1, w)), sh(math.max(1, h)), 1 },
  }
end

local function text(id, value, x, y, px, ink, z)
  canvas.text {
    id = id, text = tostring(value or ""), ink = ink or "text",
    translation = { nx(x), ny(y), z or (Z + 0.1) },
    scale = { ts(px), ts(px), 1 },
  }
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

local function label(id, value, size, layout)
  return {
    id = id, kind = "text", text = tostring(value or ""),
    style = { size = size, line_height = size + 3, wrap = false },
    layout = layout or {},
  }
end

local function contact_row(contact, index)
  local initials = (contact.name or "?"):sub(1, 1):upper()
  return {
    id = "contact-" .. index,
    kind = "container",
    layout = { height = 58, direction = "row", gap = 10, padding = { top = 7, right = 12, bottom = 7, left = 12 } },
    children = {
      { id = "avatar-" .. index, kind = "container", layout = { width = 34, height = 34, align_items = "center", justify_content = "center" },
        children = { label("avatar-text-" .. index, initials, 15) } },
      { id = "contact-" .. index .. "-body", kind = "container", layout = { flex_grow = 1, direction = "column", gap = 1 },
        children = {
          label("contact-" .. index .. "-name", contact.name, 15),
          label("contact-" .. index .. "-org", contact.org ~= "" and contact.org or (contact.title ~= "" and contact.title or "—"), 11),
        } },
    },
  }
end

local function detail_nodes(selected, wide)
  if not selected then return { label("contacts-detail-empty", "Select a contact", 13, { height = 24 }) } end
  local nodes = {}
  local initials = (selected.name or "?"):sub(1, 1):upper()
  nodes[1] = {
    id = "contacts-detail-head", kind = "container",
    layout = wide and { direction = "row", gap = 14, height = 96 }
      or { direction = "column", gap = 2 },
    children = wide and {
      { id = "detail-avatar", kind = "container", layout = { width = 76, height = 76, align_items = "center", justify_content = "center" },
        children = { label("detail-avatar-text", initials, 28) } },
      { id = "contacts-detail-meta", kind = "container", layout = { flex_grow = 1, direction = "column", gap = 3 }, children = {
        label("contacts-detail-name", selected.name, 26, { height = 32 }),
        label("contacts-detail-role", (((selected.org or "") .. ((selected.org or "") ~= "" and selected.title ~= "" and "  ·  " or "") .. (selected.title or "")) ~= "" and ((selected.org or "") .. ((selected.org or "") ~= "" and selected.title ~= "" and "  ·  " or "") .. (selected.title or ""))) or "—", 12, { height = 18 }),
      } },
    } or {
      label("contacts-detail-name", selected.name, 20, { height = 28 }),
      label("contacts-detail-role", (((selected.org or "") .. ((selected.org or "") ~= "" and selected.title ~= "" and "  ·  " or "") .. (selected.title or "")) ~= "" and ((selected.org or "") .. ((selected.org or "") ~= "" and selected.title ~= "" and "  ·  " or "") .. (selected.title or ""))) or "—", 11, { height = 17 }),
    },
  }
  local row = 1
  local function line(value, ink, height)
    row = row + 1
    nodes[row] = label("contacts-detail-" .. row, value, wide and 13 or 11, { height = height or (wide and 24 or 19) })
  end
  line("CONTACT", "muted", 20)
  for _, value in ipairs(selected.emails or {}) do line("EMAIL  ·  " .. value) end
  for _, value in ipairs(selected.phones or {}) do line("PHONE  ·  " .. value) end
  for _, value in ipairs(selected.addresses or {}) do line("ADDRESS  ·  " .. value) end
  if selected.birthday ~= "" then line("BIRTHDAY  ·  " .. selected.birthday) end
  if selected.note ~= "" then line(selected.note, "text", wide and 90 or 70) end
  return nodes
end

local function layout_tree(snapshot)
  local selected = selected_contact(snapshot)
  local wide = W >= 700
  local visible = math.min(#(snapshot.contacts or {}), wide and math.max(4, math.floor((H - 190) / 58)) or 14)
  local header = {
    { id = "contacts-head", kind = "container", layout = { direction = "row", height = wide and 50 or 46, gap = 8 },
      children = {
        label("contacts-title", "Contacts", wide and 30 or 26, { flex_grow = 1 }),
        label("contacts-count", tostring(snapshot.total or #(snapshot.contacts or {})), 13),
      } },
    label("contacts-subtitle", (snapshot.account and snapshot.account.label or "Records") .. "  ·  " .. (snapshot.account and snapshot.account.sync or ""), 11, { height = 18 }),
    { id = "contacts-search", kind = "container", layout = { height = wide and 42 or 38, padding = { left = 12, right = 10, top = 11 } },
      children = { label("contacts-search-text", "⌕  Search people, org, email", 11) } },
  }
  local rows = {}
  for index = 1, visible do rows[#rows + 1] = contact_row(snapshot.contacts[index], index) end
  local list = {
    label("contacts-section", string.format("PEOPLE  ·  %d", snapshot.total or #(snapshot.contacts or {})), 10, { height = 18 }),
    { id = "contacts-list", kind = "container", layout = { direction = "column", gap = 0 }, children = rows },
  }
  if wide then
    local left_width = math.min(360, math.max(280, W * 0.31))
    return {
      id = "contacts-root", kind = "container",
      layout = { width = W, height = H, direction = "row", gap = 22, padding = { top = 22, right = 22, bottom = 18, left = 22 } },
      children = {
        { id = "contacts-sidebar", kind = "container", layout = { width = left_width, direction = "column", gap = 10 },
          children = { { id = "contacts-sidebar-head", kind = "container", layout = { direction = "column", gap = 4 }, children = header }, table.unpack(list) } },
        { id = "contacts-detail", kind = "container", layout = { flex_grow = 1, direction = "column", gap = 8, padding = { top = 12, right = 20, bottom = 16, left = 20 } },
          children = detail_nodes(selected, true) },
      },
    }
  end
  return {
    id = "contacts-root", kind = "container",
    layout = { width = W, height = H, direction = "column", gap = 10, padding = { top = 22, right = 18, bottom = 18, left = 18 } },
    children = {
      { id = "contacts-header", kind = "container", layout = { direction = "column", gap = 4 }, children = header },
      { id = "contacts-detail", kind = "container", layout = { direction = "column", gap = 2, padding = { top = 10, right = 12, bottom = 10, left = 12 } }, children = detail_nodes(selected, false) },
      table.unpack(list),
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
  use_viewport()
  snapshot = apply_event(snapshot or {})
  local laid = canvas.layout(layout_tree(snapshot), W, H)

  rect("contacts-bg", 0, 0, W, H, "background", 1.0, 0)
  local search = find(laid, "contacts-search")
  if search then rect("contacts-search-bg", search.rect.x, search.rect.y, search.rect.width, search.rect.height, "surface", 0.8, 9) end
  local sidebar = find(laid, "contacts-sidebar")
  if sidebar then rect("contacts-sidebar-bg", sidebar.rect.x - 12, sidebar.rect.y - 10, sidebar.rect.width + 24, sidebar.rect.height + 20, "surface", 0.42, 12) end
  local detail = find(laid, "contacts-detail")
  if detail then rect("contacts-detail-bg", detail.rect.x - 18, detail.rect.y - 12, detail.rect.width + 36, detail.rect.height + 24, "surface", 0.55, 12) end

  local _, selected_index = selected_contact(snapshot)
  local selected_row = find(laid, "contact-" .. tostring(selected_index))
  if selected_row then
    rect("contacts-selected", selected_row.rect.x - 6, selected_row.rect.y, selected_row.rect.width + 12, selected_row.rect.height, "primary", 0.16, 10)
  end

  for _, node in ipairs(laid.nodes or {}) do
    local r = node.rect
    if node.kind == "text" and node.style then
      local ink = "text"
      if node.id:find("subtitle") or node.id:find("section") or node.id:find("role") or node.id:find("search%-text") or node.id:find("%-org$") then ink = "muted" end
      if node.id == "contacts-title" or node.id == "contacts-detail-name" or node.id:find("avatar%-text") then ink = "primary" end
      if node.id == "contacts-detail-role" or node.id:find("contacts%-detail%-2$") then ink = "muted" end
      text(node.id, node.text, r.x, r.y, node.style.size, ink)
    end
  end

  local detail_avatar = find(laid, "detail-avatar")
  if detail_avatar then rect("detail-avatar-bg", detail_avatar.rect.x, detail_avatar.rect.y, detail_avatar.rect.width, detail_avatar.rect.height, "primary", 0.14, 38) end
  for index = 1, math.min(#(snapshot.contacts or {}), 32) do
    local row = find(laid, "contact-" .. index)
    if row then
      local avatar = find(laid, "avatar-" .. index)
      if avatar then rect("avatar-bg-" .. index, avatar.rect.x, avatar.rect.y, avatar.rect.width, avatar.rect.height, "surface", 0.95, 17) end
      widget.hit { id = "contact-" .. index, label = "hit",
        x = nx(row.rect.x), y = ny(row.rect.y), width = sw(row.rect.width), height = sh(row.rect.height) }
    end
  end
end

return M
