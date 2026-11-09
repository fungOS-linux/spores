-- Rooms snapshot -> Canvas module inputs.
--
-- Canvas caps one module text input at 4096 printable bytes. A module is also
-- loaded from a single source file, so every byte here costs against that cap.
-- Encode the flat snapshot as printable records rather than JSON: the module
-- parses one small splitter instead of carrying a whole JSON decoder.

local payload = event.payload
if type(payload) ~= "table" or type(payload.snapshot) ~= "table" then
  error("rooms-v1 requires payload.snapshot")
end
local p = payload.snapshot

local function escape(value)
  return tostring(value == nil and "" or value)
    :gsub("\\", "\\\\"):gsub("|", "\\|"):gsub("~", "\\~")
end

local function record(kind, values)
  local fields = { kind }
  for _, value in ipairs(values or {}) do fields[#fields + 1] = escape(value) end
  return table.concat(fields, "|")
end

local records = {
  record("N", { p.revision or 0 }),
  record("W", { p.workspace and p.workspace.name, p.workspace and p.workspace.presence }),
  record("S", { p.self and p.self.member, p.self and p.self.name, p.self and p.self.room, p.self and p.self.mic }),
  record("V", { p.view and p.view.selected }),
}
if p.call then
  records[#records + 1] = record("C", {
    p.call.state, p.call.route, p.call.format, p.call.frame_ms,
    p.call.pid, p.call.frame_count, p.call.received_samples, p.call.mic_rms,
    p.call.muted, p.call.error,
  })
end
if p.error then records[#records + 1] = record("E", { p.error }) end
if p.room then
  records[#records + 1] = record("R", {
    p.room.id, p.room.name, p.room.space, p.room.joined,
  })
  for _, bus in ipairs(p.room.buses or {}) do
    records[#records + 1] = record("B", {
      bus.class, bus.buffer_ms, bus.publishers, bus.lossless,
    })
  end
  for _, member in ipairs(p.room.members or {}) do
    records[#records + 1] = record("M", {
      member.id, member.name, member.kind, member.presence, member.speaking,
      member.mic, member.presenter, member.audio, member.agent_state,
      member.buffered, member.state,
    })
  end
  for _, screen in ipairs(p.room.screens or {}) do
    records[#records + 1] = record("X", {
      screen.id, screen.owner, screen.title, screen.kind, screen.surface,
    })
  end
  for _, message in ipairs(p.room.messages or {}) do
    records[#records + 1] = record("G", {
      message.id, message.from, message.text, message.mine,
    })
  end
end
for _, room in ipairs(p.rooms or {}) do
  records[#records + 1] = record("L", {
    room.id, room.name, room.kind, room.members, room.unread, room.vr,
  })
end

local text = table.concat(records, "~")
local inputs = {}
local start = 1
local index = 0
while start <= #text do
  index = index + 1
  if index > 8 then error("rooms-v1 snapshot needs more than 8 chunks") end
  local finish = math.min(start + 4095, #text)
  -- A text input must be valid UTF-8. Do not split a multi-byte character:
  -- continuation bytes are 0x80..0xBF, and a leading byte is never in that
  -- range. This avoids bitwise operators, which older Canvas Lua builds lack.
  while finish > start do
    local byte = text:byte(finish + 1) or 0
    if byte < 0x80 or byte >= 0xC0 then break end
    finish = finish - 1
  end
  inputs["snapshot-" .. index] = { type = "text", value = text:sub(start, finish) }
  start = finish + 1
end
if index == 0 then error("rooms-v1 received an empty snapshot") end

return {
  kind = "snapshot",
  widget = {
    kind = "module",
    module = "spore-dev-rooms",
    inputs = inputs,
  },
  data = {
    state = "ready",
    source = event.owner,
    provenance = "live",
  },
}
