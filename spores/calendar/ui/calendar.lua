-- Calendar — a fast week/agenda/month view over soil:records events.
--
-- The host owns the records book, time zone resolution, layout, hit testing and
-- persistence. This view owns hierarchy, density, local date navigation, filter
-- memory and the selected event. It never reads iCal, files, sockets or a clock.

local ui = require("fungos.ui")
local action = require("fungos.action")

local M = {}
M.name = "example.calendar"
M.version = "0.1.0"

local VIEWS = {
  { id = "week", label = "Week" },
  { id = "agenda", label = "Agenda" },
  { id = "month", label = "Month" },
}

local WEEKDAYS = { "MON", "TUE", "WED", "THU", "FRI", "SAT", "SUN" }
local MONTHS = {
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
}

local function pad(value) return string.format("%02d", value) end

local function parse_date(value)
  local year, month, day = tostring(value or ""):match("^(%d%d%d%d)%-(%d%d)%-(%d%d)")
  if not year then return nil end
  return tonumber(year), tonumber(month), tonumber(day)
end

local function date_key(year, month, day)
  return string.format("%04d-%02d-%02d", year, month, day)
end

-- Howard Hinnant's civil-date algorithms, kept local so the view needs no os
-- module and remains pure. Sunday is 0.
local function days_from_civil(year, month, day)
  year = year - (month <= 2 and 1 or 0)
  local era = math.floor(year / 400)
  local yoe = year - era * 400
  local doy = math.floor((153 * (month + (month > 2 and -3 or 9)) + 2) / 5) + day - 1
  local doe = yoe * 365 + math.floor(yoe / 4) - math.floor(yoe / 100) + doy
  return era * 146097 + doe - 719468
end

local function civil_from_days(days)
  local z = days + 719468
  local era = math.floor(z / 146097)
  local doe = z - era * 146097
  local yoe = math.floor((doe - math.floor(doe / 1460) + math.floor(doe / 36524) - math.floor(doe / 146096)) / 365)
  local year = yoe + era * 400
  local doy = doe - (365 * yoe + math.floor(yoe / 4) - math.floor(yoe / 100))
  local mp = math.floor((5 * doy + 2) / 153)
  local day = doy - math.floor((153 * mp + 2) / 5) + 1
  local month = mp + (mp < 10 and 3 or -9)
  year = year + (month <= 2 and 1 or 0)
  return year, month, day
end

local function day_number(key)
  local year, month, day = parse_date(key)
  if not year then return 0 end
  return days_from_civil(year, month, day)
end

local function add_days(key, count)
  local year, month, day = civil_from_days(day_number(key) + count)
  return date_key(year, month, day)
end

local function weekday(key)
  return (day_number(key) + 4) % 7
end

local function week_start(key)
  return add_days(key, -((weekday(key) + 6) % 7))
end

local function month_start(key)
  local year, month = parse_date(key)
  return date_key(year, month, 1)
end

local function days_in_month(key)
  local year, month = parse_date(key)
  local next_year, next_month = year, month + 1
  if next_month > 12 then next_year, next_month = next_year + 1, 1 end
  return days_from_civil(next_year, next_month, 1) - days_from_civil(year, month, 1)
end

local function short_date(key)
  local _, month, day = parse_date(key)
  if not month then return key end
  return string.format("%s %d", MONTHS[month]:sub(1, 3), day)
end

local function date_label(key)
  local year, month, day = parse_date(key)
  if not year then return key end
  return string.format("%s %d, %d", MONTHS[month], day, year)
end

local function encode(value)
  return (tostring(value or ""):gsub("([^%w%-%._~])", function(char)
    return string.format("%%%02X", string.byte(char))
  end))
end

local function decode(value)
  return (tostring(value or ""):gsub("%%(%x%x)", function(hex)
    return string.char(tonumber(hex, 16))
  end))
end

local function ensure_today(snapshot)
  if snapshot.today and snapshot.today.date then return snapshot end
  local module = rawget(_G, "module")
  local now = module and module.now
  if type(now) == "table" and now.year and now.month and now.day then
    local key = date_key(now.year, now.month, now.day)
    snapshot.today = {
      date = key, year = now.year, month = now.month, day = now.day,
      weekday = weekday(key),
      label = string.format("%s %d", MONTHS[now.month], now.day),
      week_start = week_start(key),
      month_start = month_start(key),
    }
  end
  return snapshot
