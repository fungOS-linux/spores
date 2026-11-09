-- Fixture-driven Rooms view.
--
-- Canvas owns layout, pixels and hit regions. This view owns composition only:
-- it receives one flat snapshot and returns one tree. Room state, media and
-- transport live in the host; see soil `docs/rooms.md`.
--
-- Three surfaces share this file:
--   room       the full room: stats, people, audio, screens, conversation
--   widget     a compact room card for a workspace or Dock
--   fullscreen one shared screen, with a way back to the room
--
-- The same room has a Holodeck twin. This view is the flat presenter; the
-- `space` id and the member list are shared, only the presenter differs.

local ui = assert(_G.ui or require("fungos.ui"), "ui library not composed")

local M = { name = "rooms", version = "0.1.0" }

local DEFAULT_THEME = {
  title = 28, large = 22, body = 16, small = 13, micro = 10, radius = 10,
}

local function theme()
  return (_G.module and _G.module.theme) or DEFAULT_THEME
end

local function viewport_height()
  return (_G.module and _G.module.inputs and _G.module.inputs.height) or 1800
end

-- The fixture is the reviewed example. A live module produces the same shape
-- from `soil:room`; no geometry below depends on where the data came from.
local function fixture()
  return {
    revision = 42,
    view = { mode = "room", focus = nil },
    workspace = { name = "FPL Rooms", presence = "connected" },
    self = { member = "person:avery", name = "Avery", room = "studio", mic = "on" },
    rooms = {
      { id = "studio", name = "Studio", kind = "voice", members = 4, unread = 2, vr = 2 },
      { id = "workshop", name = "Workshop", kind = "voice", members = 1, unread = 0, vr = 0 },
      { id = "listening", name = "Listening room", kind = "voice", members = 2, unread = 0, vr = 0 },
    },
    room = {
      id = "studio",
      name = "Studio",
      topic = "Transmog + Quest",
      joined = true,
      space = "studio",
      buses = {
        { class = "speech", buffer_ms = 40, publishers = 3 },
        { class = "program", buffer_ms = 250, publishers = 1, lossless = true,
          now_playing = { title = "Voyager", artist = "Daft Punk" } },
      },
      members = {
        { id = "person:avery", name = "Avery", kind = "human", presence = "active",
          speaking = true, mic = "on", presenter = "canvas" },
        { id = "person:james", name = "James", kind = "human", presence = "active",
          speaking = false, mic = "on", presenter = "holodeck" },
        { id = "agent:jarvis", name = "JARVIS", kind = "agent", presence = "active",
          speaking = false, agent_state = "addressed", presenter = "canvas" },
        { id = "bot:hifi", name = "Hi-Fi", kind = "bot", presence = "active",
          speaking = false, audio = "program", buffered = true, state = "playing",
          presenter = "holodeck" },
      },
      screens = {
        { id = "share-current", owner = "James", title = "Transmog / fixie",
          kind = "window", surface = "share:current" },
        { id = "quest-camera", owner = "Avery", title = "Quest capture",
          kind = "camera", surface = "quest:camera" },
      },
      messages = {
        { id = "m1", from = "James", text = "Rotation is live on the table.", mine = false },
        { id = "m2", from = "JARVIS", text = "Two parts attached; the fixie frame is current.", mine = false },
        { id = "m3", from = "Avery", text = "Sharing the transom next.", mine = true },
      },
    },
  }
end

local function role_line(member)
  if member.kind == "agent" then
    if member.agent_state == "addressed" then return "agent · addressed only" end
    return "agent · idle"
  elseif member.kind == "bot" then
    local audio = member.audio or "program"
    if member.buffered then return "bot · " .. audio .. " · buffered" end
    return "bot · " .. audio
  elseif member.kind == "bridge" then
    return "bridged · E2EE ends at bridge"
  end
  local presence = member.presence or "offline"
  local mic = member.mic == "off" and "mic off" or "mic on"
  return presence .. " · " .. mic
end

local function activity(member)
  if member.state == "playing" then return "playing", "info" end
  if member.speaking then return "speaking", "success" end
  return "quiet", "muted"
end

local function member_tile(member)
  local label, ink = activity(member)
  return ui.card({
    h = 104,
    pad = "sm",
    children = {
      ui.row({
        gap = "xs",
        children = {
          ui.icon("dot", { ink = member.speaking and "success" or "muted", size = "small" }),
          ui.text(member.name, { style = "subheading" }),
        },
      }),
      ui.text(role_line(member), { style = "caption", ink = "muted" }),
      ui.text(label, { style = "caption", ink = ink }),
    },
  })
end

