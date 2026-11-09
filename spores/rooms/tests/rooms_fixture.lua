-- Headless check for the Rooms view. Run from spores/rooms:
--   lua tests/rooms_fixture.lua
--
-- The fixture proves the snapshot -> scene path without a compositor: every
-- node must draw inside the box it claimed, screens are host resources, every
-- control registers a host-owned hit region, and the widget and fullscreen
-- modes stay distinct from the room.

local source = assert(io.open("../../sdk/ui/ui.lua", "rb"), "ui library missing")
local ui = assert(load(source:read("*a"), "ui"))()
source:close()
_G.ui = ui
ui.debug = true
_G.module = {
  inputs = { width = 1200, height = 1800 },
  theme = { title = 28, large = 22, body = 16, small = 13, micro = 10, radius = 10 },
}

local drawn = { rects = {}, texts = {}, buttons = {}, hosts = {} }
_G.canvas = {
  rect = function(spec) drawn.rects[#drawn.rects + 1] = spec end,
  text = function(spec) drawn.texts[#drawn.texts + 1] = spec end,
  host_resource = function(spec) drawn.hosts[#drawn.hosts + 1] = spec end,
}
_G.widget = { button = function(spec) drawn.buttons[#drawn.buttons + 1] = spec end }

local function reset()
  for _, key in ipairs({ "rects", "texts", "buttons", "hosts" }) do
    drawn[key] = {}
  end
end

local function has_text(value)
  for _, node in ipairs(drawn.texts) do
    if node.text == value then return true end
  end
  return false
end

local function count_buttons(action)
  local count = 0
  for _, button in ipairs(drawn.buttons) do
    if button.action == action then count = count + 1 end
  end
  return count
end

local function find_button(action)
  for _, button in ipairs(drawn.buttons) do
    if button.action == action then return button end
  end
  return nil
end

local function unique_control_ids()
  local seen, total = {}, 0
  for _, button in ipairs(drawn.buttons) do
    if not seen[button.id] then
      seen[button.id] = true
      total = total + 1
    end
  end
  return total
end

local view = assert(loadfile("ui/rooms.lua"))()

-- Room mode: stats, people, audio, screens and conversation together.
view.render(view.fixture())

local room_counts = ("%d rects, %d texts, %d controls, %d host surfaces"):format(
  #drawn.rects, #drawn.texts, #drawn.buttons, #drawn.hosts)

assert(has_text("FPL Rooms"), "workspace header missing")
assert(has_text("ROOMS"), "room list heading missing")
assert(has_text("Studio") and has_text("Workshop") and has_text("Listening room"),
  "room rows missing")
assert(has_text("voice · 4 · 2 new · 2 VR"), "room list should show the Holodeck count")
assert(has_text("Transmog + Quest"), "selected room heading missing")
assert(has_text("joined · 4 members · 2 in Holodeck"), "Holodeck twin missing")

-- Stats are computed from members and buses, not a fixture stats table.
assert(has_text("In room") and has_text("Speaking"), "stats strip missing")
assert(has_text("Speech bus") and has_text("Program bus"), "bus stats missing")
assert(has_text("2 in VR"), "VR stat delta missing")
assert(has_text("now") and has_text("publishers"), "stat units missing")
assert(has_text("40 ms") and has_text("lossless"), "bus stat details missing")

assert(has_text("agent · addressed only"), "agent state missing")
assert(has_text("bot · program · buffered"), "music bot state missing")
assert(has_text("playing") and has_text("speaking"), "activity state missing")
assert(has_text("AUDIO"), "audio bus heading missing")
assert(has_text("speech") and has_text("program"), "audio classes missing")
assert(has_text("40 ms · 3 publishing"), "speech bus buffer missing")
assert(has_text("250 ms · 1 publishing"), "program bus buffer missing")
assert(has_text("lossless on LAN"), "lossless state missing")
assert(has_text("♪ Voyager") and has_text("Daft Punk"), "now playing missing")
assert(has_text("CONVERSATION"), "conversation heading missing")
assert(has_text("Message the room"), "composer missing")
assert(has_text("Ask JARVIS"), "agent address control missing")

-- Screens are host surfaces, and each has a way into fullscreen.
assert(has_text("SCREENS"), "screens heading missing")
assert(has_text("Transmog / fixie") and has_text("Quest capture"), "screen titles missing")
assert(#drawn.hosts == 2, "expected two screen placements")
assert(drawn.hosts[1].resource == "share:current", "first screen alias")
assert(drawn.hosts[2].resource == "quest:camera", "second screen alias")
assert(count_buttons("fullscreen-screen") == 2, "fullscreen control per screen")

assert(count_buttons("join-room") == 3, "expected one join control per room")
assert(count_buttons("new-room") == 1, "new room control missing")
assert(count_buttons("enter-holodeck") == 1, "Holodeck control missing")
assert(count_buttons("address-agent") == 1, "address agent control missing")
assert(count_buttons("share-surface") == 1, "share control missing")
assert(count_buttons("exit-fullscreen") == 0, "room mode is not fullscreen")
assert(unique_control_ids() == #drawn.buttons, "controls need distinct scene ids")

local holodeck = find_button("enter-holodeck")
assert(holodeck.body and holodeck.body.space == "studio", "space id missing")
local focus = find_button("fullscreen-screen")
assert(focus.body and focus.body.id == "share-current", "screen id missing")

-- Widget mode: a compact card with no room list, participant grid or host surface.
reset()
local widget = view.fixture()
widget.view = { mode = "widget" }
view.render(widget)
assert(has_text("Studio"), "widget room name missing")
assert(has_text("4 in room · 1 speaking · 2 in VR"), "widget stats missing")
assert(has_text("speech 3 · program 1 · lossless"), "widget bus stats missing")
assert(has_text("2 screens · Transmog / fixie"), "widget screen summary missing")
assert(has_text("♪ Voyager — Daft Punk"), "widget now playing missing")
assert(has_text("Open room"), "widget open control missing")
assert(has_text("ROOMS") == false, "widget must not render the room list")
assert(has_text("CONVERSATION") == false, "widget must not render the conversation")
assert(#drawn.hosts == 0, "widget must not place screen surfaces")
assert(count_buttons("open-room") == 1, "widget open action missing")
assert(count_buttons("fullscreen-screen") == 1, "widget fullscreen action missing")
assert(unique_control_ids() == #drawn.buttons, "widget controls need distinct scene ids")

reset()
local widget_no_screens = view.fixture()
widget_no_screens.view = { mode = "widget" }
widget_no_screens.room.screens = {}
view.render(widget_no_screens)
assert(has_text("2 screens · Transmog / fixie") == false, "empty widget screen summary")
assert(count_buttons("share-surface") == 1, "widget share fallback missing")

-- Fullscreen mode: one screen, a way back, and nothing else competing for space.
reset()
local full = view.fixture()
full.view = { mode = "room", focus = "share-current" }
view.render(full)
assert(has_text("Back to room"), "fullscreen back control missing")
assert(has_text("Transmog / fixie") and has_text("shared by James"), "fullscreen header missing")
assert(#drawn.hosts == 1, "fullscreen should place exactly one surface")
assert(drawn.hosts[1].resource == "share:current", "fullscreen surface alias")
assert(drawn.hosts[1].id == "focus-screen", "fullscreen surface id")
assert(count_buttons("exit-fullscreen") == 1, "exit fullscreen action missing")
assert(count_buttons("join-room") == 0, "fullscreen must not render the room list")
assert(has_text("CONVERSATION") == false, "fullscreen must not render the conversation")

reset()
local stale = view.fixture()
stale.view = { mode = "room", focus = "gone" }
view.render(stale)
assert(#drawn.hosts == 2, "a stale focus should fall back to the room")

-- An empty room must still render: an explicit empty state, no screens, and no
-- failure when there are no members or buses.
reset()
local empty = view.fixture()
empty.room.screens = {}
empty.room.messages = {}
empty.room.members = {}
empty.room.buses = {}
empty.room.space = nil
view.render(empty)
assert(#drawn.hosts == 0, "a room with no screens must not place a host surface")
assert(has_text("No messages yet."), "empty conversation should say so")
assert(has_text("No screens shared."), "empty screens should say so")
assert(has_text("joined · 0 members"), "empty room member count missing")
assert(has_text("AUDIO") == false, "a room with no buses must not draw the audio section")

-- A directory with no selected room renders the lobby, or a widget invitation.
reset()
local lobby = view.fixture()
lobby.room = nil
view.render(lobby)
assert(has_text("No room selected"), "a directory with no room should render a lobby")
assert(#drawn.hosts == 0, "a lobby must not place a host surface")
assert(count_buttons("join-room") == 3, "room list should remain usable in the lobby")

reset()
local widget_lobby = view.fixture()
widget_lobby.room = nil
widget_lobby.view = { mode = "widget" }
view.render(widget_lobby)
assert(has_text("No room selected."), "widget lobby should say so")
assert(count_buttons("open-rooms") == 1, "widget lobby open control missing")

print("rooms: ok (" .. room_counts .. "; widget, fullscreen, empty room and lobby ok)")
