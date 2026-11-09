-- Paper: a light shell. Every ink is redefined, because a view that assumed
-- "background is dark" would otherwise draw dark-on-dark.
return {
  name = "Paper",
  inks = {
    background = "#f5f1e8",
    surface = "#e9e3d5",
    surface_raised = "#ded6c4",
    border = "#cfc6b2",
    text = "#2b2a26",
    primary = "#15140f",
    muted = "#7a7466",
    accent = "#b5651d",
    info = "#2f6f8f",
    success = "#4f7a3a",
    warning = "#a0761d",
    danger = "#a33a2c",
  },
}
