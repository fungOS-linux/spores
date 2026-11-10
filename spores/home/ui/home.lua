-- The fungOS home surface.
--
-- A launcher draws; a host decides. Every tile carries an id and a host to
-- open, never a path, and the one intent this view emits is `home.open`.
--
-- The snapshot is host-owned: probing, the store catalog, memory and disk are
-- facts a sandbox cannot read. The fixture below is the shape the host hands
-- over, so the live module swaps in without moving a pixel.

local ui = require("fungos.ui")

local M = { name = "dev.fungos.home", version = "0.1.0" }

local DEFAULT_THEME = {
  title = 30, large = 22, body = 15, small = 12, micro = 10, radius = 10,
}

local function theme()
  return (_G.module and _G.module.theme) or DEFAULT_THEME
end

local function screen_height()
  local inputs = _G.module and _G.module.inputs
  local value = (inputs and inputs.height) or 1600
  return math.max(800, math.floor(value / 4) * 4)
end

local function slug(value)
  return (tostring(value):gsub("[^%w%-_]", "-"):sub(1, 48))
end

local function pad_to(list, count)
  while #list < count do list[#list + 1] = ui.spacer({}) end
  return list
end

local function equal(count)
  local widths = {}
  for index = 1, count do widths[index] = 1 / count end
  return widths
end

-- ---- fixture -----------------------------------------------------------
--
-- A real machine: fourteen services, sixteen Spores, one down, a store that is
-- current. Density is the point — a launcher tested on four apps is not tested.

local function fixture()
  local apps = {
    { id = "dev.fungos.chrome", name = "chrome", glyph = "chrome", host = "home.myc",
      category = "system", color = "#8b9dff", state = "installed" },
    { id = "dev.fungos.consent", name = "consent", glyph = "consent", host = "consent.myc",
      category = "system", color = "#7ad0a3", state = "installed" },
    { id = "dev.fungos.contacts", name = "contacts", glyph = "contacts", host = "contacts.myc",
      category = "communication", color = "#f0a35e", state = "installed" },
    { id = "dev.fungos.crm", name = "crm", glyph = "crm", host = "crm.myc",
      category = "productivity", color = "#f2c14e", state = "installed" },
    { id = "dev.fungos.dfm", name = "dfm-report", glyph = "dfm", host = "dfm.myc",
      category = "science", color = "#f08a8a", state = "installed" },
    { id = "dev.fungos.gallery", name = "gallery", glyph = "gallery", host = "gallery.myc",
      category = "developer-tools", color = "#8b9dff", state = "installed" },
    { id = "dev.fungos.git", name = "git", glyph = "git", host = "git.myc",
      category = "developer-tools", color = "#f0a35e", state = "installed" },
    { id = "dev.fungos.inbox", name = "inbox", glyph = "inbox", host = "inbox.myc",
      category = "communication", color = "#6ea8fe", state = "installed" },
    { id = "dev.fungos.media-probe", name = "media-probe", glyph = "media-probe", host = "media.myc",
      category = "developer-tools", color = "#8bd5ca", state = "installed" },
    { id = "dev.fungos.mush", name = "mush", glyph = "mush", host = "mush.myc",
      category = "developer-tools", color = "#c9a6ff", state = "installed" },
    { id = "dev.fungos.notify", name = "notify", glyph = "notify", host = "notify.myc",
      category = "system", color = "#f0a35e", state = "installed" },
    { id = "dev.fungos.spore-browser", name = "spore-browser", glyph = "spore-browser", host = "spores.myc",
      category = "system", color = "#c9a6ff", state = "installed" },
    { id = "dev.fungos.store", name = "store", glyph = "store", host = "store.myc",
      category = "system", color = "#f2c14e", state = "installed" },
    { id = "dev.fungos.surface-probe", name = "surface-probe", glyph = "surface-probe", host = "surface.myc",
      category = "developer-tools", color = "#8b9dff", state = "installed" },
    { id = "dev.fungos.terminal", name = "terminal", glyph = "terminal", host = "term.myc",
      category = "developer-tools", color = "#8bd5ca", state = "installed" },
    { id = "dev.fungos.worklog", name = "worklog", glyph = "worklog", host = "worklog.myc",
      category = "developer-tools", color = "#8fbf7f", state = "installed" },
  }
  local services = {
    { name = "ai", status = "up", ms = 1, host = "ai.myc" },
    { name = "mcp", status = "up", ms = 4, host = "mcp.myc" },
    { name = "webhook", status = "down", ms = nil, host = "webhook.myc" },
    { name = "manavault", status = "up", ms = 23, host = "manavault.myc" },
    { name = "confluence", status = "up", ms = 3, host = "confluence.myc" },
  }
  return {
    host = "averys-mac-studio",
    wallpaper = "undergrowth",
    apps = apps,
    services = services,
    dock = { "dev.fungos.store", "dev.fungos.terminal", "dev.fungos.chrome" },
    widgets = {
      today = { time = "07:22", date = "Monday, November 9", day = "day 313 of 2026" },
      machine = { up = 12, total = 14, median = "1 ms median", down = "down: webhook, sfx" },
      store = {
        spores = 16, source = "fungos · no signed index", current = 16, yanked = 0,
        classes = {}, updates = {},
      },
      system = { load = "363 · 370 · 381", memory = "6 / 32 GB", disk = "12 GB", battery = nil },
    },
  }
