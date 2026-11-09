-- Fleet: the mycelium environment on one surface.
--
-- A pure projection. The snapshot arrives classified, ordered and laid out by
-- `spore-fleet`; this file decides only what the wall looks like. It reads no
-- clock, no socket and no file, so the same render runs in a preview, a browser
-- and Canvas without a second opinion about which peer is live.

local ui = require("fungos.ui")

local M = { name = "fleet", version = "0.1.0" }

-- The surface size a host does not tell us. A host that passes `module.viewport`
-- overrides it; the browser host does, and so does Canvas.
local W, H = 1200, 900

local DEFAULT_THEME = {
  title = 28, large = 22, body = 16, small = 13, micro = 10, radius = 10,
}

local function theme()
  return (_G.module and _G.module.theme) or DEFAULT_THEME
end

local STATUS_INK = {
  live = "success",
  ok = "success",
  degraded = "warning",
  warn = "warning",
  stale = "danger",
  crit = "danger",
  unknown = "muted",
}

local function status_ink(value)
  return STATUS_INK[tostring(value or "")] or "muted"
end

local function columns_of(spec)
  local out = {}
  for index, column in ipairs((spec and spec.columns) or {}) do
    out[index] = {
      key = column.key,
      label = column.label or column.key,
      width = column.width or 1,
      align = column.align,
      kind = column.kind,
    }
  end
  return out
end

local function table_node(spec, props)
  props.columns = columns_of(spec)
  props.rows = (spec and spec.rows) or {}
  return ui.table(props)
end

-- `ui.viewport` is a setter in this library, not a getter: the host's size has to
-- be read here and then handed to the one-pass layout. Reading `module.viewport`
-- as well as `module.inputs` covers Canvas (inputs) and the preview/browser host
-- (viewport), and the fallback is the authoring size.
local function use_viewport()
  local host = _G.module and (_G.module.viewport or _G.module.inputs)
  W = tonumber(host and host.width) or W
  H = tonumber(host and host.height) or H
  ui.viewport(W, H)
end

function M.render(snapshot)
  snapshot = snapshot or {}
  use_viewport()
  local pad = 20
  local gap = 16
  -- Everything below the masthead and the stat row shares what is left. Fixed
  -- pixel heights would overflow a short window and leave a tall one half empty;
  -- the wall gets whatever surface the host opened.
  local rest = math.max(240, H - 64 - 96 - 24 - pad * 2 - gap * 4)
  -- Heights are computed from the surface, but a layout height has to be a
  -- multiple of the 4px base: the library refuses anything else, and a card that
  -- rounded itself would be a second spacing scale.
  local function quantize(value) return math.max(4, math.floor(value / 4) * 4) end
  local graph_h = quantize(rest * 0.57)
  local bottom_h = quantize(rest - graph_h)
  local gauge_h = math.max(84, quantize(bottom_h * 0.36))

  local counts = snapshot.counts or {}
  local stats = {}
  for index, item in ipairs(counts) do
    stats[index] = ui.stat {
      id = "fleet-stat-" .. index,
      label = item.label,
      value = item.value,
      unit = item.unit,
      decimals = item.decimals,
      delta = item.delta,
      delta_ink = item.delta_ink or status_ink(item.status),
      status = item.status,
      status_ink = item.status_ink,
      h = 96,
      action = "fleet-inspect-count",
      body = { metric = item.label },
    }
  end

  local graph = snapshot.graph or {}
  local graph_props = {
    id = "fleet-graph",
    title = "Mesh topology",
    subtitle = "sites and peers observed by mycelium",
    status = graph.status,
    status_ink = status_ink(snapshot.status),
    h = graph_h,
    legend = graph.legend,
    caption = graph.caption,
    node_action = "fleet-inspect-peer",
    empty = "no peers have been observed yet",
  }
  -- The producer names the resource; the host confirms it can place one. A
  -- browser preview and an older Canvas render the marks instead.
  local inputs = _G.module and _G.module.inputs
  if graph.resource and inputs and inputs.graph_surface then
    graph_props.resource = graph.resource
  end

  local footer = snapshot.footer
    or ("fleet.myc · " .. tostring(snapshot.captured or ""))

  ui.present(
    ui.column {
      h = H,
      pad = pad,
      gap = gap,
      children = {
        ui.row {
          widths = { 0.62, 0.38 },
          gap = "sm",
          children = {
            ui.column {
              gap = "none",
              children = {
                ui.text(snapshot.title or "FLEET", { style = "title", ink = "text" }),
                ui.text(snapshot.subtitle or "mycelium · fungOS",
                  { style = "caption", ink = "muted" }),
              },
            },
            ui.row {
              align = "end",
              gap = "sm",
              children = {
                ui.text("●", { style = "caption", ink = status_ink(snapshot.status) }),
                ui.text(tostring(snapshot.status or "unknown"),
                  { style = "caption", ink = status_ink(snapshot.status) }),
                ui.text("·", { style = "caption", ink = "muted" }),
                ui.text(tostring(snapshot.captured or ""),
                  { style = "caption", ink = "muted" }),
              },
            },
          },
        },
        ui.row {
          widths = { 0.25, 0.25, 0.25, 0.25 },
          gap = "sm",
          children = stats,
        },
        ui.row {
          widths = { 0.66, 0.34 },
          gap = "md",
          children = {
            ui.graph(graph.nodes or {}, graph.edges or {}, graph_props),
            table_node(snapshot.attention, {
              id = "fleet-attention",
              title = "Attention",
              subtitle = "what is not answering, and what disagrees",
              status = snapshot.status,
              status_ink = status_ink(snapshot.status),
              h = graph_h,
              row_h = 20,
              zebra = false,
              row_action = "fleet-inspect-peer",
              row_key = "host",
            }),
          },
        },
        ui.row {
          widths = { 0.56, 0.44 },
          gap = "md",
          children = {
            table_node(snapshot.sites, {
              id = "fleet-sites",
              title = "Sites",
              subtitle = "peers grouped by the site they enrolled in",
              status = tostring(#(snapshot.sites and snapshot.sites.rows or {})) .. " sites",
              h = bottom_h,
              row_h = 20,
              row_action = "fleet-filter-site",
              row_key = "site",
            }),
            ui.column {
              gap = "sm",
              children = {
                ui.gauge {
                  id = "fleet-release",
                  title = snapshot.health and snapshot.health.title or "Release coverage",
                  value = snapshot.health and snapshot.health.value or 0,
                  max = snapshot.health and snapshot.health.max or 1,
                  unit = " peers",
                  -- Six segments on a twenty-peer fleet: each filled segment is
                  -- a node, and the wall budget is a node budget before it is a
                  -- taste budget.
                  segments = 6,
                  thresholds = snapshot.health and snapshot.health.thresholds,
                  status = snapshot.health and snapshot.health.status or "warn",
                  status_ink = status_ink(snapshot.health and snapshot.health.status),
                  h = gauge_h,
                  action = "fleet-inspect-release",
                },
                table_node(snapshot.facts, {
                  id = "fleet-mesh",
                  title = "Mesh",
                  h = math.max(80, quantize(bottom_h - gauge_h - 12)),
                  row_h = 20,
                  zebra = false,
                  row_action = "fleet-inspect-mesh",
                  row_key = "fact",
                }),
              },
            },
          },
        },
        ui.text(tostring(footer), { style = "caption", ink = "muted" }),
      },
    },
    theme()
  )
end

return M
