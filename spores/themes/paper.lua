-- Paper: a light, quiet shell. Every ink is redefined, because a view that
-- assumed "background is dark" would otherwise draw dark-on-dark.
--
-- The inks are a document's, not a dashboard's: white paper, one grey for
-- secondary surfaces and hairlines, near-black text, and colour reserved for the
-- things that are actually links or state. A view names roles, so this file is
-- what makes the whole UI read as a page rather than a panel.
return {
  name = "Paper",
  -- The scale the preview simulates. Canvas on the wall runs a 1.5x theme, and a
  -- preview drawn at anything else answers questions about itself. A host still
  -- publishes its own; this is what a theme file contributes.
  type = { micro = 15, small = 18, body = 24, large = 27 },
  inks = {
    background = "#ffffff",
    surface = "#f7f7f5",
    surface_raised = "#f1f1ef",
    border = "#ebebea",
    text = "#37352f",
    primary = "#191919",
    muted = "#9b9a97",
    accent = "#2383e2",
    info = "#337ea9",
    success = "#448361",
    warning = "#cb912f",
    danger = "#d44c47",
  },
}
