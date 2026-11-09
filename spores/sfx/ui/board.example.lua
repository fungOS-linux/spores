-- Snapshot fixture for ui/board.lua and for `tools/compose_sfx.lua`.
--
-- It is the same shape `snapshot()` returns: built-ins plus two sample pads, a
-- live room, and one deliberate failure (`ops` has no content id) so the
-- unavailable state is reviewed rather than only described.

local KEYS = {
  "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=",
  "q", "w", "e", "r", "t", "y", "u", "i", "o", "p", "[", "]",
}

local BUILTINS = {
  { id = "blip", label = "Blip", category = "ui", duration_ms = 80 },
  { id = "ping", label = "Ping", category = "ui", duration_ms = 140 },
  { id = "pop", label = "Pop", category = "ui", duration_ms = 90 },
  { id = "tick", label = "Tick", category = "ui", duration_ms = 22 },
  { id = "coin", label = "Coin", category = "alert", duration_ms = 165 },
  { id = "chime", label = "Chime", category = "alert", duration_ms = 720 },
  { id = "rise", label = "Rise", category = "alert", duration_ms = 240 },
  { id = "drop", label = "Drop", category = "alert", duration_ms = 240 },
  { id = "alarm", label = "Alarm", category = "alert", duration_ms = 700 },
  { id = "zap", label = "Zap", category = "fx", duration_ms = 180 },
  { id = "thud", label = "Thud", category = "fx", duration_ms = 240 },
  { id = "noise", label = "Noise", category = "fx", duration_ms = 140 },
}

local pads = {}
for index, pad in ipairs(BUILTINS) do
  pads[index] = {
    id = pad.id, label = pad.label, category = pad.category,
    source = "builtin", available = true, reason = nil,
    duration_ms = pad.duration_ms, selected = pad.id == "pop",
    room = "current", key = KEYS[index],
  }
end
pads[#pads + 1] = {
  id = "intro", label = "Intro sting", category = "sample",
  source = "sample", available = true, reason = nil, duration_ms = nil,
  selected = false, room = "current", key = KEYS[#pads + 1],
  path = "sfx/intro.mp3",
  content_id = "blake3:0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0",
}
pads[#pads + 1] = {
  id = "ops", label = "Ops, we have a problem", category = "sample",
  source = "sample", available = false, reason = "missing blake3 content id",
  duration_ms = nil, selected = false, room = "home-pi", key = KEYS[#pads + 1],
  path = "sfx/ops.mp3", content_id = "",
}

local rules = {
  { kind = "email.delta.v1", pad = "chime", label = "Chime", enabled = true, available = true, gain = 1, room = nil },
  { kind = "chat.message.v1", pad = "pop", label = "Pop", enabled = true, available = true, gain = 1, room = nil },
  { kind = "notify.publish.v1", pad = "ping", label = "Ping", enabled = true, available = true, gain = 1, room = nil },
  { kind = "call.incoming.v1", pad = "alarm", label = "Alarm", enabled = true, available = true, gain = 1, room = "home-pi" },
  { kind = "ci.failed.v1", pad = "thud", label = "Thud", enabled = true, available = true, gain = 1, room = nil },
  { kind = "timer.elapsed.v1", pad = "rise", label = "Rise", enabled = false, available = true, gain = 1, room = nil },
}

local enabled = 0
for _, rule in ipairs(rules) do if rule.enabled then enabled = enabled + 1 end end

return {
  title = "SFX",
  theme = { title = 28, large = 22, body = 15, small = 12, micro = 9, radius = 9 },
  counts = { pads = #pads, rules = #rules, enabled = enabled, live_rooms = 2, issues = 1 },
  mapping = { version = 1, scope = "synced", generation = 4, gain = 0.85, room = "current", error = nil },
  status = {
    output = "48 kHz stereo",
    opened_room = "home-pi",
    current_room = "home-pi",
    last_pad = "pop",
    last_kind = "chat.message.v1",
    last_error = nil,
    bind = nil,
    selected = "pop",
    audio_error = nil,
  },
  rooms = {
    { name = "local", label = "This machine", kind = "local", live = true, principal = "ajmwagar", current = false },
    { name = "home-pi", label = "Home Pi", kind = "remote", live = true, principal = "ajmwagar", current = true },
    { name = "lab", label = "Lab stereo", kind = "remote", live = false, principal = nil, current = false },
  },
  pads = pads,
  rules = rules,
  issues = {
    { level = "error", subject = "sample ops", message = "content id \"\" is not blake3:<64 hex>" },
  },
}