end

local function read_state(snapshot)
  local module = rawget(_G, "module")
  local raw = module and module.view and module.view.selected
  local state = {
    view = "week",
    date = snapshot.today and snapshot.today.date or "",
    calendar = "all",
    event = snapshot.selected_id,
  }
  if type(raw) ~= "string" or raw == "" then return state end
  if raw:sub(1, 2) ~= "v1" then
    state.event = raw
    return state
  end
  for part in raw:gmatch("[^;]+") do
    local key, value = part:match("^([%w_]+)=(.*)$")
    if key == "view" and (value == "week" or value == "agenda" or value == "month") then
      state.view = value
    elseif key == "date" and parse_date(decode(value)) then
      state.date = decode(value)
    elseif key == "cal" then
      state.calendar = decode(value)
    elseif key == "event" then
      state.event = decode(value)
    end
  end
  return state
end

local function write_state(state)
  local module = rawget(_G, "module")
  if not (module and type(module.select) == "function") then return end
  local parts = {
    "v1",
    "view=" .. state.view,
    "date=" .. encode(state.date),
  }
  if state.calendar and state.calendar ~= "all" then
    parts[#parts + 1] = "cal=" .. encode(state.calendar)
  end
  if state.event then parts[#parts + 1] = "event=" .. encode(state.event) end
  local raw = table.concat(parts, ";")
  if #raw <= 128 then pcall(module.select, raw) end
end

local function calendar_for(snapshot, id)
  for _, calendar in ipairs(snapshot.calendars or {}) do
    if calendar.id == id then return calendar end
  end
  return { id = id or "default", label = "Calendar", color = "primary" }
end

local function event_matches(event, state)
  return state.calendar == "all" or event.calendar_id == state.calendar
end

local function visible_events(snapshot, state)
  local out = {}
  for _, event in ipairs(snapshot.events or {}) do
    if event_matches(event, state) then out[#out + 1] = event end
  end
  return out
end

local function events_on(snapshot, state, key)
  local out = {}
  for _, event in ipairs(visible_events(snapshot, state)) do
    local start = event.date_key or ""
    local finish = event.end_date or start
    if start <= key and (finish == "" or finish >= key) then out[#out + 1] = event end
  end
  table.sort(out, function(a, b)
    if (a.all_day == true) ~= (b.all_day == true) then return a.all_day == true end
    return (a.start_ms or 0) < (b.start_ms or 0)
  end)
  return out
end

local function selected_event(snapshot, state)
  for index, event in ipairs(snapshot.events or {}) do
    if event.id == state.event then return event, index end
  end
  local day_events = events_on(snapshot, state, state.date)
  return day_events[1], nil
end

local function next_event(snapshot, now)
  local best
  for _, event in ipairs(snapshot.events or {}) do
    local start = tonumber(event.start_ms) or 0
    if not event.all_day and start > now and (not best or start < (tonumber(best.start_ms) or 0)) then
      best = event
    end
  end
  return best
end

local function relative_minutes(ms, now)
  local minutes = math.floor(((tonumber(ms) or 0) - now) / 60000 + 0.5)
  if minutes < 0 then return "now" end
  if minutes < 60 then return string.format("in %dm", minutes) end
  return string.format("in %dh %02dm", math.floor(minutes / 60), minutes % 60)
end

local function time_label(event)
  if event.all_day then return "All day" end
  local start = event.start_time or ""
  local finish = event.end_time or ""
  if start == "" then return "" end
  if finish == "" then return start end
  return start .. "–" .. finish
end

local function event_meta(event)
  local parts = { time_label(event) }
  if event.location and event.location ~= "" then parts[#parts + 1] = event.location end
  if event.is_recurring then parts[#parts + 1] = "repeats" end
  return table.concat(parts, "  ·  ")
end

local function find_event_index(snapshot, id)
  for index, event in ipairs(snapshot.events or {}) do
    if event.id == id then return index end
  end
end

local function event_row(snapshot, event, index, opts)
  opts = opts or {}
  local calendar = calendar_for(snapshot, event.calendar_id)
  local selected = event.id == opts.selected_id
  local children = {
    ui.text("cal-event-" .. index .. "-time", time_label(event), {
      size = 9, width = opts.time_width or 84, height = 17,
      ink = selected and "primary" or "muted",
    }),
    ui.column("cal-event-" .. index .. "-body", { flex_grow = 1, gap = 3 }, {
      ui.text("cal-event-" .. index .. "-title", ui.clip(event.summary or "Untitled", opts.title_chars or 40), {
        size = opts.size or 11, height = 20, ink = selected and "primary" or "text",
      }),
      ui.text("cal-event-" .. index .. "-meta", ui.clip(event_meta(event), opts.meta_chars or 52), {
        size = 9, height = 15, ink = "muted",
      }),
    }),
  }
  return ui.list_row("cal-event-" .. index, {
    height = opts.height or 66,
    gap = 12,
    padding = { top = 7, right = 8, bottom = 7, left = 8 },
    selected = selected,
    action = action.intent("calendar.select", { event_id = event.id }),
  }, children)
end

local function view_tabs(state)
  local children = {}
  for _, view in ipairs(VIEWS) do
    local selected = state.view == view.id
    children[#children + 1] = ui.list_row("cal-view-" .. view.id, {
      width = 76, height = 30,
      padding = { left = 10, right = 10 },
      selected = selected,
      action = action.intent("calendar.view", { view = view.id }),
    }, {
      ui.text("cal-view-" .. view.id .. "-label", view.label, {
        size = 9, ink = selected and "primary" or "muted", flex_grow = 1,
      }),
    })
  end
  return ui.row("cal-views", { width = 244, height = 30, gap = 4 }, children)
end

local function header(snapshot, state)
  local range
  if state.view == "month" then
    local year, month = parse_date(state.date)
    range = string.format("%s %d", MONTHS[month], year)
  elseif state.view == "agenda" then
    range = "Next 30 days"
  else
    local start = week_start(state.date)
    range = short_date(start) .. " – " .. short_date(add_days(start, 6))
  end
  return ui.row("cal-header", { height = 86, gap = 12, align_items = "center" }, {
    ui.column("cal-heading", { flex_grow = 1, height = 76, gap = 8 }, {
      ui.text("cal-title", snapshot.title or "Calendar", { size = 25, ink = "primary", height = 44 }),
      ui.text("cal-subtitle", range .. "  ·  " .. (snapshot.account and snapshot.account.label or "Records"), {
        size = 9, ink = "muted", height = 18,
      }),
    }),
    ui.row("cal-nav", { width = 180, height = 30, gap = 4 }, {
      ui.button("cal-prev", "‹", { width = 36, height = 30, size = 14 }),
      ui.button("cal-today", "Today", { flex_grow = 1, height = 30, size = 10 }),
      ui.button("cal-next", "›", { width = 36, height = 30, size = 14 }),
    }),
    view_tabs(state),
  })
end

local function mini_month(snapshot, state)
  local start = month_start(state.date)
  local year, month = parse_date(start)
  local grid_start = add_days(start, -((weekday(start) + 6) % 7))
  local children = {
    ui.row("cal-mini-head", { height = 24 }, {
      ui.text("cal-mini-title", string.format("%s %d", MONTHS[month]:sub(1, 3), year), {
        size = 11, ink = "text", flex_grow = 1,
      }),
    }),
    ui.row("cal-mini-weekdays", { height = 18, gap = 1 }, (function()
      local out = {}
      for index, label in ipairs(WEEKDAYS) do
        out[#out + 1] = ui.text("cal-mini-wd-" .. index, label:sub(1, 1), { size = 8, ink = "muted", flex_grow = 1 })
      end
      return out
    end)()),
  }
  for week = 0, 5 do
    local cells = {}
    for day = 0, 6 do
      local key = add_days(grid_start, week * 7 + day)
      local _, cell_month, cell_day = parse_date(key)
      local is_today = snapshot.today and key == snapshot.today.date
      local is_selected = key == state.date
      local in_month = cell_month == month
      local count = #events_on(snapshot, state, key)
      cells[#cells + 1] = ui.text("cal-mini-" .. key .. "-day",
        tostring(cell_day) .. (count > 0 and "•" or ""), {
          size = 9, flex_grow = 1,
          ink = is_selected and "background" or (is_today and "primary" or (in_month and "text" or "muted")),
          height = 28,
        })
    end
    children[#children + 1] = ui.row("cal-mini-week-" .. week, { height = 30, gap = 1 }, cells)
  end
  return ui.column("cal-mini", { gap = 3 }, children)
end

local function calendar_filter(snapshot, state)
  local items = {
    { id = "all", label = "All calendars", count = #(snapshot.events or {}), selected = state.calendar == "all" },
  }
  for _, calendar in ipairs(snapshot.calendars or {}) do
    items[#items + 1] = {
      id = calendar.id,
      label = calendar.label or calendar.id,
      count = calendar.count,
      selected = state.calendar == calendar.id,
    }
  end
  return ui.sidebar_group("cal-filters", "CALENDARS", items, { height = 28, size = 10 })
end

local function next_up(snapshot, now)
  local event = next_event(snapshot, now)
  if not event then
    return ui.column("cal-next-up", { gap = 3 }, {
      ui.section_label("cal-next-up-label", "NEXT UP"),
      ui.text("cal-next-empty", "Nothing scheduled.", { size = 10, ink = "muted", height = 20 }),
    })
  end
  local calendar = calendar_for(snapshot, event.calendar_id)
  return ui.column("cal-next-up", { gap = 4 }, {
    ui.section_label("cal-next-up-label", "NEXT UP"),
    ui.list_row("cal-next-card", { height = 68, padding = { left = 10, right = 10 } }, {
      ui.column("cal-next-body", { flex_grow = 1, gap = 1 }, {
        ui.text("cal-next-title", ui.clip(event.summary or "Untitled", 28), { size = 11, ink = "primary", height = 17 }),
        ui.text("cal-next-meta", relative_minutes(event.start_ms, now) .. "  ·  " .. time_label(event), {
          size = 9, ink = "muted", height = 15,
        }),
        ui.text("cal-next-cal", calendar.label or calendar.id or "Calendar", {
          size = 8, ink = calendar.color or "muted", height = 14,
        }),
      }),
    }),
  })
end

local function sidebar(snapshot, state, now)
  return ui.sidebar("cal-sidebar", {
    width = 272, flex_shrink = 0, gap = 12,
    padding = { top = 4, right = 10, bottom = 8, left = 0 },
  }, {
    mini_month(snapshot, state),
    calendar_filter(snapshot, state),
    next_up(snapshot, now),
  })
end

local function week_view(snapshot, state, max_chars)
  local start = week_start(state.date)
  local columns = {}
  for index = 0, 6 do
    local key = add_days(start, index)
    local events = events_on(snapshot, state, key)
    local is_today = snapshot.today and key == snapshot.today.date
    local children = {}
    for event_index = 1, math.min(#events, 4) do
      local event = events[event_index]
      local full_index = find_event_index(snapshot, event.id) or event_index
      local id = "cal-week-event-" .. index .. "-" .. full_index
      local marker = event.all_day and "all-day" or (event.start_time or "")
      children[#children + 1] = ui.list_row(id, {
        height = 34, gap = 0, padding = { top = 4, right = 5, bottom = 4, left = 5 },
        selected = event.id == state.event,
        action = action.intent("calendar.select", { event_id = event.id }),
      }, {
        ui.text(id .. "-title", ui.clip(marker .. "  " .. (event.summary or "Untitled"), max_chars or 14), {
          size = 8, flex_grow = 1,
          ink = event.id == state.event and "primary" or (event.all_day and "muted" or "text"),
        }),
      })
    end
    if #events > 4 then
      children[#children + 1] = ui.text("cal-week-more-" .. index, string.format("+%d", #events - 4), {
        size = 8, ink = "muted", height = 13,
      })
    end
    columns[#columns + 1] = ui.column("cal-day-" .. index, {
      flex_grow = 1, gap = 4, padding = { left = 4, right = 4 },
    }, children)
  end
  return ui.column("cal-week", { gap = 6, flex_grow = 1 }, {
    ui.row("cal-week-head", { height = 42, gap = 4 }, (function()
      local out = {}
      for index = 0, 6 do
        local key = add_days(start, index)
        local is_today = snapshot.today and key == snapshot.today.date
        out[#out + 1] = ui.column("cal-week-head-" .. index, { flex_grow = 1, gap = 1, align_items = "center" }, {
          ui.text("cal-week-wd-" .. index, WEEKDAYS[index + 1], { size = 8, ink = is_today and "primary" or "muted" }),
          ui.text("cal-week-num-" .. index, tostring(select(3, parse_date(key))), {
            size = 16, ink = is_today and "primary" or "text",
          }),
        })
      end
      return out
    end)()),
    ui.rule("cal-week-rule", { height = 1 }),
    ui.row("cal-week-columns", { flex_grow = 1, gap = 4 }, columns),
  })
end

local function agenda_view(snapshot, state)
  local events = visible_events(snapshot, state)
  table.sort(events, function(a, b) return (a.start_ms or 0) < (b.start_ms or 0) end)
  local groups = {}
  local current_key
  local current
  for _, event in ipairs(events) do
    local key = event.date_key or ""
    if key ~= current_key then
      current_key = key
      current = { key = key, events = {} }
      groups[#groups + 1] = current
    end
    current.events[#current.events + 1] = event
  end
  local children = {}
  for group_index, group in ipairs(groups) do
    local is_today = snapshot.today and group.key == snapshot.today.date
    children[#children + 1] = ui.row("cal-agenda-head-" .. group_index, { height = 28, gap = 8 }, {
      ui.text("cal-agenda-date-" .. group_index, date_label(group.key), {
        size = 11, ink = is_today and "primary" or "text", flex_grow = 1,
      }),
      ui.text("cal-agenda-count-" .. group_index, tostring(#group.events) .. (#group.events == 1 and " event" or " events"), {
        size = 9, ink = "muted",
      }),
    })
    for event_index, event in ipairs(group.events) do
      local index = find_event_index(snapshot, event.id) or event_index
      children[#children + 1] = event_row(snapshot, event, index, {
        selected_id = state.event,
        height = 66,
        title_chars = 56,
        meta_chars = 70,
      })
    end
  end
  if #children == 0 then
    children[1] = ui.text("cal-agenda-empty", "No events in this range.", {
      size = 11, ink = "muted", height = 28,
    })
  end
  return ui.column("cal-agenda", { gap = 6, flex_grow = 1 }, children)
end

local function month_view(snapshot, state)
  local start = month_start(state.date)
  local grid_start = add_days(start, -((weekday(start) + 6) % 7))
  local year, month = parse_date(start)
  local rows = {
    ui.row("cal-month-weekdays", { height = 22, gap = 2 }, (function()
      local out = {}
      for index, label in ipairs(WEEKDAYS) do
        out[#out + 1] = ui.text("cal-month-wd-" .. index, label, { size = 8, ink = "muted", flex_grow = 1 })
      end
      return out
    end)()),
  }
  for week = 0, 5 do
    local cells = {}
    for day = 0, 6 do
      local key = add_days(grid_start, week * 7 + day)
      local _, cell_month, cell_day = parse_date(key)
      local in_month = cell_month == month
      local is_today = snapshot.today and key == snapshot.today.date
      local is_selected = key == state.date
      local events = events_on(snapshot, state, key)
      local children = {
        ui.text("cal-month-" .. key .. "-day", tostring(cell_day) .. (#events > 0 and "•" or ""), {
          size = 10, ink = is_selected and "background" or (is_today and "primary" or (in_month and "text" or "muted")),
          height = 18,
        }),
      }
      if events[1] then
        local suffix = #events > 1 and (" +" .. (#events - 1)) or ""
        children[#children + 1] = ui.text("cal-month-" .. key .. "-event-1",
          ui.clip((events[1].summary or "Untitled") .. suffix, 16), {
            size = 8, ink = is_selected and "background" or (events[1].calendar_color or "muted"), height = 13,
          })
      end
      cells[#cells + 1] = ui.column("cal-month-cell-" .. key, {
        flex_grow = 1, height = 88, gap = 1,
        padding = { top = 5, right = 4, bottom = 4, left = 5 },
      }, children)
    end
    rows[#rows + 1] = ui.row("cal-month-week-" .. week, { height = 90, gap = 2 }, cells)
  end
  return ui.column("cal-month", { gap = 2, flex_grow = 1 }, rows)
end

local function detail(snapshot, state, now, width)
  local event = selected_event(snapshot, state)
  local children = {
    ui.section_label("cal-detail-label", "EVENT"),
  }
  if not event then
    children[#children + 1] = ui.text("cal-detail-empty", "Select an event to inspect it.", {
      size = 10, ink = "muted", height = 24, wrap = true,
    })
    return ui.column("cal-detail", { width = width, gap = 6 }, children)
  end
  local calendar = calendar_for(snapshot, event.calendar_id)
  children[#children + 1] = ui.text("cal-detail-title", event.summary or "Untitled", {
    size = 18, ink = "primary", height = 30, wrap = true,
  })
  children[#children + 1] = ui.text("cal-detail-when",
    (event.date_key and short_date(event.date_key) or "") .. "  ·  " .. time_label(event), {
      size = 10, ink = "text", height = 18,
    })
  children[#children + 1] = ui.row("cal-detail-cal", { height = 20, gap = 8 }, {
    ui.text("cal-detail-cal-label", calendar.label or "Calendar", {
      size = 9, ink = calendar.color or "primary", width = 76,
    }),
    ui.text("cal-detail-relative", relative_minutes(event.start_ms, now), {
      size = 9, ink = "muted", flex_grow = 1,
    }),
  })
  local line = 0
  local function add(label, value, ink)
    if not value or value == "" then return end
    line = line + 1
    children[#children + 1] = ui.text("cal-detail-line-" .. line, label .. "  " .. value, {
      size = 9, ink = ink or "text", height = 18, wrap = true,
    })
  end
  add("LOCATION", event.location)
  add("ORGANIZER", event.organizer)
  add("ATTENDEES", table.concat(event.attendees or {}, ", "))
  if event.is_recurring and event.rrule and event.rrule ~= "" then
    add("REPEATS", event.rrule, "primary")
  end
  if #(event.conflicts or {}) > 0 then
    line = line + 1
    children[#children + 1] = ui.text("cal-detail-conflict", "CONFLICT  overlaps another event", {
      size = 9, ink = "warning", height = 18,
    })
  end
  if event.notes and event.notes ~= "" then
    children[#children + 1] = ui.section_label("cal-detail-notes-label", "NOTES")
    children[#children + 1] = ui.text("cal-detail-notes", ui.clip(event.notes, 900), {
      size = 10, ink = "muted", height = 96, wrap = true,
    })
  end
  children[#children + 1] = ui.rule("cal-detail-rule", { height = 1 })
  children[#children + 1] = ui.row("cal-detail-actions", { height = 30, gap = 6 }, {
    ui.button("cal-detail-open", "Open", {
      size = 10, height = 28, flex_grow = 1,
      action = action.intent("calendar.open", { event_id = event.id }),
    }),
    ui.button("cal-detail-edit", "Edit", {
      size = 10, height = 28, flex_grow = 1,
      action = action.intent("calendar.edit", { event_id = event.id }),
    }),
  })
  return ui.column("cal-detail", { width = width, gap = 6 }, children)
end

local function activate(snapshot, state, target)
  local event_index = target:match("^cal%-event%-(%d+)$")
    or target:match("^cal%-week%-event%-%d+%-(%d+)$")
  if event_index then
    local event = (snapshot.events or {})[tonumber(event_index)]
    if event then
      state.event = event.id
      state.date = event.date_key or state.date
    end
  elseif target == "cal-prev" then
    state.date = add_days(state.date, state.view == "month" and -28 or -7)
  elseif target == "cal-next" then
    state.date = add_days(state.date, state.view == "month" and 28 or 7)
  elseif target == "cal-today" then
    state.date = snapshot.today and snapshot.today.date or state.date
  elseif target:match("^cal%-view%-(%w+)$") then
    local view = target:match("^cal%-view%-(%w+)$")
    if view == "week" or view == "agenda" or view == "month" then state.view = view end
  elseif target:match("^cal%-filter%-") then
    state.calendar = target:match("^cal%-filter%-(.+)$")
  elseif target:match("^cal%-mini%-%d%d%d%d%-%d%d%-%d%d$") then
    state.date = target:match("^cal%-mini%-(.+)$")
  elseif target:match("^cal%-month%-cell%-%d%d%d%d%-%d%d%-%d%d$") then
    state.date = target:match("^cal%-month%-cell%-(.+)$")
  end
  write_state(state)
end

local function paint_text(surface, laid, state, snapshot)
  ui.paint_text(surface, laid, function(node)
    if node.id == "cal-title" or node.id == "cal-detail-title" then return "primary" end
    if node.id == "cal-detail-cal-label" then
      local event = selected_event(snapshot, state)
      local calendar = event and calendar_for(snapshot, event.calendar_id)
      return calendar and calendar.color or "primary"
    end
    if node.id:find("^cal%-mini%-%d%d%d%d%-%d%d%-%d%d%-day$") then
      local key = node.id:match("^cal%-mini%-(%d%d%d%d%-%d%d%-%d%d)%-day$")
      if key == state.date then return "background" end
      if snapshot.today and key == snapshot.today.date then return "primary" end
    end
    if node.id:find("^cal%-month%-%d%d%d%d%-%d%d%-%d%d%-day$") then
      local key = node.id:match("^cal%-month%-(%d%d%d%d%-%d%d%-%d%d)%-day$")
      if key == state.date then return "background" end
      if snapshot.today and key == snapshot.today.date then return "primary" end
    end
    if node.id:find("^cal%-event%-%d+%-title$") or node.id:find("^cal%-week%-event%-%d+%-%d+%-title$") then
      local index = tonumber(node.id:match("^cal%-event%-(%d+)%-title$") or node.id:match("^cal%-week%-event%-%d+%-(%d+)%-title$"))
      local event = index and (snapshot.events or {})[index]
      if event and event.id == state.event then return "primary" end
      return "text"
    end
    if node.id:find("conflict") then return "warning" end
    if node.id:find("meta") or node.id:find("subtitle") or node.id:find("time")
      or node.id:find("count") or node.id:find("empty") or node.id:find("notes")
      or node.id:find("dots") or node.id:find("more") then return "muted" end
    return node.ink or "text"
  end)
end

local function paint_chrome(surface, laid, state, snapshot, now)
  local sidebar = ui.find(laid, "cal-sidebar")
  local main = ui.find(laid, "cal-main")
  local detail_node = ui.find(laid, "cal-detail")
  if sidebar then
    surface:rect("cal-sidebar-bg", sidebar.rect.x, sidebar.rect.y, sidebar.rect.width, sidebar.rect.height, "surface", 0.42, 12)
  end
  if detail_node then
    surface:rect("cal-detail-bg", detail_node.rect.x, detail_node.rect.y, detail_node.rect.width, detail_node.rect.height, "surface", 0.30, 12)
  end
  -- Today and selected date markers.
  for index, node in ipairs(laid.nodes or {}) do
    if node.kind == "container" then
      local key = node.id:match("^cal%-mini%-(%d%d%d%d%-%d%d%-%d%d)$")
        or node.id:match("^cal%-month%-cell%-(%d%d%d%d%-%d%d%-%d%d)$")
      if key then
        local selected = key == state.date
        local today = snapshot.today and key == snapshot.today.date
        if selected or today then
          surface:rect("cal-marker-" .. key, node.rect.x, node.rect.y, node.rect.width, node.rect.height,
            selected and "primary" or "surface", selected and 0.18 or 0.35, 7)
        end
      end
    elseif node.kind == "text" then
      local key = node.id:match("^cal%-mini%-(%d%d%d%d%-%d%d%-%d%d)%-day$")
      if key then
        local selected = key == state.date
        local today = snapshot.today and key == snapshot.today.date
        if selected or today then
          surface:rect("cal-marker-mini-" .. key, node.rect.x, node.rect.y, node.rect.width, node.rect.height,
            selected and "primary" or "surface", selected and 0.18 or 0.35, 7)
        end
      end
    end
  end
  -- Event color rails.
  for _, node in ipairs(laid.nodes or {}) do
    local index = node.id:match("^cal%-event%-(%d+)$")
      or node.id:match("^cal%-week%-event%-%d+%-(%d+)$")
    index = tonumber(index)
    local event = index and (snapshot.events or {})[index]
    if event then
      local color = event.calendar_color or "primary"
      surface:rect("cal-event-rail-" .. node.id, node.rect.x, node.rect.y, 3, node.rect.height,
        event.id == state.event and "primary" or color, 0.75, 2)
    end
  end
end

local function render(snapshot, state, ctx)
  local now = tonumber(ctx.now) or ui.now_ms()
  local wide = ctx.viewport.width >= 1320
  local main_width = wide
    and (ctx.viewport.width - 36 - 272 - 292 - 28)
    or (ctx.viewport.width - 36 - 272 - 14)
  local week_chars = math.max(8, math.floor((math.max(280, main_width) / 7 - 12) / 7))
  local main_view
  if state.view == "agenda" then
    main_view = agenda_view(snapshot, state)
  elseif state.view == "month" then
    main_view = month_view(snapshot, state)
  else
    main_view = week_view(snapshot, state, week_chars)
  end
  local main_pane = ui.pane("cal-main", {
    pane_id = "calendar", flex_grow = 1, min_width = 0, gap = 4,
  }, { main_view })
  local body
  if wide then
    body = ui.row("cal-body", { flex_grow = 1, gap = 14 }, {
      sidebar(snapshot, state, now), main_pane, detail(snapshot, state, now, 292),
    })
  else
    body = ui.column("cal-body", { flex_grow = 1, gap = 10 }, {
      ui.row("cal-body-row", { flex_grow = 1, gap = 14 }, {
        sidebar(snapshot, state, now), main_pane,
      }),
      detail(snapshot, state, now, nil),
    })
  end
  return ui.workspace("cal-root", {
    width = ctx.viewport.width,
    height = ctx.viewport.height,
    gap = 10,
    padding = { top = 16, right = 18, bottom = 14, left = 18 },
  }, {
    header(snapshot, state),
    body,
  })
end

function M.render(input)
  local snapshot = ensure_today(input or {})
  local state = read_state(snapshot)
  local event = ui.event()
  if event and event.kind == "activate" and event.target_id then
    activate(snapshot, state, event.target_id)
  end
  local ctx = {
    viewport = (function()
      local vp = rawget(_G, "module") and rawget(_G, "module").viewport
      return { width = vp and vp.width or 1200, height = vp and vp.height or 900 }
    end)(),
    now = ui.now_ms(),
  }
  local tree = render(snapshot, state, ctx)
  local laid = canvas.layout(tree, ctx.viewport.width, ctx.viewport.height)
  local surface = ui.surface(ctx.viewport.width, ctx.viewport.height)
  surface:background("background")
  paint_chrome(surface, laid, state, snapshot, ctx.now)
  paint_text(surface, laid, state, snapshot)
  -- Hit targets: navigation, views, filters, mini-month days, month cells,
  -- week/agenda events and detail actions.
  local ids = {
    "cal-prev", "cal-today", "cal-next",
    "cal-view-week", "cal-view-agenda", "cal-view-month",
    "cal-filter-all",
    "cal-detail-open", "cal-detail-edit",
  }
  for _, calendar in ipairs(snapshot.calendars or {}) do ids[#ids + 1] = "cal-filter-" .. calendar.id end
  for index, node in ipairs(laid.nodes or {}) do
    if node.id:match("^cal%-mini%-%d%d%d%d%-%d%d%-%d%d$")
      or node.id:match("^cal%-month%-cell%-%d%d%d%d%-%d%d%-%d%d$")
      or node.id:match("^cal%-event%-%d+$") then
      ids[#ids + 1] = node.id
    end
  end
  for _, id in ipairs(ids) do
    local node = ui.find(laid, id)
    if node then surface:hit(id, node.rect) end
  end
  for _, node in ipairs(laid.nodes or {}) do
    if node.id:match("^cal%-week%-event%-%d+%-%d+$") then
      surface:hit(node.id, node.rect)
    end
  end
end

return M
