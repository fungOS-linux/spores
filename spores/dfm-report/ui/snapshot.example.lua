-- Fixture for `tend check`. The host owns the real report; this is the shape it
-- hands the view, with one finding of each severity so every ink path draws.
return {
  part = "bracket-v3",
  process = "machining",
  selected = 2,
  summary = { blockers = 1, warnings = 1, notes = 1 },
  findings = {
    {
      rule = "wall-thickness",
      severity = "warning",
      measured = 1.2,
      limit = 1.5,
      message = "Wall below the 1.5 mm floor for the chosen material.",
      anchor = { x = 1.2, y = 0.4 },
    },
    {
      rule = "hole-diameter",
      severity = "blocker",
      measured = 3.1,
      limit = 4.0,
      message = "Bolt hole undersized for the M4 fastener.",
      anchor = { x = 2.0, y = 0.8 },
      basis = "ISO 273",
    },
    {
      rule = "draft-angle",
      severity = "note",
      measured = 0.9,
      limit = 0.5,
      message = "Draft is generous; no change needed.",
    },
  },
}
