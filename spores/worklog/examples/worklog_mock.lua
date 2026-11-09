-- Fixture snapshot for ui/worklog.lua.
--
-- Shape matches what `worklog render-lua` emits, field for field. It is written
-- by hand so a reader can see the contract without running the receiver, and
-- tests/ui_fixture.lua renders it the way Canvas would.
return {
  title = "WORKLOG",
  limit = 60,
  counts = { total = 5, today = 3 },
  sources = {
    { id = "github", label = "Github", count = 2 },
    { id = "marbles", label = "Marbles", count = 3 },
  },
  events = {
    {
      id = "github:fungOS-linux/spores#41:closed",
      kind = "merged", source = "github", actor = "ajmwagar",
      summary = "merged: Render the worklog",
      subject = "github:fungOS-linux/spores#41",
      at = 1767323045000, at_text = "2026-01-02T03:04:05Z", age = "now",
      selected = true,
      links = { { rel = "subject", locator = "github:fungOS-linux/spores#41" } },
    },
    {
      id = "marbles:fpl:7",
      kind = "closed", source = "marbles", actor = "marbles",
      summary = "closed spores/spores-42",
      subject = "marbles://fpl/spores/spores-42",
      at = 1767320000000, at_text = "2026-01-02T02:13:20Z", age = "51m",
      selected = false,
      links = { { rel = "subject", locator = "marbles://fpl/spores/spores-42" } },
    },
    {
      id = "marbles:fpl:6",
      kind = "claimed", source = "marbles", actor = "marbles",
      summary = "claimed spores/spores-42",
      subject = "marbles://fpl/spores/spores-42",
      at = 1767310000000, at_text = "2026-01-01T23:26:40Z", age = "3h",
      selected = false,
      links = { { rel = "subject", locator = "marbles://fpl/spores/spores-42" } },
    },
    {
      id = "github:fungOS-linux/spores#40:closed",
      kind = "changed", source = "github", actor = "someone",
      summary = "opened: Name the inbox Spore",
      subject = "github:fungOS-linux/spores#40",
      at = 1767300000000, at_text = "2026-01-01T20:40:00Z", age = "6h",
      selected = false,
      links = { { rel = "subject", locator = "github:fungOS-linux/spores#40" } },
    },
    {
      id = "marbles:fpl:5",
      kind = "dependency", source = "marbles", actor = "marbles",
      summary = "depends_on spores/spores-41 — blocked on the records host",
      subject = "marbles://fpl/spores/spores-41",
      at = 1767290000000, at_text = "2026-01-01T17:53:20Z", age = "1d",
      selected = false,
      links = {
        { rel = "subject", locator = "marbles://fpl/spores/spores-41" },
        { rel = "blocker", locator = "marbles://fpl/soil/soil-9" },
      },
    },
  },
}
