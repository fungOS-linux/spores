-- Search: three-pane browser for the host VFS workspace.
--
-- Canvas has no viewport size in the snapshot, so the split is authored for a
-- wide dev placement and re-derived from `module.viewport` at render time. On a
-- narrow surface the three zones stack; on a wide one they sit side by side.
-- Layout is host-measured; hit targets route result-N back through the module
-- activation path to move the selected entry.

local M = {}
M.name = "search"
M.version = "0.1.0"

local W, H = 1024, 704
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

local function label(id, value, size, layout)
  return {
    id = id, kind = "text", text = tostring(value or ""),
    style = { size = size, line_height = size + 3, wrap = false },
    layout = layout or {},
  }
end

-- Zone widths as fractions of W. Wide surfaces get three columns; narrow ones
-- collapse to a single full-width column and the zones stack.
local function zones()
  local wide = W >= H
  if wide then
    return { sidebar = W * 0.26, results = W * 0.44, detail = W * 0.30 }, true
  end
  return { sidebar = W, results = W, detail = W }, false
end

local function human_size(n)
  if not n or n == 0 then return "—" end
  local units = { "B", "KB", "MB", "GB" }
  local value, i = n, 1
  while value >= 1024 and i < #units do value = value / 1024; i = i + 1 end
  return string.format("%d%s", math.floor(value + 0.5), units[i])
end

local function media_line(result)
  local mt = result.media_type or ""
  if mt == "" then mt = result.is_dir and "folder" or "file" end
  return mt .. "  ·  " .. human_size(result.size)
end

