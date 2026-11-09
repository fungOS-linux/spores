-- Amber: a terminal shell. Low contrast on purpose; the accent is the only
-- thing allowed to be bright.
return {
  name = "Amber",
  -- The scale the preview simulates. Canvas on the wall runs a 1.5x theme, and a
  -- preview drawn at anything else answers questions about itself. A host still
  -- publishes its own; this is what a theme file contributes.
  type = { micro = 15, small = 18, body = 24, large = 27 },
  inks = {
    background = "#0a0805",
    surface = "#171208",
    surface_raised = "#221a0c",
    border = "#3a2a10",
    text = "#f0b45a",
    primary = "#ffd79a",
    muted = "#a2762f",
    accent = "#ff9f1c",
    info = "#e0a94a",
    success = "#c8b04a",
    warning = "#ffc45c",
    danger = "#e2603c",
  },
}
