-- Graphite: the default. A dark neutral shell with one accent.
return {
  name = "Graphite",
  -- The scale the preview simulates. Canvas on the wall runs a 1.5x theme, and a
  -- preview drawn at anything else answers questions about itself. A host still
  -- publishes its own; this is what a theme file contributes.
  type = { micro = 15, small = 18, body = 24, large = 27 },
  inks = {
    background = "#0d0f14",
    surface = "#1e2430",
    surface_raised = "#262e3c",
    border = "#2a3241",
    text = "#e6ebf5",
    primary = "#f2f5fb",
    muted = "#8b96ab",
    accent = "#7aa2ff",
    info = "#7dc4e4",
    success = "#a6da95",
    warning = "#eed49f",
    danger = "#ed8796",
  },
}
