-- Graphite: the default. Ink-on-charcoal, one accent, and hairlines rather than
-- cards — the shape of a document editor's dark mode, which is what the worklog
-- is: a page you read, not a dashboard you scan.
return {
  name = "Graphite",
  -- The scale the preview simulates. Canvas on the wall runs a 1.5x theme, and a
  -- preview drawn at anything else answers questions about itself. A host still
  -- publishes its own; this is what a theme file contributes.
  type = { micro = 15, small = 18, body = 24, large = 27 },
  inks = {
    background = "#191919",
    surface = "#202020",
    surface_raised = "#262626",
    border = "#2f2f2f",
    text = "#e9e9e7",
    primary = "#ffffff",
    muted = "#9b9b9b",
    accent = "#529cca",
    info = "#7ab0d8",
    success = "#5fb27a",
    warning = "#d9a55a",
    danger = "#e0706b",
  },
}
