-- ChartGPU visual language as ordinary fungos.ui nodes.
--
-- The native ChartGPU view draws polylines from resident GPU buffers. Canvas's
-- Lua scene has rect/text nodes, so this fixture keeps the same five-panel
-- language with decimated column marks. LTTB runs before the marks are emitted,
-- preserving endpoints and dominant spikes while keeping the scene small.

local ui = assert(_G.ui or require("fungos.ui"), "ui library not composed")

local M = { name = "chartgpu", version = "0.1.0" }

local DEFAULT_THEME = {
  title = 28, large = 22, body = 16, small = 13, micro = 10, radius = 10,
}

local function theme()
  return (_G.module and _G.module.theme) or DEFAULT_THEME
end

local function series(count, base, amplitude, period, phase, spike_at, spike)
  local points = {}
  for index = 0, count do
    local y = base + amplitude * math.sin((index + phase) / period)
    if spike_at and index >= spike_at then
      local tail = math.max(0, 1 - (index - spike_at) / 12)
      y = y + spike * tail
    end
    points[#points + 1] = { x = index, y = math.max(0, y) }
  end
  return points
end

local function fixture()
  return {
    latency = {
      p50 = series(119, 70, 18, 7, 0),
      p95 = series(119, 210, 55, 6, 3, 74, 120),
      p99 = series(119, 300, 75, 5, 5, 74, 220),
    },
    throughput = { requests = series(119, 960, 150, 9, 1, 78, -430) },
    resources = {
      memory = series(119, 48, 10, 13, 2),
      cpu = series(119, 44, 16, 5, 4),
    },
    errors = {
      e4xx = series(119, 2.2, 1.0, 4, 1),
      e5xx = series(119, 0.4, 0.25, 8, 2, 74, 8),
    },
    connections = {
      active = series(119, 92, 22, 7, 3),
      waiting = series(119, 12, 6, 5, 1, 78, 44),
    },
  }
end

local function panel(id, spec)
  return ui.chart(spec.series or {}, {
    id = id,
    title = spec.title,
    subtitle = spec.subtitle,
    status = "live",
    y_min = spec.y_min or 0,
    y_max = spec.y_max,
    unit = spec.unit,
    y_decimals = spec.y_decimals,
    x_decimals = 0,
    columns = 8,
    caps = false,
    h = 820,
    pad = "sm",
  })
end

function M.render(snapshot)
  snapshot = snapshot or {}
  if not snapshot.latency then snapshot = fixture() end
  local latency = snapshot.latency or {}
  local throughput = snapshot.throughput or {}
  local resources = snapshot.resources or {}
  local errors = snapshot.errors or {}
  local connections = snapshot.connections or {}

  ui.render(
    ui.column {
      h = 1800,
      gap = "sm",
      pad = "sm",
      children = {
        ui.text("ChartGPU · decimated", { style = "title", ink = "text" }),
        ui.text("five panels · 120 samples per series · LTTB to 8 columns · one rect per mark",
          { style = "caption", ink = "muted" }),
        ui.row {
          widths = { 0.62, 0.38 },
          gap = "sm",
          children = {
            panel("latency", {
              title = "Response latency",
              subtitle = "P50 / P95 / P99 · incident replay",
              y_max = 700,
              unit = " ms",
              series = {
                { label = "P99", ink = "danger", points = latency.p99 },
                { label = "P95", ink = "warning", points = latency.p95 },
                { label = "P50", ink = "success", points = latency.p50 },
              },
            }),
            panel("throughput", {
              title = "Throughput",
              subtitle = "req/s · degraded by connection pressure",
              y_max = 1400,
              series = {
                { label = "Requests", ink = "primary", points = throughput.requests },
              },
            }),
          },
        },
        ui.row {
          widths = { 0.34, 0.33, 0.33 },
          gap = "sm",
          children = {
            panel("resources", {
              title = "CPU / memory",
              subtitle = "% · leak onset to recovery",
              y_max = 100,
              unit = "%",
              series = {
                { label = "Memory", ink = "primary", points = resources.memory },
                { label = "CPU", ink = "warning", points = resources.cpu },
              },
            }),
            panel("errors", {
              title = "Errors",
              subtitle = "errors/s · 4xx baseline / 5xx cascade",
              y_max = 24,
              unit = "/s",
              y_decimals = 1,
              series = {
                { label = "5xx", ink = "danger", points = errors.e5xx },
                { label = "4xx", ink = "warning", points = errors.e4xx },
              },
            }),
            panel("connections", {
              title = "Connections",
              subtitle = "conns · pool saturation",
              y_max = 220,
              series = {
                { label = "Active", ink = "info", points = connections.active },
                { label = "Waiting", ink = "danger", points = connections.waiting },
              },
            }),
          },
        },
      },
    },
    theme()
  )
end

return M
