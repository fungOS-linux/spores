# What it shows

`gallery` renders the shared dashboard primitives against one fixture: `ui.stat`,
`ui.gauge`, `ui.table`, and `ui.chart`. It exists so a change to the component
vocabulary can be seen and checked without a live metrics backend, and so a
visual difference is a deliberate change to a component rather than a change in
data.

The ChartGPU panel is the portable fallback for the five-panel layout: ordinary
charts with LTTB-decimated series, not the resident-stream GPU renderer.

# What it cannot do

It owns composition, not data or authority. No network, no records, no process,
no secrets, no raw filesystem, no clock. Everything on screen comes from the
bundle.
