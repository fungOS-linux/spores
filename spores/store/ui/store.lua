-- Spore Store: browse, updates, and one listing at a time.
--
-- Fixture-driven first slice. The snapshot below has the shape the service will
-- hand the module (`fungos.store.listing@1`, `...review@1`, `...pending@1`), so
-- the real source swaps in without moving a pixel of the layout.
--
-- This Spore is a composition today: it draws and emits intents. Installing is
-- the host's authority (see docs/spore-store.md in fungOS-linux/soil); a window
-- that could install would be a second authority over what runs on the machine.

local ui = require("fungos.ui")

local M = { name = "dev.fungos.store", version = "0.1.0" }

local DEFAULT_THEME = {
  title = 30, large = 22, body = 15, small = 12, micro = 10, radius = 10,
}

local function theme()
  return (_G.module and _G.module.theme) or DEFAULT_THEME
end

-- A window height is not a token; round to the 4px base the library validates.
local function screen_height()
  local inputs = _G.module and _G.module.inputs
  local value = (inputs and inputs.height) or 1600
  return math.max(800, math.floor(value / 4) * 4)
end

local function slug(value)
  return (tostring(value):gsub("[^%w%-_]", "-"):sub(1, 48))
end

local function stars(score)
  local filled = math.min(5, math.max(0, math.floor((score or 0) + 0.5)))
  return string.rep("★", filled) .. string.rep("☆", 5 - filled)
end

local function short_ref(value)
  value = tostring(value or "")
  if #value <= 26 then return value end
  return value:sub(1, 14) .. "…" .. value:sub(-8)
end

local function equal(count)
  local widths = {}
  for index = 1, count do widths[index] = 1 / count end
  return widths
end

