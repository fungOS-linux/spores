-- Fixture-driven gallery for the unified fungos.ui dashboard nodes.
--
-- The library owns layout, tokens and scene primitives. This view owns only the
-- demo composition and fixture data; a live Spore swaps the snapshot without
-- changing any of the geometry below.

local ui = assert(_G.ui or require("fungos.ui"), "ui library not composed")

local M = { name = "gallery", version = "0.1.0" }

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
      local tail = math.max(0, 1 - (index - spike_at) / 10)
      y = y + spike * tail
    end
    points[#points + 1] = { x = index, y = math.max(0, y) }
  end
  return points
end

local function fixture()
  return {
    stats = {
      { label = "Throughput", value = 1240, unit = " req/s", delta = "+8.4%", status = "live" },
      { label = "P95 latency", value = 238, unit = " ms", delta = "+12.0%", delta_ink = "warning", status = "warn" },
      { label = "Error rate", value = 0.42, decimals = 2, unit = "%", delta = "-0.06%", delta_ink = "success", status = "ok" },
    },
    gauge = {
      title = "CPU pressure",
      value = 76,
      unit = "%",
      thresholds = { { value = 70, ink = "warning" }, { value = 90, ink = "danger" } },
      status = "warn",
    },
    table = {
      title = "Services",
      subtitle = "last 15m",
      status = "ok",
      columns = {
        { key = "service", label = "Service", width = 2.2 },
        { key = "p95", label = "P95", width = 1, align = "right" },
        { key = "errors", label = "Errors", width = 1, align = "right" },
        { key = "status", label = "Status", width = 1.4, kind = "status" },
      },
      rows = {
        { service = "checkout", p95 = "238 ms", errors = "0.42%", status = "warn" },
        { service = "catalog", p95 = "164 ms", errors = "0.08%", status = "ok" },
        { service = "auth", p95 = "412 ms", errors = "1.80%", status = "crit" },
        { service = "search", p95 = "190 ms", errors = "0.21%", status = "ok" },
      },
    },
    graph = {
      title = "Topology",
      subtitle = "sites and peers · positions are data",
      nodes = {
        { id = "site:home", label = "home", x = 0.12, y = 0.78, ink = "info", state = "live", rail = true, halo = true, label_position = "left" },
        { id = "site:lab", label = "lab", x = 0.12, y = 0.40, ink = "success", state = "live", rail = true, halo = true, label_position = "left" },
        { id = "p:canvas", label = "canvas", x = 0.42, y = 0.78, ink = "info", state = "live", halo = true, label_position = "below" },
        { id = "p:soil", label = "soil", x = 0.62, y = 0.78, ink = "info", state = "live", halo = true, label_position = "below" },
        { id = "p:spore", label = "spore", x = 0.82, y = 0.78, ink = "muted", state = "stale", label_position = "below" },
        { id = "p:pi", label = "pi", x = 0.42, y = 0.40, ink = "success", state = "live", halo = true, label_position = "below" },
        { id = "p:edge", label = "edge", x = 0.62, y = 0.40, ink = "muted", state = "stale", label_position = "below" },
      },
      edges = {
        { source = "site:home", target = "p:canvas", ink = "info", state = "live" },
        { source = "site:home", target = "p:soil", ink = "info", state = "live" },
        { source = "site:home", target = "p:spore", ink = "muted", state = "stale" },
        { source = "site:lab", target = "p:pi", ink = "success", state = "live" },
        { source = "site:lab", target = "p:edge", ink = "muted", state = "stale" },
        { source = "p:canvas", target = "p:soil", ink = "info", state = "live" },
      },
      legend = {
        { ink = "success", label = "live" },
        { ink = "muted", label = "stale" },
      },
    },
    chart = {
      title = "Request latency",
      subtitle = "p50 / p95 / p99 · incident replay",
      status = "live",
      y_min = 0,
      y_max = 520,
      unit = " ms",
      threshold = { value = 300, label = "SLO 300ms" },
      selection = { x0 = 12, x1 = 18 },
      series = {
        { label = "p50", ink = "success", points = series(23, 70, 12, 5, 0) },
        { label = "p95", ink = "warning", points = series(23, 210, 40, 6, 2, 14, 60) },
        { label = "p99", ink = "danger", points = series(23, 300, 55, 5, 14, 90) },
      },
    },
  }
end

function M.render(snapshot)
  snapshot = snapshot or {}
  if not snapshot.stats then snapshot = fixture() end
  local stats = {}
  for index, item in ipairs(snapshot.stats or {}) do
    stats[index] = ui.stat {
      id = "stat-" .. index,
      label = item.label,
      value = item.value,
      unit = item.unit,
      decimals = item.decimals,
      delta = item.delta,
      delta_ink = item.delta_ink,
      status = item.status,
      action = "inspect-stat",
      body = { metric = item.label },
      h = 152,
    }
  end

  ui.present(
    ui.column {
      h = 2400,
      gap = "md",
      pad = "md",
      children = {
        ui.text("Dashboard primitives", { style = "title", ink = "text" }),
        ui.text("panel · stat · gauge · table · chart · thresholds · selection", { style = "caption", ink = "muted" }),
        ui.row {
          widths = { 1 / 3, 1 / 3, 1 / 3 },
          gap = "sm",
          children = stats,
        },
        ui.row {
          widths = { 0.34, 0.66 },
          gap = "sm",
          children = {
            ui.gauge({ id = "gauge-cpu", title = snapshot.gauge and snapshot.gauge.title, value = snapshot.gauge and snapshot.gauge.value, unit = snapshot.gauge and snapshot.gauge.unit, thresholds = snapshot.gauge and snapshot.gauge.thresholds, status = snapshot.gauge and snapshot.gauge.status, action = "inspect-gauge", h = 240 }),
            ui.table({ id = "services", title = snapshot.table and snapshot.table.title, subtitle = snapshot.table and snapshot.table.subtitle, status = snapshot.table and snapshot.table.status, columns = snapshot.table and snapshot.table.columns, rows = snapshot.table and snapshot.table.rows, row_action = "open-service", row_key = "service", h = 240 }),
          },
        },
        ui.graph(snapshot.graph.nodes, snapshot.graph.edges, {
          id = "topology",
          title = snapshot.graph.title,
          subtitle = snapshot.graph.subtitle,
          status = "live",
          legend = snapshot.graph.legend,
          node_action = "inspect-node",
          h = 520,
        }),
        ui.chart(snapshot.chart and snapshot.chart.series or {}, {
          id = "latency",
          title = snapshot.chart and snapshot.chart.title,
          subtitle = snapshot.chart and snapshot.chart.subtitle,
          status = snapshot.chart and snapshot.chart.status,
          y_min = snapshot.chart and snapshot.chart.y_min,
          y_max = snapshot.chart and snapshot.chart.y_max,
          unit = snapshot.chart and snapshot.chart.unit,
          threshold = snapshot.chart and snapshot.chart.threshold,
          selection = snapshot.chart and snapshot.chart.selection,
          legend_action = "toggle-series",
          plot_action = "select-range",
          h = 1100,
        }),
      },
    },
    theme()
  )
end

return M