end

-- ---- pieces ------------------------------------------------------------

local function label(text, style, ink)
  return ui.text(tostring(text or ""), { style = style or "caption", ink = ink or "muted" })
end

local function bar(view)
  local today = view.widgets.today or {}
  return ui.row({
    id = "home-bar",
    widths = { 0.4, 0.2, 0.4 },
    gap = "sm",
    children = {
      ui.row({ gap = "sm", children = {
        ui.text(today.time or "", { style = "heading", ink = "text" }),
        label(today.date),
      } }),
      label(view.host),
      ui.row({ gap = "sm", align = "end", children = {
        label("Search apps"),
        label("⌕", "heading", "muted"),
      } }),
    },
  })
end

local function widget(title, children)
  local kept = { label(string.upper(title), "subheading", "muted") }
  for _, child in ipairs(children or {}) do
    if child ~= nil then kept[#kept + 1] = child end
  end
  return { ui.card({ pad = "sm", gap = "xs", children = kept }) }
end

local function facts(rows)
  local out = {}
  for _, row in ipairs(rows or {}) do
    if row and row[2] and row[2] ~= "" then
      out[#out + 1] = ui.row({
        widths = { 0.45, 0.55 },
        gap = "sm",
        children = { label(row[1]), ui.text(tostring(row[2]), { style = "caption", ink = "text" }) },
      })
    end
  end
  return out
end