local function rows_of(items, per_row, build)
  local rows, row = {}, {}
  for _, item in ipairs(items or {}) do
    row[#row + 1] = build(item)
    if #row == per_row then
      rows[#rows + 1] = ui.row({ widths = equal(per_row), gap = "sm", children = row })
      row = {}
    end
  end
  if #row > 0 then
    while #row < per_row do row[#row + 1] = ui.spacer({}) end
    rows[#rows + 1] = ui.row({ widths = equal(per_row), gap = "sm", children = row })
  end
  return rows
end

-- How an update applies. The class is the promise the store makes about what a
-- person will have to approve or restart, so it is never inferred in the view.
local CLASS = {
  live = { label = "live", ink = "success", blurb = "view only · applies now" },
  restart = { label = "restart", ink = "info", blurb = "module rebuilt · next launch" },
  consent = { label = "consent", ink = "warning", blurb = "new capability · needs you" },
  system = { label = "system", ink = "danger", blurb = "system surface · needs reboot" },
  hold = { label = "held", ink = "muted", blurb = "pinned · will not apply" },
}

local function class_of(update)
  return CLASS[update.class or "restart"] or CLASS.restart
end

local function rating_text(app)
  if not app.rating then return "no ratings yet" end
  return string.format("%s  %.1f · %d", stars(app.rating), app.rating, app.ratings or 0)
end

local function state_text(app)
  if app.state == "update" then return "update ready" end
  if app.state == "installed" then return "installed" end
  return "not installed"
end

-- ---- fixture -----------------------------------------------------------
--
-- Real listings, so the density is honest: two-line names, one-line
-- publishers, ratings that are not all 4.9, and updates in every class.

local function fixture()
  local apps = {
    {
      id = "dev.fungos.markdown", name = "markdown", publisher = "fungOS",
      category = "productivity", version = "0.1.0", rating = 4.7, ratings = 212,
      badge = "M", color = "#8b9dff", state = "update",
      summary = "Markdown reader/editor with a hot-reloadable Lua UI",
      description = "Read and edit Markdown as one document. Rust owns the model, Lua owns every pixel, so a running editor can be restyled without a rebuild.",
      license = "MIT", homepage = "https://github.com/fungOS-linux/spores",
      screenshots = {
        { caption = "Editing a note with the view hot-reloaded" },
        { caption = "The same document on the web surface" },
        { caption = "Diagram fences drawn by Canvas" },
      },
      grants = {
        { grant = "soil:canvas", reason = "present the reader/editor surface" },
        { grant = "soil:storage", reason = "open and save documents by DocId" },
        { grant = "soil:time", reason = "debounced autosave" },
      },
      histogram = { 148, 42, 12, 6, 4 },
      reviews = {
        { author = "avery", rating = 5, verified = true, age = "2d",
          body = "The editor restyles while I have it open. That stopped being a novelty about a week ago and is now just how it works." },
        { author = "jules", rating = 4, verified = true, age = "1w",
          body = "Fast and quiet. I would like a split view before I move my notes over." },
      },
    },
    {
      id = "dev.fungos.terminal", name = "terminal", publisher = "fungOS",
      category = "developer-tools", version = "0.1.0", rating = 4.8, ratings = 96,
      badge = "T", color = "#8bd5ca", state = "installed",
      summary = "Browse Mycelium targets and open a terminal session",
      description = "A terminal that lists the Mycelium targets this machine may reach and opens a host-mediated session, without ever holding SSH credentials itself.",
      screenshots = {
        { caption = "Targets this machine may reach, with the one in use marked" },
        { caption = "A session in a pane the host owns" },
      },
      license = "MIT",
    },
    {
      id = "dev.fungos.inbox", name = "inbox", publisher = "fungOS",
      category = "communication", version = "0.1.0", rating = 4.4, ratings = 58,
      badge = "I", color = "#6ea8fe", state = "update",
      summary = "One inbox over mail and chat metadata",
      description = "Mail threads and chat conversations in one list. It observes, caches and labels; it never sends.",
      screenshots = {
        { caption = "Mail and chat in one timeline" },
        { caption = "A thread with its participants and labels" },
      },
      license = "MIT",
    },
    {
      id = "dev.fungos.worklog", name = "worklog", publisher = "fungOS",
      category = "developer-tools", version = "0.1.0", rating = 4.2, ratings = 31,
      badge = "W", color = "#8fbf7f", state = "update",
      summary = "Timeline of what happened across Marbles, GitHub and threads",
      description = "One chronological log of work, ordered by when it happened rather than by which tool recorded it.",
      screenshots = {
        { caption = "A day of work, ordered by when it happened" },
        { caption = "One row, with every source that touched it" },
      },
      license = "MIT",
    },
    {
      id = "dev.fungos.mush", name = "mush", publisher = "fungOS",
      category = "developer-tools", version = "0.1.0", rating = 4.6, ratings = 74,
      badge = "µ", color = "#c9a6ff", state = "update",
      summary = "Sandboxed VFS shell with real environment and startup file",
      description = "A shell over one workspace: real PWD, environment and startup file, with installed wasm commands instead of host binaries.",
      license = "MIT",
    },
    {
      id = "dev.fungos.gallery", name = "gallery", publisher = "fungOS",
      category = "graphics", version = "0.1.0", rating = 4.0, ratings = 12,
      badge = "G", color = "#8b9dff", state = "get",
      summary = "Dashboard primitives, rendered",
      description = "A fixture-driven gallery of the shared dashboard nodes, so a component change can be seen before it ships in an app.",
      screenshots = {
        { caption = "The primitive gallery, one card per component" },
      },
      license = "MIT",
    },
    {
      id = "dev.fungos.notify", name = "notify", publisher = "fungOS",
      category = "system", version = "0.1.0", rating = 4.5, ratings = 40,
      badge = "N", color = "#f0a35e", state = "installed",
      summary = "Notification tray, focus mode and shared ringer",
      description = "The tray for everything that wants attention: notifications, focus profiles, and calls through one ringer.",
      screenshots = {
        { caption = "The tray, grouped by what is asking for attention" },
      },
      license = "Apache-2.0 OR MIT",
    },
  }

  local updates = {
    { id = "dev.fungos.markdown", name = "markdown", from = "0.1.0", to = "0.2.0", class = "restart",
      reason = "module rebuilt; opens on the new version at next launch" },
    { id = "dev.fungos.inbox", name = "inbox", from = "0.1.0", to = "0.1.1", class = "live",
      reason = "view only; hot-reloads in the open window" },
    { id = "dev.fungos.worklog", name = "worklog", from = "0.1.0", to = "0.1.1", class = "system",
      reason = "system surface digest changes; applies after the next reboot" },
    { id = "dev.fungos.notify", name = "notify", from = "0.1.0", to = "0.2.0", class = "consent",
      reason = "asks for the ringer grant it did not have" },
    { id = "dev.fungos.mush", name = "mush", from = "0.1.0", to = "0.2.0", class = "hold",
      reason = "pinned by fleet policy certified-2026-11" },
  }

  return {
    route = "browse",
    source = { name = "fungOS", index = "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi", verified = true },
    query = "",
    category = "all",
    categories = {
      { id = "all", label = "All Spores", count = #apps, selected = true },
      { id = "updates", label = "Updates", count = #updates },
      { id = "installed", label = "Installed", count = 6 },
      { id = "communication", label = "Communication", count = 1 },
      { id = "developer-tools", label = "Developer Tools", count = 3 },
      { id = "education", label = "Education", count = 0 },
      { id = "games", label = "Games", count = 0 },
      { id = "graphics", label = "Graphics", count = 1 },
      { id = "media", label = "Media", count = 0 },
      { id = "network", label = "Network", count = 0 },
      { id = "productivity", label = "Productivity", count = 1 },
      { id = "science", label = "Science", count = 0 },
      { id = "security", label = "Security", count = 0 },
      { id = "system", label = "System", count = 1 },
      { id = "utilities", label = "Utilities", count = 0 },
    },
    apps = apps,
    featured = {
      "dev.fungos.markdown", "dev.fungos.terminal",
      "dev.fungos.inbox", "dev.fungos.worklog",
    },
    updates = updates,
    pending = { total = #updates, consent = 1, reboot = 1, hold = 1 },
  }
end

local function find_app(view, id)
  for _, app in ipairs(view.apps or {}) do
    if app.id == id then return app end
  end
  return nil
end

local function find_update(view, id)
  for _, update in ipairs(view.updates or {}) do
    if update.id == id then return update end
  end
  return nil
end

-- ---- shared pieces -----------------------------------------------------

local function mark(app, size, suffix)
  local name = slug(app.id) .. (suffix and ("-" .. suffix) or "")
  local spore = app.name or tostring(app.id):match("([^.]+)$") or "?"
  local glyph = ui.glyph[spore] or app.badge or "?"
  return ui.card({
    id = "store-mark-" .. name,
    w = size,
    h = size,
    pad = "sm",
    ink = "primary",
    opacity = 0.16,
    children = { ui.text(glyph, {
      id = "store-mark-text-" .. name,
      style = size >= 80 and "display" or "title",
      ink = "primary",
    }) },
  })
end

local function px(value)
  return math.max(4, math.floor(value / 4) * 4)
end

-- A screenshot is a CID reference, not bytes in the package. The store draws a
-- framed window in its place until the host resolves the listing's asset; that is
-- a presentation choice, not a second image pipeline. It is shaped like the thing
-- it stands in for — title dots, a rail, content lines — so a card reads as an
-- app with a surface rather than as a row of grey bars.
local function shot_art(id, height, accent)
  local line = math.max(8, px(height * 0.07))
  local body = px(height * 0.72)
  local ink = accent and "primary" or "muted"
  local tint = accent and 0.3 or 0.12
  local children = {}
  -- A title bar only when there is room for one; at 72px the dots would be the
  -- whole picture.
  if height >= 96 then
    children[#children + 1] = ui.row({ id = id .. "-dots", gap = "xs",
      widths = { 0.04, 0.04, 0.04 }, children = {
        ui.fill({ ink = ink, opacity = tint, h = line }),
        ui.fill({ ink = ink, opacity = tint, h = line }),
        ui.fill({ ink = ink, opacity = tint, h = line }),
      } })
  end
  children[#children + 1] = ui.row({ id = id .. "-body", widths = { 0.26, 0.74 }, gap = "xs", children = {
    ui.fill({ ink = ink, opacity = tint * 0.55, h = body }),
    ui.column({ id = id .. "-lines", gap = "xs", children = {
      ui.fill({ ink = ink, opacity = tint * 0.8, h = px(body * 0.5) }),
      ui.fill({ ink = "muted", opacity = 0.2, h = line }),
      ui.fill({ ink = "muted", opacity = 0.16, h = line }),
      ui.fill({ ink = "muted", opacity = 0.12, h = line }),
    } }),
  } })
  return ui.card({ id = id, h = height, pad = "xs", gap = "xs", children = children })
end

local function preview(shot, id, height)
  local caption = shot and shot.caption or "Screenshots arrive with the signed listing"
  return ui.column({ id = id, gap = "xs", children = {
    shot_art(id .. "-art", height, shot ~= nil),
    ui.text(caption, { id = id .. "-caption", style = "caption",
      ink = shot and "text" or "muted" }),
    shot and shot.cid and ui.text("screenshot · " .. short_ref(shot.cid), {
      id = id .. "-cid", style = "caption", ink = "muted",
    }) or nil,
  } })
end

local function rating_line(app, style, suffix)
  if not app.rating then
    return ui.text("New · no ratings yet", { style = "caption", ink = "muted" })
  end
  local name = slug(app.id) .. (suffix and ("-" .. suffix) or "")
  return ui.row({ id = "store-rating-" .. name, widths = { 0.55, 0.45 },
    gap = "sm", children = {
      ui.text(stars(app.rating), { style = style or "body", ink = "primary" }),
      ui.text(string.format("%.1f · %d ratings", app.rating, app.ratings or 0), {
        style = "caption", ink = "muted",
      }),
    } })
end

-- Installing and updating are the work; opening an app already installed is not.
-- So the loud button is the one that changes the machine.
local function app_action(app, suffix)
  local label = app.state == "get" and "Get" or (app.state == "update" and "Update" or "Open")
  return ui.button(label, {
    id = "store-open-" .. slug(app.id) .. "-" .. suffix,
    small = true,
    quiet = app.state == "installed",
    action = ui.action("store-select", { id = app.id }),
  })
end

local function header(view)
  local pending = (view.pending and view.pending.total) or 0
  return ui.column({ id = "store-head", gap = "sm", children = {
    ui.row({ id = "store-head-top", widths = { 0.6, 0.4 }, children = {
      ui.column({ id = "store-head-title", gap = "xs", children = {
        ui.text("Spore Store", { id = "store-title", style = "title", ink = "text" }),
        ui.text(string.format("%s%s · %s", view.source.name,
          view.source.verified and " · verified" or "", short_ref(view.source.index)), {
          id = "store-source-line", style = "caption", ink = "muted",
        }),
      } }),
      ui.row({ id = "store-head-actions", gap = "sm", align = "end", children = {
        ui.button("Sources", { id = "store-sources", small = true, quiet = true,
          action = ui.action("store-source", { name = view.source.name }) }),
        ui.button(pending > 0 and ("Updates · " .. pending) or "Updates", {
          id = "store-updates", small = true, quiet = pending == 0,
          action = ui.action("store-category", { category = "updates" }),
        }),
      } }),
    } }),
    ui.card({ id = "store-search", pad = "sm", children = {
      ui.row({ id = "store-search-row", widths = { 0.06, 0.84, 0.1 }, gap = "sm", children = {
        ui.icon("search", { id = "store-search-glyph", ink = "muted" }),
        ui.text(view.query ~= "" and view.query or "Search the store", {
          id = "store-search-text", style = "body",
          ink = view.query ~= "" and "text" or "muted",
        }),
        ui.kbd("/"),
      } }),
    } }),
  } })
end

local function footer(view)
  local total = #(view.apps or {})
  local pending = view.pending or {}
  local waiting = pending.consent or 0
  return ui.column({ id = "store-foot", gap = "xs", children = {
    ui.rule({ id = "store-foot-rule" }),
    ui.text(string.format("%d Spores · %d updates · %d need your consent · index %s",
      total, pending.total or 0, waiting, short_ref(view.source.index)), {
      id = "store-foot-text", style = "caption", ink = "muted",
    }),
  } })
end

local function sidebar(view)
  local items = {}
  for index, category in ipairs(view.categories or {}) do
    local label = category.count and category.count > 0
      and string.format("%s  ·  %d", category.label, category.count)
      or category.label
    items[#items + 1] = ui.button(label, {
      id = "store-cat-" .. slug(category.id),
      small = true,
      quiet = not category.selected,
      action = ui.action("store-category", { category = category.id }),
    })
  end
  return ui.column({ id = "store-side", gap = "xs", children = items })
end

local function hero(app)
  local shot = (app.screenshots or {})[1]
  return ui.card({
    id = "store-hero",
    pad = "md",
    gap = "md",
    children = {
      ui.row({ id = "store-hero-body", widths = { 0.58, 0.42 }, gap = "md", children = {
        ui.column({ id = "store-hero-copy", gap = "sm", children = {
          ui.row({ id = "store-hero-identity", widths = { 0.17, 0.83 }, gap = "sm", children = {
            mark(app, 96, "hero"),
            ui.column({ gap = "xs", children = {
              ui.text("EDITOR'S CHOICE", { id = "store-hero-kicker", style = "caption", ink = "primary" }),
              ui.text(app.name, { id = "store-hero-name", style = "title", ink = "text" }),
              ui.text(string.format("%s · %s · v%s", app.publisher, app.category, app.version), {
                id = "store-hero-meta", style = "caption", ink = "muted",
              }),
            } }),
          } }),
          rating_line(app, "heading", "hero"),
          ui.text(app.summary, { id = "store-hero-summary", style = "body", ink = "text" }),
          ui.row({ id = "store-hero-actions", gap = "sm", children = {
            app_action(app, "hero"),
            ui.button("See screenshots", {
              id = "store-hero-shots", small = true, quiet = true,
              action = ui.action("store-select", { id = app.id }),
            }),
          } }),
        } }),
        preview(shot, "store-hero-preview", 144),
      } }),
    },
  })
end

-- The picks carry no artwork: at this size a screenshot frame is a grey band, and
-- the hero already shows what a listing's surface looks like. An icon, a rating
-- and one line of what the app does is what a person scans in a row of three.
local function feature_card(app)
  local cardslug = slug(app.id)
  return ui.card({
    id = "store-feature-" .. cardslug,
    pad = "sm",
    gap = "sm",
    children = {
      ui.row({ id = "store-feature-top-" .. cardslug, widths = { 0.22, 0.78 }, gap = "sm", children = {
        mark(app, 56, "feature"),
        ui.column({ gap = "xs", children = {
          ui.text(app.name, { id = "store-feature-name-" .. cardslug, style = "heading", ink = "text" }),
          ui.text(app.publisher .. " · " .. app.category, { style = "caption", ink = "muted" }),
        } }),
      } }),
      rating_line(app, nil, "feature"),
      ui.text(app.summary, { style = "caption", ink = "muted" }),
      app_action(app, "feature"),
    },
  })
end

local function grid_card(app)
  return ui.card({
    id = "store-card-" .. slug(app.id),
    pad = "sm",
    gap = "sm",
    children = {
      ui.row({ id = "store-card-top-" .. slug(app.id), widths = { 0.22, 0.78 }, gap = "sm", children = {
        mark(app, 56, "grid"),
        ui.column({ gap = "xs", children = {
          ui.text(app.name, { id = "store-card-name-" .. slug(app.id), style = "heading", ink = "text" }),
          ui.text(app.publisher .. " · " .. app.category, { style = "caption", ink = "muted" }),
        } }),
      } }),
      rating_line(app, nil, "grid"),
      ui.text(app.summary, { style = "caption", ink = "muted" }),
      app_action(app, "grid"),
    },
  })
end

-- ---- browse ------------------------------------------------------------

local function featured_apps(view)
  local apps = {}
  for _, id in ipairs(view.featured or {}) do
    local app = find_app(view, id)
    if app then apps[#apps + 1] = app end
  end
  if #apps == 0 then
    for index = 1, math.min(4, #(view.apps or {})) do apps[#apps + 1] = view.apps[index] end
  end
  return apps
end

local function browse(view)
  local featured = featured_apps(view)
  local hero_app = featured[1] or (view.apps or {})[1]
  local picks = {}
  for index = 2, math.min(4, #featured) do picks[#picks + 1] = feature_card(featured[index]) end

  local current = "All Spores"
  for _, category in ipairs(view.categories or {}) do
    if category.selected then current = category.label end
  end

  local section = function(title, subtitle, children)
    return ui.column({ gap = "sm", children = {
      ui.row({ widths = { 0.72, 0.28 }, gap = "sm", children = {
        ui.text(title, { style = "heading", ink = "text" }),
        ui.text(subtitle, { style = "caption", ink = "muted" }),
      } }),
      table.unpack(children),
    } })
  end

  local main_children = {
    ui.row({ id = "store-main-head", widths = { 0.72, 0.28 }, gap = "sm", children = {
      ui.column({ gap = "xs", children = {
        ui.text(current == "All Spores" and "Discover" or current, {
          id = "store-main-title", style = "title", ink = "text",
        }),
        ui.text("Signed by the source, installed by digest", { style = "caption", ink = "muted" }),
      } }),
      ui.text(string.format("%d apps", #(view.apps or {})), {
        id = "store-main-count", style = "caption", ink = "muted",
      }),
    } }),
  }
  if hero_app then main_children[#main_children + 1] = hero(hero_app) end
  if #picks > 0 then
    main_children[#main_children + 1] = section("Featured", "Editor's picks this week", {
      ui.row({ id = "store-featured", widths = equal(#picks), gap = "sm", children = picks }),
    })
  end
  local grid_apps = {}
  for index = 1, math.min(6, #(view.apps or {})) do grid_apps[#grid_apps + 1] = view.apps[index] end
  main_children[#main_children + 1] = ui.column({ gap = "sm", children = {
    ui.row({ id = "store-all-head", widths = { 0.72, 0.28 }, gap = "sm", children = {
      ui.column({ gap = "xs", children = {
        ui.text("All Spores", { style = "heading", ink = "text" }),
        ui.text("Every listing is a package digest", { style = "caption", ink = "muted" }),
      } }),
      ui.row({ id = "store-all-action", gap = "sm", align = "end", children = {
        ui.button(string.format("See all %d", #(view.apps or {})), {
          id = "store-see-all", small = true, quiet = true,
          action = ui.action("store-category", { category = "all" }),
        }),
      } }),
    } }),
    ui.column({ id = "store-grid", gap = "sm", children = rows_of(grid_apps, 3, grid_card) }),
  } })

  return ui.row({ id = "store-body", widths = { 0.2, 0.8 }, gap = "md", grow = true, children = {
    sidebar(view),
    ui.column({ id = "store-main", gap = "md", children = main_children }),
  } })
end

-- ---- detail ------------------------------------------------------------

local function screenshots(app)
  local shots = app.screenshots or {}
  if #shots == 0 then return nil end
  local cards = {}
  for index, shot in ipairs(shots) do
    cards[#cards + 1] = ui.card({
      id = "store-shot-" .. slug(app.id) .. "-" .. index,
      pad = "sm",
      gap = "xs",
      h = 240,
      children = {
        shot_art("store-shot-frame-" .. index, 160, true),
        ui.text(shot.caption, { id = "store-shot-caption-" .. index, style = "caption", ink = "text" }),
        ui.text("screenshot · " .. short_ref(shot.cid or "pending pin"), {
          id = "store-shot-cid-" .. index, style = "caption", ink = "muted",
        }),
      },
    })
  end
  return ui.row({ id = "store-shots", widths = { 1 / 3, 1 / 3, 1 / 3 }, gap = "sm", children = cards })
end

local function capability_list(app)
  local lines = {}
  for _, grant in ipairs(app.grants or {}) do
    lines[#lines + 1] = string.format("%s — %s", grant.grant, grant.reason)
  end
  if #lines == 0 then return nil end
  return ui.bullets(lines, { id = "store-grants" })
end

local function histogram(app)
  local counts = app.histogram or {}
  local rows = {}
  -- `histogram` is 5..1 stars in that order, so row 1 (five stars) is
  -- `counts[1]`. Reversing here is how a five-star bucket ends up drawn as
  -- one star with the same number beside it.
  for index = 1, 5 do
    rows[#rows + 1] = { stars = string.rep("★", 6 - index), count = tostring(counts[index] or 0) }
  end
  return ui.table({
    id = "store-histogram",
    title = "Ratings",
    columns = {
      { key = "stars", label = "", width = 1 },
      { key = "count", label = "Reviews", width = 0.6, align = "right" },
    },
    rows = rows,
    row_h = 20,
    h = 220,
  })
end

local function reviews(app)
  local cards = {}
  for index, review in ipairs(app.reviews or {}) do
    cards[#cards + 1] = ui.card({
      id = "store-review-" .. index,
      pad = "sm",
      gap = "xs",
      children = {
        ui.row({ gap = "sm", widths = { 1 / 3, 1 / 3, 1 / 3 }, children = {
          ui.text(review.author .. " · " .. review.age, { style = "body", ink = "text" }),
          ui.text(stars(review.rating), { style = "caption", ink = "primary" }),
          ui.text(review.verified and "verified install" or "unverified", {
            style = "caption", ink = review.verified and "success" or "muted",
          }),
        } }),
        ui.text(review.body, { style = "caption", ink = "text" }),
      },
    })
  end
  return ui.column({ id = "store-reviews", gap = "sm", children = cards })
end

local function detail(view)
  local app = view.selected
  local update = view.selected_update
  local action = "store-install"
  local label = "Get"
  if app.state == "update" then label = "Update"
  elseif app.state == "installed" then label = "Open"; action = "store-open" end

  local meta = string.format("%s · %s · %s", app.publisher, app.category, app.version)
  local children = {
    ui.row({ id = "store-detail-bar", widths = { 0.32, 0.68 }, gap = "sm", children = {
      ui.row({ id = "store-detail-back", gap = "sm", children = {
        ui.button("Back", { id = "store-back", small = true, quiet = true,
          action = ui.action("store-back", {}) }),
      } }),
      ui.row({ id = "store-detail-actions", gap = "sm", align = "end", children = {
        ui.button("Write a review", { id = "store-write-review", small = true, quiet = true,
          action = ui.action("store-review", { id = app.id, digest = app.digest }) }),
        ui.button(label, { id = "store-install",
          action = ui.action(action, { id = app.id, digest = app.digest }) }),
      } }),
    } }),
    ui.row({ id = "store-detail-head", widths = { 0.13, 0.57, 0.3 }, gap = "md", children = {
      mark(app, 120, "detail"),
      ui.column({ id = "store-detail-title", gap = "xs", children = {
        ui.text(app.name, { id = "store-detail-name", style = "title", ink = "text" }),
        ui.text(meta, { id = "store-detail-meta", style = "caption", ink = "muted" }),
        ui.text(rating_text(app), { id = "store-detail-rating", style = "body", ink = "text" }),
      } }),
      ui.column({ id = "store-detail-facts", gap = "xs", children = {
        ui.text(app.license or "", { style = "caption", ink = "muted" }),
        ui.text(app.homepage and short_ref(app.homepage) or "no homepage", {
          style = "caption", ink = "muted",
        }),
        ui.text(update and ("update · " .. update.class) or "up to date", {
          style = "caption", ink = update and class_of(update).ink or "success",
        }),
      } }),
    } }),
    screenshots(app),
    ui.text("About", { style = "heading", ink = "text" }),
    ui.text(app.description or app.summary, { id = "store-detail-description", style = "body", ink = "text" }),
    ui.text("What it can reach", { style = "heading", ink = "text" }),
    capability_list(app),
    ui.row({ id = "store-detail-social", widths = { 0.62, 0.38 }, gap = "md", children = {
      ui.column({ gap = "sm", children = { ui.text("Reviews", { style = "heading", ink = "text" }), reviews(app) } }),
      ui.column({ gap = "sm", children = { histogram(app) } }),
    } }),
  }

  -- Only keep nodes that exist: the library rejects a nil child.
  local kept = {}
  for _, child in ipairs(children) do
    if child ~= nil then kept[#kept + 1] = child end
  end
  return ui.column({ id = "store-detail", gap = "md", children = kept })
end

-- ---- updates -----------------------------------------------------------

local function update_card(update, index)
  local class = class_of(update)
  local pinned = update.class == "hold"
  return ui.card({
    id = "store-update-" .. slug(update.id),
    pad = "sm",
    gap = "xs",
    children = {
      ui.row({ id = "store-update-top-" .. index, widths = { 0.32, 0.3, 0.2, 0.18 }, gap = "sm", children = {
        ui.text(update.name, { style = "heading", ink = "text" }),
        ui.text(string.format("%s → %s", update.from, update.to), {
          style = "caption", ink = "muted",
        }),
        ui.text(class.blurb, { style = "caption", ink = "muted" }),
        ui.text(class.label, { id = "store-update-class-" .. index, style = "caption", ink = class.ink }),
      } }),
      ui.text(update.reason, { style = "caption", ink = "text" }),
      ui.row({ id = "store-update-actions-" .. index, gap = "sm", align = "end", children = {
        ui.button(pinned and "Unpin" or "Update", {
          id = "store-update-apply-" .. index,
          small = true,
          quiet = pinned,
          enabled = not pinned,
          action = ui.action(pinned and "store-unpin" or "store-update", { id = update.id, digest = update.digest }),
        }),
      } }),
    },
  })
end

local function updates(view)
  local cards = {}
  for index, update in ipairs(view.updates or {}) do
    cards[#cards + 1] = update_card(update, index)
  end
  local pending = view.pending or {}
  local banner = nil
  if (pending.consent or 0) + (pending.reboot or 0) > 0 then
    banner = ui.card({ id = "store-pending", pad = "sm", gap = "xs", children = {
      ui.text("Waiting on you", { style = "heading", ink = "warning" }),
      ui.text(string.format(
        "%d waiting on consent, %d waiting on a reboot. Auto-update applies the rest within the rollout window; it never applies these.",
        pending.consent or 0, pending.reboot or 0), {
        id = "store-pending-text", style = "caption", ink = "text",
      }),
    } })
  end

  local children = {
    ui.row({ id = "store-updates-head", widths = { 0.72, 0.28 }, gap = "sm", children = {
      ui.column({ gap = "xs", children = {
        ui.text("Updates", { id = "store-updates-title", style = "title", ink = "text" }),
        ui.text("classified by what it costs to apply", {
          id = "store-updates-subtitle", style = "caption", ink = "muted",
        }),
      } }),
      ui.row({ id = "store-updates-actions", gap = "sm", align = "end", children = {
        ui.button("Update all that can", { id = "store-update-all",
          action = ui.action("store-update-all", {}) }),
      } }),
    } }),
    banner,
    ui.column({ id = "store-updates-list", gap = "sm", children = cards }),
  }
  local kept = {}
  for _, child in ipairs(children) do
    if child ~= nil then kept[#kept + 1] = child end
  end
  return ui.column({ id = "store-updates", gap = "md", children = kept })
end

-- ---- entrypoint --------------------------------------------------------

function M.render(view)
  view = view or {}
  -- A partial snapshot (a route, a selected id) keeps its fields; only the data
  -- it does not carry is filled from the fixture. Replacing the table would
  -- silently drop `route` and draw the browse page for a detail request.
  if not view.apps then
    local base = fixture()
    for key, value in pairs(view) do base[key] = value end
    view = base
  end
  if view.route == "detail" then
    view.selected = view.selected or find_app(view, view.selected_id)
    if not view.selected then
      view.route = "browse"
    elseif not view.selected_update then
      view.selected_update = find_update(view, view.selected.id)
    end
  end

  local body
  if view.route == "detail" then
    body = detail(view)
  elseif view.route == "updates" then
    body = updates(view)
  else
    body = browse(view)
  end

  ui.present(ui.column({
    id = "store-root",
    h = screen_height(),
    pad = "md",
    gap = "md",
    children = { header(view), body, footer(view) },
  }), theme())
end

-- The snapshot this view was designed against. Tests and the composer use it;
-- the live module replaces it with the signed index and never edits the view.
M.fixture = fixture

return M