local function member_grid(members)
  local rows = {}
  for index = 1, #members, 4 do
    local chunk = {}
    for item = index, math.min(index + 3, #members) do
      chunk[#chunk + 1] = member_tile(members[item])
    end
    local widths = {}
    for item = 1, #chunk do widths[item] = 1 / #chunk end
    rows[#rows + 1] = ui.row({ widths = widths, gap = "sm", children = chunk })
  end
  return rows
end

local function scene_id(value)
  return (tostring(value):gsub("[^%w%-_]", "-")):sub(1, 48)
end

local function room_row(room, selected)
  local live = room.kind == "voice" and room.members > 0
  local detail = ("%s · %d"):format(room.kind, room.members)
  if room.unread and room.unread > 0 then
    detail = detail .. (" · %d new"):format(room.unread)
  end
  if room.vr and room.vr > 0 then
    detail = detail .. (" · %d VR"):format(room.vr)
  end
  return ui.card({
    h = 96,
    pad = "sm",
    children = {
      ui.row({
        gap = "xs",
        children = {
          ui.icon("dot", { ink = live and "success" or "muted", size = "small" }),
          ui.text(room.name, { style = "subheading" }),
        },
      }),
      ui.text(detail, { style = "caption", ink = "muted" }),
      ui.button(selected and "Here" or "Join", {
        id = "join-" .. scene_id(room.id),
        action = "join-room",
        body = { id = room.id },
        quiet = selected,
      }),
    },
  })
end

local function room_list(snapshot)
  local children = {
    ui.text("ROOMS", { style = "caption", ink = "muted" }),
  }
  for _, room in ipairs(snapshot.rooms or {}) do
    local selected = snapshot.self and snapshot.self.room == room.id
      and snapshot.room and snapshot.room.joined
    children[#children + 1] = room_row(room, selected)
  end
  children[#children + 1] = ui.spacer({ size = "sm" })
  children[#children + 1] = ui.button("New room", { action = "new-room", quiet = true })
  return ui.column({ gap = "sm", children = children })
end

local function conversation(room)
  local children = {
    ui.text("CONVERSATION", { style = "caption", ink = "muted" }),
  }
  local messages = room.messages or {}
  if #messages == 0 then
    children[#children + 1] = ui.text("No messages yet.", { style = "caption", ink = "muted" })
  end
  for _, message in ipairs(messages) do
    children[#children + 1] = ui.card({
      pad = "sm",
      children = {
        ui.row({
          widths = { 0.22, 0.78 },
          gap = "sm",
          children = {
            ui.text(message.from, {
              style = "caption", ink = message.mine and "primary" or "muted",
            }),
            ui.text(message.text, { style = "body" }),
          },
        }),
      },
    })
  end
  return ui.column({ gap = "xs", children = children })
end

local function bus_of(room, class)
  for _, bus in ipairs(room.buses or {}) do
    if bus.class == class then return bus end
  end
  return nil
end

local function count(members, predicate)
  local total = 0
  for _, member in ipairs(members) do
    if predicate(member) then total = total + 1 end
  end
  return total
end

local function stats(room)
  local members = room.members or {}
  local speech = bus_of(room, "speech")
  local program = bus_of(room, "program")
  local vr = count(members, function(member) return member.presenter == "holodeck" end)
  local speaking = count(members, function(member) return member.speaking == true end)
  local function status(count) return count > 0 and "live" or "idle" end
  local speech_publishers = speech and speech.publishers or 0
  local program_publishers = program and program.publishers or 0
  return ui.row({
    widths = { 0.25, 0.25, 0.25, 0.25 },
    gap = "sm",
    children = {
      ui.stat({
        label = "In room", value = #members,
        delta = ("%d in VR"):format(vr), status = status(#members),
      }),
      ui.stat({
        label = "Speaking", value = speaking, unit = "now",
        status = status(speaking),
      }),
      ui.stat({
        label = "Speech bus", value = speech_publishers, unit = "publishers",
        delta = speech and ("%d ms"):format(speech.buffer_ms) or nil,
        status = status(speech_publishers),
      }),
      ui.stat({
        label = "Program bus", value = program_publishers, unit = "publishers",
        delta = program and program.lossless and "lossless" or nil,
        status = status(program_publishers),
      }),
    },
  })
end

local function bus_card(bus)
  local children = {
    ui.text(bus.class, { style = "subheading" }),
    ui.text(("%d ms · %d publishing"):format(bus.buffer_ms, bus.publishers), {
      style = "caption", ink = "muted",
    }),
  }
  if bus.lossless then
    children[#children + 1] = ui.text("lossless on LAN", { style = "caption", ink = "success" })
  end
  return ui.card({ h = 92, pad = "sm", children = children })
end

local function audio(room)
  local buses = room.buses or {}
  if #buses == 0 then return nil end
  local widths = {}
  for index = 1, #buses do widths[index] = 1 / #buses end
  local cards = {}
  for index, bus in ipairs(buses) do cards[index] = bus_card(bus) end
  local children = {
    ui.text("AUDIO", { style = "caption", ink = "muted" }),
    ui.row({ widths = widths, gap = "sm", children = cards }),
  }
  for _, bus in ipairs(buses) do
    if bus.now_playing then
      local playing = bus.now_playing
      children[#children + 1] = ui.card({
        pad = "sm",
        children = {
          ui.row({
            widths = { 0.65, 0.35 },
            gap = "sm",
            children = {
              ui.text("♪ " .. playing.title, { style = "subheading" }),
              ui.text(playing.artist, { style = "caption", ink = "muted" }),
            },
          }),
          ui.text(("buffered %d ms · lossless on LAN"):format(bus.buffer_ms), {
            style = "caption", ink = "success",
          }),
        },
      })
    end
  end
  return ui.column({ gap = "xs", children = children })
end

local function screen_tile(screen)
  return ui.card({
    pad = "sm",
    children = {
      ui.row({
        widths = { 0.7, 0.3 },
        gap = "sm",
        children = {
          ui.text(screen.title, { style = "subheading" }),
          ui.text(screen.owner, { style = "caption", ink = "muted" }),
        },
      }),
      ui.host_surface(screen.surface, { id = scene_id(screen.id), h = 200 }),
      ui.row({
        gap = "sm",
        children = {
          ui.text(screen.kind or "screen", { style = "caption", ink = "muted" }),
          ui.button("Fullscreen", {
            id = "fullscreen-" .. scene_id(screen.id),
            action = "fullscreen-screen", body = { id = screen.id },
          }),
        },
      }),
    },
  })
end

local function screen_grid(room)
  local screens = room.screens or {}
  local children = {
    ui.text("SCREENS", { style = "caption", ink = "muted" }),
  }
  if #screens == 0 then
    children[#children + 1] = ui.text("No screens shared.", { style = "caption", ink = "muted" })
    return ui.column({ gap = "xs", children = children })
  end
  for index = 1, #screens, 2 do
    local row = {}
    for item = index, math.min(index + 1, #screens) do
      row[#row + 1] = screen_tile(screens[item])
    end
    local widths = {}
    for item = 1, #row do widths[item] = 1 / #row end
    children[#children + 1] = ui.row({ widths = widths, gap = "sm", children = row })
  end
  return ui.column({ gap = "xs", children = children })
end

local function composer()
  return ui.bar({
    border = true,
    pad = "sm",
    children = {
      ui.row({
        widths = { 0.48, 0.26, 0.26 },
        gap = "sm",
        children = {
          ui.text("Message the room", { style = "body", ink = "muted" }),
          ui.button("Ask JARVIS", {
            action = "address-agent", body = { member = "agent:jarvis" },
          }),
          ui.button("Share", { action = "share-surface" }),
        },
      }),
    },
  })
end

local function room_panel(snapshot)
  local room = snapshot.room
  local title = room.topic
  if title == nil or title == "" then title = room.name or room.id end
  local space = room.space or room.id
  local joined = ("joined · %d members"):format(#(room.members or {}))
  local vr = count(room.members or {}, function(member)
    return member.presenter == "holodeck"
  end)
  if vr > 0 then
    joined = joined .. (" · %d in Holodeck"):format(vr)
  end
  local children = {
    ui.text(title, { style = "title" }),
    ui.row({
      gap = "md",
      children = {
        ui.text(joined, { style = "caption", ink = "muted" }),
        ui.button("Open in Holodeck", {
          action = "enter-holodeck", body = { space = space },
        }),
      },
    }),
    ui.fill({ h = 4, ink = "muted", opacity = 0.18 }),
    stats(room),
  }
  for _, row in ipairs(member_grid(room.members or {})) do
    children[#children + 1] = row
  end
  local audio_column = audio(room)
  if audio_column then children[#children + 1] = audio_column end
  children[#children + 1] = screen_grid(room)
  children[#children + 1] = conversation(room)
  children[#children + 1] = composer()
  return ui.column({ gap = "sm", children = children })
end

local function focused_screen(snapshot)
  local id = snapshot.view and snapshot.view.focus
  if not id then return nil end
  for _, screen in ipairs((snapshot.room and snapshot.room.screens) or {}) do
    if screen.id == id then return screen end
  end
  return nil
end

local function fullscreen_view(snapshot, screen)
  local height = math.max(400, math.floor((viewport_height() - 300) / 4) * 4)
  return ui.column({
    gap = "sm",
    children = {
      ui.bar({
        border = true,
        children = {
          ui.row({
            gap = "md",
            children = {
              ui.button("Back to room", { action = "exit-fullscreen" }),
              ui.text(screen.title, { style = "subheading" }),
              ui.text("shared by " .. screen.owner, { style = "caption", ink = "muted" }),
              ui.text(snapshot.room.name or snapshot.room.id, { style = "caption", ink = "muted" }),
            },
          }),
        },
      }),
      ui.card({
        pad = "sm",
        children = {
          ui.host_surface(screen.surface, { id = "focus-screen", h = height }),
        },
      }),
    },
  })
end

local function widget(snapshot)
  local room = snapshot.room
  if not room then
    return ui.column({
      gap = "sm",
      children = {
        ui.text("Rooms", { style = "title" }),
        ui.text("No room selected.", { style = "caption", ink = "muted" }),
        ui.button("Open rooms", { action = "open-rooms" }),
      },
    })
  end
  local members = room.members or {}
  local speech = bus_of(room, "speech")
  local program = bus_of(room, "program")
  local vr = count(members, function(member) return member.presenter == "holodeck" end)
  local speaking = count(members, function(member) return member.speaking == true end)
  local screens = room.screens or {}
  local first = screens[1]
  local children = {
    ui.row({
      widths = { 0.55, 0.45 },
      gap = "sm",
      children = {
        ui.text(room.name or room.topic or room.id, { style = "title" }),
        ui.text((snapshot.workspace and snapshot.workspace.name) or "Rooms", {
          style = "caption", ink = "muted",
        }),
      },
    }),
    ui.text(("%d in room · %d speaking · %d in VR"):format(#members, speaking, vr), {
      style = "caption",
    }),
    ui.text(("speech %d · program %d%s"):format(
      speech and speech.publishers or 0,
      program and program.publishers or 0,
      program and program.lossless and " · lossless" or ""), {
      style = "caption", ink = "muted",
    }),
  }
  if first then
    children[#children + 1] = ui.text(("%d screens · %s"):format(#screens, first.title), {
      style = "caption", ink = "info",
    })
  end
  if program and program.now_playing then
    children[#children + 1] = ui.text("♪ " .. program.now_playing.title .. " — " .. program.now_playing.artist, {
      style = "caption", ink = "info",
    })
  end
  children[#children + 1] = ui.row({
    widths = { 0.5, 0.5 },
    gap = "sm",
    children = {
      ui.button("Open room", { action = "open-room", body = { id = room.id } }),
      first and ui.button("Fullscreen", {
        id = "fullscreen-" .. scene_id(first.id),
        action = "fullscreen-screen", body = { id = first.id },
      }) or ui.button("Share", { action = "share-surface" }),
    },
  })
  return ui.card({ pad = "sm", children = children })
end

local function lobby_panel()
  return ui.column({
    gap = "sm",
    children = {
      ui.text("No room selected", { style = "title" }),
      ui.text("Pick a room to see who is here.", { style = "caption", ink = "muted" }),
    },
  })
end

local function header(snapshot)
  local workspace = snapshot.workspace or { name = "Rooms", presence = "offline" }
  local ink = workspace.presence == "connected" and "success" or "warning"
  return ui.bar({
    border = true,
    children = {
      ui.row({
        gap = "md",
        children = {
          ui.text(workspace.name, { style = "title" }),
          ui.icon("dot", { ink = ink, size = "small" }),
          ui.text(workspace.presence, { style = "caption", ink = ink }),
          ui.text(("you: %s"):format(snapshot.self and snapshot.self.name or "unknown"), {
            style = "caption", ink = "muted",
          }),
        },
      }),
    },
  })
end

local function room_view(snapshot)
  return ui.column({ gap = "sm", children = {
    header(snapshot),
    ui.row({
      widths = { 0.3, 0.7 },
      gap = "lg",
      children = {
        room_list(snapshot),
        snapshot.room and room_panel(snapshot) or lobby_panel(),
      },
    }),
  } })
end

function M.render(snapshot)
  snapshot = snapshot or fixture()
  local mode = snapshot.view and snapshot.view.mode or "room"
  local focused = focused_screen(snapshot)
  local tree
  if focused then
    tree = fullscreen_view(snapshot, focused)
  elseif mode == "widget" then
    tree = widget(snapshot)
  else
    tree = room_view(snapshot)
  end
  ui.render(tree, theme())
  return tree
end

M.fixture = fixture

return M
