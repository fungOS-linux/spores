-- Sheets Spore UI.
--
-- A pure projection: the host supplies the calculated tab, this file decides
-- what it looks like. No arithmetic and no action bus happen here -- values
-- arrive from `fungos.sheet/calc@1` already evaluated by the wasm module. The
-- "Recompute" label is a note; recalculation is the Spore's `calc` command.
--
-- Nodes use theme tokens only (`ink`, `fill`), never literal colours, so the
-- same tree draws light or dark. `ui.rule` is a 1px line; `fill` is a token
-- painted behind a container. Both are host styling, not content.
local ui = require("fungos.ui")

local M = { name = "dev.fungos.sheets", version = "0.1.0" }

local DEFAULT_LABEL_WIDTH = 220
local CELL_WIDTH = 112
local ROW_HEIGHT = 26
local HEADER_HEIGHT = 22
local function number_text(value, decimals)
  local text = string.format("%." .. (decimals or 0) .. "f", value)
  local whole, fraction = text:match("^(%d+)(%.?%d*)$")
  local grouped = whole:reverse():gsub("(%d%d%d)", "%1,"):reverse()
  grouped = grouped:gsub("^,", "")
  return grouped .. (fraction or "")
end

local function group(value, decimals)
  if value == nil then return "—" end
  return number_text(value, decimals)
end

local function money(value, decimals)
  if value == nil then return "—" end
  local text = "$" .. number_text(math.abs(value), decimals)
  return value < 0 and ("(" .. text .. ")") or text
end

local FORMAT = {
  count = function(value) return group(value) end,
  money = function(value) return money(value) end,
  money2 = function(value) return money(value, 2) end,
  percent = function(value) return group(value, 1) .. "%" end,
}

local function cell_text(value, format)
  if type(value) == "string" then return value end
  return (FORMAT[format] or FORMAT.count)(value)
end

-- A profile name is longer than a line item, so the snapshot can widen the
-- label column without changing the cell columns.
local function label_width(snapshot)
  return snapshot.label_width or DEFAULT_LABEL_WIDTH
end

local function header(snapshot)
  return ui.row("sheet-header", { height = 54, gap = 12, align = "center" }, {
    ui.column("sheet-titles", { flex_grow = 1, gap = 2 }, {
      ui.text("sheet-title", snapshot.title or "Cash model", { size = 20, height = 26, ink = "primary" }),
      ui.text("sheet-subtitle", snapshot.subtitle or "Growth case · 60 months", { size = 10, height = 16, ink = "muted" }),
    }),
    ui.text("sheet-provenance", snapshot.provenance or "estimate", { size = 9, height = 16, ink = "muted", width = 200, align = "right" }),
  })
end

-- The grid is as wide as the snapshot has columns, so the same view renders a
-- five-year forecast or a three-scenario comparison.
local function columns_of(snapshot)
  return snapshot.columns or snapshot.years or {}
end

local function grid_width(snapshot)
  return label_width(snapshot) + #columns_of(snapshot) * (1 + CELL_WIDTH)
end

local function column_header(snapshot)
  local cells = {
    ui.text("col-label", snapshot.column_label or "LINE", { size = 9, height = HEADER_HEIGHT, width = label_width(snapshot), ink = "muted", pad = 10 }),
  }
  for index, year in ipairs(columns_of(snapshot)) do
    ui.push(cells, ui.rule("col-rule-" .. index, { width = 1, height = HEADER_HEIGHT, ink = "line" }))
    ui.push(cells, ui.text("col-" .. index, year, {
      size = 9, height = HEADER_HEIGHT, width = CELL_WIDTH, ink = "muted", align = "right", pad = 10,
    }))
  end
  return ui.row("sheet-columns", { height = HEADER_HEIGHT, fill = "surface" }, cells)
end

local function data_row(snapshot, row, index)
  local cells = {
    ui.text(row.id .. "-label", row.label, {
      size = 11, height = ROW_HEIGHT, width = label_width(snapshot), ink = "text", pad = 10,
    }),
  }
  for column, value in ipairs(row.values or {}) do
    ui.push(cells, ui.rule(row.id .. "-rule-" .. column, { width = 1, height = ROW_HEIGHT, ink = "line" }))
    local format = (snapshot.column_formats and snapshot.column_formats[column]) or row.format
    local text = cell_text(value, format)
    local negative = type(value) == "number" and value < 0
    ui.push(cells, ui.text(row.id .. "-" .. column, text, {
      size = 11,
      height = ROW_HEIGHT,
      width = CELL_WIDTH,
      align = "right",
      pad = 10,
      ink = row.ink or (negative and "danger" or "text"),
    }))
  end
  local band = (index % 2 == 0) and "surface" or nil
  return ui.row(row.id, { height = ROW_HEIGHT, fill = band }, cells)
end

local function grid(snapshot)
  local children = { column_header(snapshot) }
  for index, row in ipairs(snapshot.rows or {}) do
    ui.push(children, data_row(snapshot, row, index))
    ui.push(children, ui.rule("grid-rule-" .. index, { height = 1, width = grid_width(snapshot), ink = "line" }))
  end
  return ui.column("sheet-grid", { width = grid_width(snapshot) }, children)
end

local function rail(snapshot)
  local children = {
    ui.text("rail-title", "ASSUMPTIONS", { size = 9, height = 20, ink = "muted" }),
    ui.rule("rail-rule", { height = 1, width = 210, ink = "line" }),
  }
  for index, item in ipairs(snapshot.assumptions or {}) do
    ui.push(children, ui.text("rail-label-" .. index, item.label, {
      size = 9, height = 14, ink = "muted", width = 210,
    }))
    ui.push(children, ui.text("rail-value-" .. index, item.value, {
      size = 12, height = 22, ink = "primary", width = 210,
    }))
  end
  ui.push(children, ui.rule("rail-rule-2", { height = 1, width = 210, ink = "line" }))
  ui.push(children, ui.text("rail-action", "Recompute writes a calc record", {
    size = 9,
    height = 24,
    ink = "muted",
  }))
  return ui.column("sheet-rail", { width = 210 }, children)
end

function M.render(ctx)
  local context = ctx or {}
  local width = grid_width(context) + 24 + 210
  local rows = #(context.rows or {})
  local height = 54 + 10 + 1 + 10 + HEADER_HEIGHT + rows * (ROW_HEIGHT + 1) + 6
  return ui.column("sheet-root", { width = width, height = height, gap = 10 }, {
    header(context),
    ui.rule("header-rule", { height = 1, width = width, ink = "line" }),
    ui.row("sheet-body", { flex_grow = 1, gap = 24 }, {
      grid(context),
      rail(context),
    }),
  })
end

return M
