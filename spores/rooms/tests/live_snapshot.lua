-- Headless check for the live record stream the Rooms host sends through a
-- Canvas binder. The module never sees JSON; it sees printable chunks.
--
--   lua tests/live_snapshot.lua

_G.module = { inputs = {} }
local text = table.concat({
  "N|7",
  "W|FPL Rooms|connected",
  "S|avery|Avery|studio|on",
  "V|room:studio",
  "C|connected|udp-opus|48 kHz mono|20|123|42|960|0.02|false|",
  "R|studio|Studio|studio|true",
  "B|speech|40|2|false",
  "M|avery|Avery|human|active|true|on|canvas|speech||",
  "M|jarvis|JARVIS|agent|active|false|off|canvas|speech|addressed||",
  "L|studio|Studio|voice|2|0|0",
}, "~")

-- Split across two chunks to exercise the join, not just the parser.
local split = math.floor(#text / 2)
_G.module.inputs["snapshot-1"] = text:sub(1, split)
_G.module.inputs["snapshot-2"] = text:sub(split + 1)

local snapshot = assert(dofile("ui/snapshot.lua"), "live snapshot missing")
assert(snapshot.live == true, "live flag missing")
assert(snapshot.revision == 7, "revision missing")
assert(snapshot.workspace.name == "FPL Rooms" and snapshot.workspace.presence == "connected")
assert(snapshot.self.name == "Avery" and snapshot.self.room == "studio")
assert(snapshot.view.selected == "room:studio")
assert(snapshot.call.state == "connected" and snapshot.call.frame_count == 42)
assert(snapshot.room.id == "studio" and snapshot.room.space == "studio")
assert(#snapshot.room.buses == 1 and snapshot.room.buses[1].class == "speech")
assert(#snapshot.room.members == 2)
assert(snapshot.room.members[1].speaking == true)
assert(snapshot.room.members[2].kind == "agent")
assert(snapshot.room.members[2].agent_state == "addressed")
assert(#snapshot.rooms == 1 and snapshot.rooms[1].name == "Studio")
print("live snapshot: ok")
