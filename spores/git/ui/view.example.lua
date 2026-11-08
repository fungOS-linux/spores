-- Fixture for ui/review.lua.
--
-- Rows are real review-state issues from the tracker, with long titles shortened
-- the way the row clips them. One is marked `merged` so the land path renders;
-- the rest are still open. The bottom row has no pull request, which is its own
-- decision: a review item nobody opened a PR for is not landable, it is lost.
return {
  title = "REVIEW",
  selected_id = "federation-f1-assertion-cache",
  counts = { waiting = 7, merged = 1, no_pr = 1 },
  projects = {
    { id = "agent", label = "agent", count = 7, selected = true },
    { id = "marbles", label = "marbles", count = 1 },
    { id = "op-trk-live-76311", label = "op-trk-live-76311", count = 1 },
  },
  rows = {
    {
      id = "federation-f1-assertion-cache",
      title = "marbles federation F1: assertion cache with named freshness",
      project = "marbles", marble = "marbles-4f2", age = "13d",
      pr = { repo = "FuturePresentLabs/marbles", number = 26, state = "merged",
             url = "https://github.com/FuturePresentLabs/marbles/pull/26" },
    },
    {
      id = "pm-pane-falsealive",
      title = "pm locks+bus: pane %0 false-alive (dead PM looks live, all",
      project = "agent", marble = "agent-9xk", age = "13d",
      pr = { repo = "ajmwagar/agentsmith", number = 47, state = "open",
             url = "https://github.com/ajmwagar/agentsmith/pull/47" },
    },
    {
      id = "governor-load-truth",
      title = "governor: truthful host-tier load/tasks probe (tasks=8192",
      project = "agent", marble = "agent-7pw", age = "13d",
      pr = { repo = "ajmwagar/agentsmith", number = 48, state = "open",
             url = "https://github.com/ajmwagar/agentsmith/pull/48" },
    },
    {
      id = "sampler-token-liveness",
      title = "sampler liveness: working only on positive token delta",
      project = "agent", marble = "agent-2mv", age = "13d",
      pr = { repo = "ajmwagar/agentsmith", number = 49, state = "open",
             url = "https://github.com/ajmwagar/agentsmith/pull/49" },
    },
    {
      id = "motion-guard-noise",
      title = "motion-guard: stop the unlocked/stale-lock blocker loop",
      project = "agent", marble = "agent-6c1", age = "13d",
      pr = { repo = "ajmwagar/agentsmith", number = 50, state = "open",
             url = "https://github.com/ajmwagar/agentsmith/pull/50" },
    },
    {
      id = "lifecycle-fixture-wipes-live-registry",
      title = "pi-lifecycle fixture run can prune the production pi-tmux",
      project = "agent", marble = "agent-3jz", age = "13d",
      pr = { repo = "ajmwagar/agentsmith", number = 51, state = "open",
             url = "https://github.com/ajmwagar/agentsmith/pull/51" },
    },
    {
      id = "fungos-wasm-host-ble-streams-signing",
      title = "fungOS wasm host: absorb Colmi BLE reversal + Unibus streams",
      project = "agent", marble = "agent-5rt", age = "13d",
      pr = { repo = "ajmwagar/mycelium", number = 2, state = "open",
             url = "https://github.com/ajmwagar/mycelium/pull/2" },
    },
    {
      id = "op-trk-live-76311-364345000-8pid",
      title = "live bead",
      project = "op-trk-live-76311", marble = "op-trk-8pid", age = "55d",
    },
  },
}