local function result_row(result, index)
  local kind_glyph = result.is_dir and "▸ " or "· "
  local children = {
    label("result-" .. index .. "-name", kind_glyph .. result.name, 15),
    label("result-" .. index .. "-meta", media_line(result), 11),
  }
  if result.snippet and result.snippet ~= "" then
    children[#children + 1] = label("result-" .. index .. "-snippet", result.snippet, 11, { wrap = true })
  end
  return {
    id = "result-" .. index,
    kind = "container",
    layout = { height = 74, direction = "column", gap = 3, padding = { top = 9, right = 12, bottom = 9, left = 12 } },
    children = children,
  }
end

local FILTERS = {
  { id = "all", label = "All" },
  { id = "files", label = "Files" },
  { id = "folders", label = "Folders" },
}

local function clip(value, limit)
  value = tostring(value or "")
  if #value <= limit then return value end
  return value:sub(1, limit - 1) .. "…"
end

local function filter_results(results, filter)
  local out = {}
  for _, result in ipairs(results or {}) do
    if filter == "files" then
      if not result.is_dir then out[#out + 1] = result end
    elseif filter == "folders" then
      if result.is_dir then out[#out + 1] = result end
    else
      out[#out + 1] = result
    end
  end
  return out
end

local function encode_path(value)
  return (tostring(value or ""):gsub("([^%w%-%._~/])", function(char)
    return string.format("%%%02X", string.byte(char))
  end))
end

local function decode_path(value)
  return (tostring(value or ""):gsub("%%(%x%x)", function(hex)
    return string.char(tonumber(hex, 16))
  end))
end

local function read_view_state()
  local module = rawget(_G, "module")
  local raw = module and module.view and module.view.selected
  if type(raw) ~= "string" or raw == "" then return { filter = "all" } end
  -- Backward-compatible selection-only state from the first slice.
  if raw:sub(1, 8) == "v1:path:" then
    return { filter = "all", path = decode_path(raw:sub(9)) }
  end
  if raw:sub(1, 9) == "v1:index:" then
    return { filter = "all", index = tonumber(raw:sub(10)) }
  end
  local state = { filter = "all" }
  if raw:sub(1, 2) == "v1" and raw:sub(3, 3) == ";" then
    for part in raw:gmatch("[^;]+") do
      local key, value = part:match("^([%w_]+)=(.*)$")
      if key == "sel" then
        if value:sub(1, 2) == "p:" then state.path = decode_path(value:sub(3))
        elseif value:sub(1, 2) == "i:" then state.index = tonumber(value:sub(3)) end
      elseif key == "filter" and (value == "all" or value == "files" or value == "folders") then
        state.filter = value
      elseif key == "hover" then
        state.hover = tonumber(value)
      end
    end
    return state
  end
  return { filter = "all", path = raw }
end

local function write_view_state(snapshot, state)
  local module = rawget(_G, "module")
  if not (module and type(module.select) == "function") then return end
  local path = snapshot.selected_path
  if type(path) ~= "string" or path == "" then return end
  local encoded = encode_path(path)
  local parts = { "v1" }
  if #encoded <= 76 then
    parts[#parts + 1] = "sel=p:" .. encoded
  else
    for index, result in ipairs(snapshot.results or {}) do
      if result.path == path then parts[#parts + 1] = "sel=i:" .. index break end
    end
  end
  if state.filter and state.filter ~= "all" then
    parts[#parts + 1] = "filter=" .. state.filter
  end
  if state.hover and snapshot.results and snapshot.results[state.hover] then
    parts[#parts + 1] = "hover=" .. state.hover
  end
  local key = table.concat(parts, ";")
  -- Selection is the important part. Drop transient hover first, then filter,
  -- rather than losing the row the person deliberately chose.
  while #key > 128 and #parts > 2 do
    parts[#parts] = nil
    key = table.concat(parts, ";")
  end
  if #key <= 128 then pcall(module.select, key) end
end

local function full_result_index(snapshot, path)
  for index, result in ipairs(snapshot.results or {}) do
    if result.path == path then return index end
  end
end

local function apply_view_state(snapshot, state)
  if state.path then
    for _, result in ipairs(snapshot.results or {}) do
      if result.path == state.path then
        snapshot.selected_path = state.path
        return snapshot
      end
    end
  elseif state.index and (snapshot.results or {})[state.index] then
    snapshot.selected_path = snapshot.results[state.index].path
  end
  return snapshot
end

local function selected_result(snapshot, results)
  for index, result in ipairs(results or {}) do
    if result.path == snapshot.selected_path then return result, index end
  end
  return (results or {})[1], 1
end

local function layout_tree(snapshot, state)
  local z, wide = zones()
  local results_for_filter = filter_results(snapshot.results or {}, state.filter)
  local row_height = wide and 82 or 76
  local list_room = math.max(1, math.floor((H - (wide and 230 or 260)) / row_height))
  local visible = math.min(#results_for_filter, list_room, 48)
  local selected, selected_index = selected_result(snapshot, results_for_filter)

  -- Left sidebar: title, query, filters and result count.
  local sidebar = {
    { id = "search-head", kind = "container", layout = { direction = "row", height = 46, gap = 8 },
      children = {
        label("search-title", "Search", 26, { flex_grow = 1 }),
        label("search-count", tostring(snapshot.total or #(snapshot.results or {})), 13),
      } },
    label("search-subtitle", (snapshot.ready and "VFS workspace" or "index not ready")
      .. (snapshot.mode and snapshot.mode ~= "not-ready" and ("  ·  " .. snapshot.mode) or ""), 11, { height = 18 }),
    { id = "search-field", kind = "container", layout = { height = 38, padding = { left = 12, right = 10, top = 10 } },
      children = {
        label("search-query", (snapshot.query ~= "") and ("⌕ " .. snapshot.query) or "⌕  Search files, paths, content", 11),
      } },
  }
  local chips = {}
  for _, filter in ipairs(FILTERS) do
    local count = #filter_results(snapshot.results or {}, filter.id)
    chips[#chips + 1] = {
      id = "filter-" .. filter.id,
      kind = "container",
      selected = state.filter == filter.id,
      layout = { direction = "row", height = 26, padding = { top = 5, right = 10, bottom = 5, left = 10 } },
      children = { label("filter-" .. filter.id .. "-label", filter.label .. "  " .. count, 10) },
    }
  end
  sidebar[#sidebar + 1] = {
    id = "search-filters", kind = "container",
    layout = { direction = "row", height = 30, gap = 6 },
    children = chips,
  }
  sidebar[#sidebar + 1] = label("search-status",
    string.format("%d shown · generation %d", #results_for_filter, tonumber(snapshot.generation) or 0),
    10, { height = 18 })
  if snapshot.error then
    sidebar[#sidebar + 1] = label("search-error", snapshot.error, 11, { height = 18 })
  end

  -- Main pane: the results list.
  local rows = {}
  for index = 1, visible do rows[#rows + 1] = result_row(results_for_filter[index], index) end
  if #rows == 0 then
    rows[1] = label("results-empty", snapshot.error and "Index unavailable" or "No matches", 13, { height = 28 })
  end
  local results = {
    { id = "results-head", kind = "container", layout = { direction = "row", height = 34 },
      children = { label("results-title", "RESULTS", 10, { flex_grow = 1 }) } },
    { id = "search-list", kind = "container", layout = { direction = "column", gap = 2 }, children = rows },
  }

  -- Detail pane. `snapshot.detail` is authoritative only for the path the host
  -- previewed; selecting another row must not show a stale preview from hit 1.
  local detail = {}
  if selected then
    detail[1] = label("detail-name", selected.name, 20, { height = 28 })
    detail[2] = label("detail-meta", media_line(selected), 11, { height = 17 })
    detail[3] = label("detail-path", selected.path, 11, { height = 17, wrap = true })
    local row = 3
    local function line(value) row = row + 1; detail[row] = label("detail-" .. row, value, 11, { height = 19 }) end
    line("kind  " .. (selected.is_dir and "directory" or "file"))
    if selected.matched_by then line("matched by  " .. selected.matched_by) end
    if tonumber(selected.score) and tonumber(selected.score) > 0 then line("score  " .. tostring(selected.score)) end
    local preview
    if snapshot.detail and snapshot.detail.path == selected.path then
      preview = snapshot.detail.preview
    end
    if preview and preview ~= "" then
      row = row + 1
      detail[row] = label("detail-preview-label", "PREVIEW", 10, { height = 18 })
      row = row + 1
      detail[row] = label("detail-preview", clip(preview, 6000), 12, { wrap = true, height = H * 0.40 })
    elseif not selected.is_dir then
      row = row + 1
      detail[row] = label("detail-preview-empty", "Preview loads when the host selects this entry.", 11, { height = 19, wrap = true })
    end
  else
    detail[1] = label("detail-empty", "No selection", 13)
  end

  local sidebar_box = { id = "search-sidebar", kind = "container",
    layout = { direction = "column", gap = 6, padding = { top = 20, right = 16, bottom = 16, left = 16 } }, children = sidebar }
  local results_box = { id = "search-results", kind = "container",
    layout = { direction = "column", gap = 6, padding = { top = 20, right = 16, bottom = 16, left = 16 }, flex_grow = 1 }, children = results }
  local detail_box = { id = "search-detail", kind = "container",
    layout = { direction = "column", gap = 6, padding = { top = 20, right = 16, bottom = 16, left = 16 } }, children = detail }

  local root_layout
  if wide then
    root_layout = { width = W, height = H, direction = "row", gap = 0, padding = { top = 0, right = 0, bottom = 0, left = 0 } }
  else
    root_layout = { width = W, height = H, direction = "column", gap = 0, padding = { top = 0, right = 0, bottom = 0, left = 0 } }
  end

  return {
    id = "search-root", kind = "container", layout = root_layout,
    children = { sidebar_box, results_box, detail_box },
    _selected_index = selected_index,
  }
end

local function apply_event(snapshot, state)
  local event = _G.module and _G.module.event
  if not event or event.kind ~= "activate" then return snapshot end
  local target = tostring(event.target_id or "")
  if target:sub(1, 7) == "filter-" then
    local filter = target:sub(8)
    if filter ~= "all" and filter ~= "files" and filter ~= "folders" then return snapshot end
    state.filter = filter
    local results = filter_results(snapshot.results or {}, state.filter)
    local found = false
    for _, result in ipairs(results) do
      if result.path == snapshot.selected_path then found = true break end
    end
    if not found then
      snapshot.selected_path = results[1] and results[1].path or ""
      snapshot.detail = nil
    end
    write_view_state(snapshot, state)
    return snapshot
  end
  local index = tonumber(target:match("^result%-(%d+)$"))
  local results = filter_results(snapshot.results or {}, state.filter)
  if index and results[index] then
    snapshot.selected_path = results[index].path
    snapshot.detail = nil
    write_view_state(snapshot, state)
  end
  return snapshot
end

function M.render(snapshot)
  use_viewport()
  snapshot = snapshot or {}
  local state = read_view_state()
  snapshot = apply_view_state(snapshot, state)
  snapshot = apply_event(snapshot, state)
  local laid = canvas.layout(layout_tree(snapshot, state), W, H)

  rect("search-bg", 0, 0, W, H, "background", 1.0, 0)
  local z, _ = zones()

  -- Zone dividers / panels.
  local sidebar = find(laid, "search-sidebar")
  local results = find(laid, "search-results")
  local detail = find(laid, "search-detail")
  if sidebar then rect("search-sidebar-bg", sidebar.rect.x, sidebar.rect.y, z.sidebar, H, "surface", 0.55, 0) end
  if sidebar and results then
    rect("search-divider-1", sidebar.rect.x + z.sidebar - 1, 0, 2, H, "surface", 0.9, 0)
  end
  if results and detail then
    rect("search-divider-2", results.rect.x + z.results - 1, 0, 2, H, "surface", 0.9, 0)
  end

  -- Filter chip backgrounds.
  for _, filter in ipairs(FILTERS) do
    local chip = find(laid, "filter-" .. filter.id)
    if chip then
      local active = state.filter == filter.id
      rect("filter-" .. filter.id .. "-bg", chip.rect.x, chip.rect.y, chip.rect.width, chip.rect.height,
        active and "primary" or "surface", active and 0.18 or 0.48, 7)
    end
  end

  local filtered = filter_results(snapshot.results or {}, state.filter)
  local _, selected_index = selected_result(snapshot, filtered)

  -- Hover is host-throttled: Canvas only sends pointer.move when the hit target
  -- changes, so this costs a re-render on row entry/exit, not per mouse pixel.
  local event = _G.module and _G.module.event
  if event and event.kind == "pointer" then
    local next_hover
    if event.phase ~= "leave" and event.x and event.y then
      local px, py = event.x * W, event.y * H
      for index = 1, #filtered do
        local row = find(laid, "result-" .. index)
        if row and px >= row.rect.x and px <= row.rect.x + row.rect.width
          and py >= row.rect.y and py <= row.rect.y + row.rect.height then
          next_hover = full_result_index(snapshot, filtered[index].path)
          break
        end
      end
    end
    if next_hover ~= state.hover then
      state.hover = next_hover
      write_view_state(snapshot, state)
    end
  end

  local hover_index
  if state.hover and (snapshot.results or {})[state.hover] then
    local hover_path = snapshot.results[state.hover].path
    for index, result in ipairs(filtered) do
      if result.path == hover_path then hover_index = index break end
    end
  end
  local hover_row = hover_index and find(laid, "result-" .. tostring(hover_index)) or nil
  if hover_row and hover_index ~= selected_index then
    rect("search-hover", hover_row.rect.x - 6, hover_row.rect.y, hover_row.rect.width + 12, hover_row.rect.height, "surface", 0.32, 8)
  end

  -- Selected row highlight.
  local selected_row = find(laid, "result-" .. tostring(selected_index))
  if selected_row then
    rect("search-selected", selected_row.rect.x - 6, selected_row.rect.y, selected_row.rect.width + 12, selected_row.rect.height, "primary", 0.16, 8)
  end

  -- Text + hit targets.
  for _, node in ipairs(laid.nodes or {}) do
    local r = node.rect
    if node.kind == "text" and node.style then
      local ink = "text"
      if node.id:find("subtitle") or node.id:find("count") or node.id:find("meta")
        or node.id:find("snippet") or node.id:find("path") or node.id:find("preview")
        or node.id:find("empty") or node.id:find("status") then ink = "muted" end
      if node.id == "search-title" or node.id == "detail-name" or node.id:find("avatar") then ink = "primary" end
      if node.id:find("^filter%-") then
        for _, filter in ipairs(FILTERS) do
          if node.id == "filter-" .. filter.id .. "-label" and state.filter == filter.id then ink = "primary" end
        end
      end
      text(node.id, node.text, r.x, r.y, node.style.size, ink)
    end
  end

  for _, filter in ipairs(FILTERS) do
    local chip = find(laid, "filter-" .. filter.id)
    if chip then
      widget.hit { id = "filter-" .. filter.id, label = filter.label,
        x = nx(chip.rect.x), y = ny(chip.rect.y), width = sw(chip.rect.width), height = sh(chip.rect.height) }
    end
  end

  for index = 1, math.min(#filtered, 48) do
    local row = find(laid, "result-" .. index)
    if row then
      widget.hit { id = "result-" .. index, label = "hit",
        x = nx(row.rect.x), y = ny(row.rect.y), width = sw(row.rect.width), height = sh(row.rect.height) }
    end
  end
end

return M
