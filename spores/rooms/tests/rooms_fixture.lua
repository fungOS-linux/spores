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
    if button.action == action or button.id == action then count = count + 1 end
  end
  return count
end

local function count_controls(prefix)
  local count = 0
  for _, button in ipairs(drawn.buttons) do
    if tostring(button.id):sub(1, #prefix) == prefix then count = count + 1 end
  end
  return count
end

local function find_button(action)
  for _, button in ipairs(drawn.buttons) do
    if button.action == action or button.id == action then return button end
  end
  return nil
end

local function reset_module()
  _G.module.event = nil
  _G.module.view = nil
  _G.module.select = nil
end

local function activate(target)
  _G.module.event = { kind = "activate", target_id = target }
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

-- A control outside the 8x8 scene is not a styling preference: Canvas rejects
-- the whole module. Keep this cheap invariant at every render so a taller
-- column, a longer label, or a new panel cannot silently push a button off the
-- surface.
local function assert_scene_bounds()
  for _, button in ipairs(drawn.buttons) do
    assert(button.x >= -4.001 and button.x + button.width <= 4.001,
      "control escapes horizontally: " .. tostring(button.id))
    assert(button.y <= 4.001 and button.y - button.height >= -4.001,
      "control escapes vertically: " .. tostring(button.id))
  end
  -- A full-width card may bleed a few pixels for its own edge; that is a
  -- decorative rect, not content Canvas rejects. Text origins and controls are
  -- the meaningful bounds check.
  for _, label in ipairs(drawn.texts) do
    local x, y = label.translation[1], label.translation[2]
    assert(x >= -4.001 and x <= 4.001, "text starts outside horizontally")
    assert(y >= -4.001 and y <= 4.001, "text starts outside vertically")
  end
  for _, host in ipairs(drawn.hosts) do
    local x, y = host.translation[1], host.translation[2]
    local w, h = host.scale[1], host.scale[2]
    assert(x >= -4.001 and x + w <= 4.001, "host surface escapes horizontally")
    assert(y <= 4.001 and y - h >= -4.001, "host surface escapes vertically")
  end
end

local viewport_width, viewport_height = ui.viewport_size()
assert(viewport_width == 1200 and viewport_height == 1800,
  "declared inputs should supply the fallback viewport")

_G.module.viewport = { width = 1600, height = 900 }
viewport_width, viewport_height = ui.viewport_size()
assert(viewport_width == 1600 and viewport_height == 900,
  "module.viewport should win over declared inputs")
_G.module.viewport = nil

local view = assert(loadfile("ui/rooms.lua"))()

-- Room mode: stats, people, audio, screens and conversation together.
view.render(view.fixture())
assert_scene_bounds()

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
assert(has_text("now"), "stat units missing")
assert(has_text("40 ms") and has_text("lossless"), "bus stat details missing")

assert(has_text("addressed only"), "agent state missing")
assert(has_text("buffered"), "music bot state missing")
assert(has_text("playing") and has_text("speaking"), "activity state missing")
assert(has_text("AUDIO"), "audio bus heading missing")
assert(has_text("speech") and has_text("program"), "audio classes missing")
assert(has_text("40 ms · 3 publishing"), "speech bus buffer missing")
assert(has_text("250 ms · 1 publishing"), "program bus buffer missing")
assert(has_text("lossless on LAN"), "lossless state missing")
assert(has_text("♪ Voyager") and has_text("Daft Punk"), "now playing missing")
assert(has_text("CHAT"), "conversation heading missing")
assert(has_text("Message the room"), "composer missing")
local has_field = false
for _, button in ipairs(drawn.buttons) do
  if button.id == "message-input" then has_field = true end
end
assert(has_field, "composer field missing")
assert(has_text("Ask JARVIS"), "agent address control missing")

-- Screens are host surfaces, and each has a way into fullscreen.
assert(has_text("SCREENS"), "screens heading missing")
assert(has_text("Transmog / fixie") and has_text("Quest capture"), "screen titles missing")
assert(#drawn.hosts == 2, "expected two screen placements")
assert(drawn.hosts[1].resource == "share:current", "first screen alias")
assert(drawn.hosts[2].resource == "quest:camera", "second screen alias")
assert(count_controls("fullscreen-") == 2, "fullscreen control per screen")

-- A host without surface import still renders a truthful, compact placeholder.
local host_resource_capability = _G.canvas.host_resource
_G.canvas.host_resource = nil
reset()
view.render(view.fixture())
assert(has_text("Preview unavailable"), "missing host preview should say so")
assert(#drawn.hosts == 0, "without host import no surface should be placed")
assert_scene_bounds()
_G.canvas.host_resource = host_resource_capability
reset()
view.render(view.fixture())
assert_scene_bounds()

-- The live Canvas surface can span two displays. Host-surface previews are
-- taller than the fallback placeholders, so exercise that branch at the wide
-- viewport too; the composer must still land inside the scene.
_G.module.viewport = { width = 3822, height = 1182 }
reset()
view.render(view.fixture())
assert(#drawn.hosts == 2, "wide room should keep both screen surfaces")
assert_scene_bounds()
_G.module.viewport = nil
reset()
view.render(view.fixture())
assert_scene_bounds()

assert(count_controls("join-studio") == 1 and count_controls("join-workshop") == 1
  and count_controls("join-listening") == 1, "expected one join control per room")
assert(count_buttons("new-room") == 1, "new room control missing")
assert(count_buttons("enter-holodeck") == 1, "Holodeck control missing")
assert(count_buttons("address-agent") == 1, "address agent control missing")
assert(count_buttons("share-surface") == 1, "share control missing")
assert(count_buttons("exit-fullscreen") == 0, "room mode is not fullscreen")
assert(unique_control_ids() == #drawn.buttons, "controls need distinct scene ids")

local holodeck = find_button("enter-holodeck")
assert(holodeck.body and holodeck.body.space == "studio", "space id missing")
local focus = find_button("fullscreen-share-current")
assert(focus.body and focus.body.id == "share-current", "screen id missing")
assert(has_text("THIS MAC"), "the local call panel is missing")
assert(has_text("Join call"), "the local join control is missing")
assert(count_buttons("join-call") == 1, "join-call control missing")
for _, button in ipairs(drawn.buttons) do
  assert(button["local"] == true, "fixture controls must not publish undeclared actions")
  assert(button.action == nil, "local controls must omit their action route")
end

-- The local call bridge: the view paints the click optimistically and the host
-- bridge starts the call from the scene it can observe.
reset_module()
local selected = nil
_G.module.select = function(id) selected = id end
_G.module.view = {}
activate("join-call")
reset()
view.render(view.fixture())
assert(selected == "call:connected", "join-call should select connected")
assert(has_text("Mute mic") and has_text("Leave call"), "connected controls missing")

_G.module.event = nil
_G.module.view.selected = selected
activate("toggle-mic")
reset()
view.render(view.fixture())
assert(selected == "call:muted", "mic toggle should select muted")
local mute_control = find_button("toggle-mic")
assert(mute_control and mute_control.label == "Unmute mic",
  "the view should paint the requested mute state for the host bridge")

_G.module.event = nil
_G.module.view.selected = selected
activate("toggle-mic")
reset()
view.render(view.fixture())
assert(selected == "call:connected", "unmute should select connected")
assert(find_button("toggle-mic").label == "Mute mic", "unmuted state should offer mute")

_G.module.event = nil
_G.module.view.selected = selected
activate("leave-call")
reset()
view.render(view.fixture())
assert(selected == "call:idle", "leave should select idle")
assert(has_text("Join call"), "idle state should offer join again")

-- Fullscreen is local too: the view remembers its own choice across the
-- re-render the click causes, without a host round trip.
_G.module.event = nil
_G.module.view.selected = "call:connected"
activate("fullscreen-share-current")
reset()
view.render(view.fixture())
assert(has_text("Back to room"), "local fullscreen control missing")
assert(#drawn.hosts == 1 and drawn.hosts[1].id == "focus-screen", "local focus missing")
assert((selected or ""):find("focus:share-current", 1, true), "focus should be remembered")

_G.module.event = nil
_G.module.view.selected = selected
activate("exit-fullscreen")
reset()
view.render(view.fixture())
assert(has_text("CHAT"), "exit-fullscreen should return to the room")
assert(has_text("Back to room") == false, "room mode must not keep the fullscreen header")

reset_module()

-- Widget mode: a compact card with no room list, participant grid or host surface.
reset()
local widget = view.fixture()
widget.view = { mode = "widget" }
view.render(widget)
assert_scene_bounds()
assert(has_text("Studio"), "widget room name missing")
assert(has_text("4 in room · 1 speaking · 2 in VR"), "widget stats missing")
assert(has_text("speech 3 · program 1 · lossless"), "widget bus stats missing")
assert(has_text("2 screens · Transmog / fixie"), "widget screen summary missing")
assert(has_text("♪ Voyager — Daft Punk"), "widget now playing missing")
assert(has_text("Open room"), "widget open control missing")
assert(has_text("ROOMS") == false, "widget must not render the room list")
assert(has_text("CHAT") == false, "widget must not render the conversation")
assert(#drawn.hosts == 0, "widget must not place screen surfaces")
assert(count_buttons("open-room") == 1, "widget open action missing")
assert(count_controls("fullscreen-") == 1, "widget fullscreen action missing")
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
assert_scene_bounds()
assert(has_text("Back to room"), "fullscreen back control missing")
assert(has_text("Transmog / fixie") and has_text("shared by James · Studio"),
  "fullscreen header missing")
assert(#drawn.hosts == 1, "fullscreen should place exactly one surface")
assert(drawn.hosts[1].resource == "share:current", "fullscreen surface alias")
assert(drawn.hosts[1].id == "focus-screen", "fullscreen surface id")
assert(count_buttons("exit-fullscreen") == 1, "exit fullscreen action missing")
assert(count_controls("join-") == 0, "fullscreen must not render the room list")
assert(has_text("CHAT") == false, "fullscreen must not render the conversation")

-- Fullscreen without surface import keeps the stage and the way back, instead
-- of drawing an empty rectangle.
_G.canvas.host_resource = nil
reset()
view.render(full)
assert(has_text("Preview unavailable"), "fullscreen fallback should say so")
assert(#drawn.hosts == 0, "fullscreen fallback places no host surface")
assert_scene_bounds()
_G.canvas.host_resource = host_resource_capability

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
assert_scene_bounds()
assert(#drawn.hosts == 0, "a room with no screens must not place a host surface")
assert(has_text("The room is quiet"), "empty conversation should say so")
assert(has_text("Nothing shared yet"), "empty screens should say so")
assert(has_text("joined · 0 members"), "empty room member count missing")
assert(has_text("AUDIO") == false, "a room with no buses must not draw the audio section")

-- A directory with no selected room renders the lobby, or a widget invitation.
reset()
local lobby = view.fixture()
lobby.room = nil
view.render(lobby)
assert_scene_bounds()
assert(has_text("No room selected"), "a directory with no room should render a lobby")
assert(#drawn.hosts == 0, "a lobby must not place a host surface")
assert(count_controls("join-studio") == 1 and count_controls("join-workshop") == 1
  and count_controls("join-listening") == 1, "room list should remain usable in the lobby")

reset()
local widget_lobby = view.fixture()
widget_lobby.room = nil
widget_lobby.view = { mode = "widget" }
view.render(widget_lobby)
assert(has_text("No room selected."), "widget lobby should say so")
assert(count_buttons("open-rooms") == 1, "widget lobby open control missing")

print("rooms: ok (" .. room_counts .. "; widget, fullscreen, empty room and lobby ok)")
