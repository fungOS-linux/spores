-- Geode: a cool shell, violet accent, cyan helper.
return {
  name = "Geode",
  -- The scale the preview simulates. Canvas on the wall runs a 1.5x theme, and a
  -- preview drawn at anything else answers questions about itself. A host still
  -- publishes its own; this is what a theme file contributes.
  type = { micro = 15, small = 18, body = 24, large = 27 },
  inks = {
    background = "#12101c",
    surface = "#1d1930",
    surface_raised = "#282243",
    border = "#332c52",
    text = "#e9e5ff",
    primary = "#ffffff",
    muted = "#9a93bd",
    accent = "#b18cff",
    info = "#7fd6f0",
    success = "#86e0b0",
    warning = "#f2cd7a",
    danger = "#f2809b",
  },
}
