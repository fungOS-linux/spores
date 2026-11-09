-- The installed demo has one reviewed snapshot: the fixture in rooms.lua.
-- A live room host supplies the same shape as printable records instead of
-- this file. Canvas gives a module only declared text inputs, each capped at
-- 4096 bytes, so the host's binder splits the record stream into chunks.

local module = _G.module
if not module then return nil end

local parts = {}
for index = 1, 8 do
  local chunk = module.inputs and module.inputs["snapshot-" .. index]
  if type(chunk) == "string" and chunk ~= "" then
    parts[#parts + 1] = chunk
  end
end
if #parts == 0 then return nil end

local text = table.concat(parts)
if #text > 32768 then
  return { live = true, error = "room snapshot exceeds 32768 bytes" }
end

-- Records are `~`-separated and fields are `|`-separated. Both separators are
-- escaped in values, so names and chat text cannot change the shape.
local function split(value, separator)
  local out, current, escaped = {}, {}, false
  for index = 1, #value do
    local char = value:sub(index, index)
    if escaped then
      current[#current + 1] = char
      escaped = false
    elseif char == "\\" then
      escaped = true
    elseif char == separator then
      out[#out + 1] = table.concat(current)
      current = {}
    else
      current[#current + 1] = char
    end
  end
  out[#out + 1] = table.concat(current)
  return out
end

local function bool(value) return value == "true" end
local function number(value) return tonumber(value) or 0 end
local function text_or_nil(value) return value ~= "" and value or nil end

local snapshot = { live = true, rooms = {}, call = {} }
for _, record in ipairs(split(text, "~")) do
  local f = split(record, "|")
  local kind = f[1]
  if kind == "N" then
    snapshot.revision = number(f[2])
  elseif kind == "W" then
    snapshot.workspace = { name = f[2], presence = f[3] }
  elseif kind == "S" then
    snapshot.self = {
      member = text_or_nil(f[2]), name = f[3], room = text_or_nil(f[4]), mic = f[5],
    }
  elseif kind == "V" then
    snapshot.view = { selected = text_or_nil(f[2]) }
  elseif kind == "E" then
    snapshot.error = f[2]
  elseif kind == "C" then
    snapshot.call = {
      state = f[2], route = f[3], format = f[4], frame_ms = number(f[5]),
      pid = number(f[6]), frame_count = number(f[7]), received_samples = number(f[8]),
      mic_rms = number(f[9]), muted = bool(f[10]), error = text_or_nil(f[11]),
    }
  elseif kind == "R" then
    snapshot.room = {
      id = f[2], name = f[3], space = f[4], joined = bool(f[5]),
      buses = {}, members = {}, screens = {}, messages = {},
    }
  elseif kind == "B" and snapshot.room then
    snapshot.room.buses[#snapshot.room.buses + 1] = {
      class = f[2], buffer_ms = number(f[3]), publishers = number(f[4]),
      lossless = bool(f[5]),
    }
  elseif kind == "M" and snapshot.room then
    snapshot.room.members[#snapshot.room.members + 1] = {
      id = f[2], name = f[3], kind = f[4], presence = f[5], speaking = bool(f[6]),
      mic = f[7], presenter = f[8], audio = f[9], agent_state = text_or_nil(f[10]),
      buffered = bool(f[11]), state = text_or_nil(f[12]),
    }
  elseif kind == "X" and snapshot.room then
    snapshot.room.screens[#snapshot.room.screens + 1] = {
      id = f[2], owner = f[3], title = f[4], kind = f[5], surface = f[6],
    }
  elseif kind == "G" and snapshot.room then
    snapshot.room.messages[#snapshot.room.messages + 1] = {
      id = f[2], from = f[3], text = f[4], mine = bool(f[5]),
    }
  elseif kind == "L" then
    snapshot.rooms[#snapshot.rooms + 1] = {
      id = f[2], name = f[3], kind = f[4], members = number(f[5]),
      unread = number(f[6]), vr = number(f[7]),
    }
  end
end
return snapshot