local function widgets(view)
  local widgets = view.widgets or {}
  local today = widgets.today or {}
  local machine = widgets.machine or {}
  local store = widgets.store or {}
  local system = widgets.system or {}

  local cards = {}
  for _, card in ipairs(widget("Today", {
    ui.text(today.time or "", { style = "title", ink = "text" }),
    label(today.date),
    label(today.day),
  })) do cards[#cards + 1] = card end

  for _, card in ipairs(widget("This machine", {
    ui.text(string.format("%d / %d up", machine.up or 0, machine.total or 0), { style = "title", ink = "text" }),
    label(machine.median),
    label(machine.down),
  })) do cards[#cards + 1] = card end

  local store_children = {
    ui.text(string.format("%d Spores", store.spores or 0), { style = "title", ink = "text" }),
    label(store.source),
    label(string.format("%d current · %d yanked", store.current or 0, store.yanked or 0)),
  }
  local classes = {}
  for _, class in ipairs(store.classes or {}) do
    classes[#classes + 1] = label(string.format("%d %s", class.count, class.name), "caption", class.ink)
  end
  if #classes == 0 then classes[1] = label("every Spore is current") end
  for _, line in ipairs(classes) do store_children[#store_children + 1] = line end
  for _, card in ipairs(widget("Store", store_children)) do cards[#cards + 1] = card end

  for _, card in ipairs(widget("System", facts({
    { "load", system.load },
    { "memory", system.memory },
    { "disk", system.disk },
    { "battery", system.battery },
  }))) do cards[#cards + 1] = card end

  return ui.row({ id = "home-widgets", widths = equal(4), gap = "sm", children = cards })
end

local function tile(app)
  return ui.column({
    id = "home-app-" .. slug(app.id),
    gap = "xs",
    children = {
      ui.icon_button(app.glyph or "grid", {
        id = "home-open-" .. slug(app.id),
        action = ui.action("home-open", { id = app.id, host = app.host or "" }),
      }),
      ui.text(app.name or app.id, { style = "caption", ink = "text" }),
      label(app.category or "app", "subheading", "muted"),
    },
  })
end

local function grid(view)
  local rows = {}
  local row = {}
  for _, app in ipairs(view.apps or {}) do
    row[#row + 1] = tile(app)
    if #row == 6 then
      rows[#rows + 1] = ui.row({ widths = equal(6), gap = "sm", children = row })
      row = {}
    end
  end
  if #row > 0 then
    rows[#rows + 1] = ui.row({ widths = equal(6), gap = "sm", children = pad_to(row, 6) })
  end
  return ui.column({ id = "home-grid", gap = "md", children = rows })
end

local function services(view)
  local rows = {}
  for _, service in ipairs(view.services or {}) do
    local ink = service.status == "up" and "success"
      or (service.status == "down" and "danger" or "info")
    local meta = service.status == "up" and string.format("%d ms", service.ms or 0)
      or (service.status == "down" and "no answer" or service.status)
    rows[#rows + 1] = ui.row({
      widths = { 0.12, 0.56, 0.32 },
      gap = "sm",
      children = {
        ui.text("●", { style = "caption", ink = ink }),
        ui.text(service.name or "", { style = "caption", ink = "text" }),
        label(meta),
      },
    })
  end
  return ui.card({
    id = "home-services",
    pad = "sm",
    gap = "xs",
    children = { label("SERVICES", "subheading", "muted"), table.unpack(rows) },
  })
end

local function dock(view)
  local by_id = {}
  for _, app in ipairs(view.apps or {}) do by_id[app.id] = app end
  local children = {}
  for _, id in ipairs(view.dock or {}) do
    local app = by_id[id]
    if app then
      children[#children + 1] = ui.icon_button(app.glyph or "grid", {
        id = "home-dock-" .. slug(app.id),
        action = ui.action("home-open", { id = app.id, host = app.host or "" }),
      })
    end
  end
  if #children == 0 then return nil end
  return ui.row({ id = "home-dock", gap = "md", children = children })
end

-- ---- entrypoint --------------------------------------------------------

function M.render(view)
  view = view or {}
  if not view.apps then
    local base = fixture()
    for key, value in pairs(view) do base[key] = value end
    view = base
  end
  view.widgets = view.widgets or {}
  view.widgets.today = view.widgets.today or {}
  view.widgets.machine = view.widgets.machine or {}
  view.widgets.store = view.widgets.store or {}
  view.widgets.system = view.widgets.system or {}

  local children = {
    bar(view),
    widgets(view),
    ui.row({
      id = "home-body",
      widths = { 0.78, 0.22 },
      gap = "md",
      children = { grid(view), services(view) },
    }),
  }
  local docked = dock(view)
  if docked then children[#children + 1] = docked end

  ui.present(ui.column({
    id = "home-root",
    h = screen_height(),
    pad = "md",
    gap = "md",
    children = children,
  }), theme())
end

M.fixture = fixture
return M
