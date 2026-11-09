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

-- The room is three columns. Keep the proportions in one place: the screen
-- preview reserves its aspect ratio from the middle column, and the row that
-- actually places the columns uses the same numbers.
local COLUMNS = { sidebar = 0.22, main = 0.54, rail = 0.24 }

local DEFAULT_THEME = {
  title = 28, large = 22, body = 16, small = 13, micro = 10, radius = 10,
}

local function theme()
  return (_G.module and _G.module.theme) or DEFAULT_THEME
end

local function viewport()
  return ui.viewport_size()
end

local function viewport_height()
  return select(2, viewport())
end

local function viewport_width()
  local width = viewport()
  return width
end

-- Screen cards live in the middle column, two to a row. Reserve a 16:9-ish
-- preview at the width that column actually gets, with a cap so a two-display
-- surface does not spend half its height on a thumbnail.
local function screen_preview_height()
  local tile = viewport_width() * COLUMNS.main / 2 - ui.gap.md
  return math.max(140, math.min(200, math.floor(tile * 9 / 16 / 4) * 4))
end

-- A full-height column in logical pixels. The header and gutter are measured
-- in the same units, so a wide surface gets a wide room, not a portrait page
-- stretched across it.
local function column_height()
  return math.max(420, math.floor((viewport_height() - 160) / 4) * 4)
end

-- A full-height side panel with a visible surface. `ui.bar` stacks children and
-- has no flex pass, so wrap the content in a fixed-height column: a growing
-- spacer inside the column is what pins the user card or the shared-space
-- footer to the foot of the panel.
local function side_panel(children)
  local pad = ui.gap.md
  return ui.bar({
    h = column_height(),
    pad = "md",
    ink = "surface",
    radius = "md",
    children = {
      ui.column({
        h = math.max(200, column_height() - pad * 2),
        gap = "none",
        children = children,
      }),
    },
  })
end

-- Canvas runs a module with no state between invocations. The one exception is
-- `module.select`, which lets a view remember a choice that is its own rather
-- than the Spore's: an open screen, a selected row, a local call panel. We use
-- one bounded key for both presentation choices so a click on Fullscreen does
-- not erase the call state and a call transition does not close the screen.
local function split_selection(value)
  local state = {}
  for part in tostring(value or ""):gmatch("[^;]+") do
    local key, item = part:match("^(%a+):(.*)$")
    if key == "focus" then
      state.focus = item ~= "" and item or nil
    elseif key == "call" then
      state.call = item ~= "" and item or nil
    elseif key == "room" then
      state.room = item ~= "" and item or nil
    elseif key == "leave" then
      state.leave = item == "1"
    elseif key == "request" then
      state.request = item ~= "" and item or nil
    end
  end
  return state
end

local function selection_value(state)
  local parts = {}
  if state.call then parts[#parts + 1] = "call:" .. state.call end
  if state.room then parts[#parts + 1] = "room:" .. state.room end
  if state.leave then parts[#parts + 1] = "leave:1" end
  if state.focus then parts[#parts + 1] = "focus:" .. state.focus end
  if state.request then parts[#parts + 1] = "request:" .. state.request end
  return #parts > 0 and table.concat(parts, ";") or nil
end

local function view_state(snapshot)
  local module = _G.module
  -- Canvas re-renders a module from its declared inputs, and a re-render does
  -- not carry `module.select` memory. The host echoes the selection it read
  -- from the last scene, so a live snapshot update does not close a screen or
  -- move the room selection back to the joined room.
  local selected = split_selection(snapshot and snapshot.view and snapshot.view.selected)
  if not (snapshot and snapshot.live) then
    -- A live host echoes the selection it read from the last scene, and that
    -- echo is authoritative across an input refresh. The fixture has no host,
    -- so it keeps the module's own remembered click.
    local remembered = split_selection(module and module.view and module.view.selected)
    for key, value in pairs(remembered) do selected[key] = value end
  end
  local event = module and module.event
  local changed = false
  if event and event.kind == "activate" and event.target_id then
    local target = event.target_id
    local focus = target:match("^fullscreen%-(.+)$")
    local room = target:match("^join%-(.+)$")
    if focus then
      selected.focus = focus
      changed = true
    elseif target == "exit-fullscreen" then
      selected.focus = nil
      changed = true
    elseif target == "join-call" or target == "leave-call" or target == "toggle-mic" then
      -- A live host owns the call: the next snapshot carries the real
      -- lifecycle, mute flag and error. A click leaves one bounded request
      -- marker in the scene so the host can tell a click from a persistent
      -- control. The reviewed fixture has no host, so it keeps the old
      -- optimistic click as a presentation sample.
      if snapshot and snapshot.live then
        if target == "join-call" then
          selected.request = "start"
        elseif target == "leave-call" then
          selected.request = "stop"
        elseif (snapshot.call and snapshot.call.muted) then
          selected.request = "unmute"
        else
          selected.request = "mute"
        end
        changed = true
      elseif target == "join-call" then
        selected.call = "connected"
        changed = true
      elseif target == "leave-call" then
        selected.call = "idle"
        changed = true
      else
        selected.call = selected.call == "muted" and "connected" or "muted"
        changed = true
      end
    elseif room then
      selected.room = room
      selected.leave = false
      changed = true
    elseif target == "leave-room" then
      selected.room = nil
      selected.leave = true
      changed = true
    elseif target == "enter-holodeck" and snapshot and snapshot.live then
      selected.request = "holodeck"
      changed = true
    elseif target == "address-agent" and snapshot and snapshot.live then
      selected.request = "agent"
      changed = true
    end
    if changed and module.select then module.select(selection_value(selected)) end
  end
  return selected
end

local function control(label, props)
  props = props or {}
  -- This fixture has no action controller. Local controls ask Canvas to
  -- re-render the module; the host bridge reads the rendered call controls.
  props["local"] = true
  return ui.button(label, props)
end

-- The fixture is the reviewed example. A live module produces the same shape
-- from `soil:room`; no geometry below depends on where the data came from.
local function fixture()
  return {
    revision = 42,
    view = { mode = "room", focus = nil },
    call = {
      state = "idle",
      route = "loopback",
      format = "48 kHz mono",
      frame_ms = 20,
      note = "Local mic + TTS tester; room relay later.",
    },
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
  -- The tag column already says AGENT or BOT. Repeating it here spent the
  -- narrow rail's width on a label the reader just saw.
  if member.kind == "agent" then
    if member.agent_state == "addressed" then return "addressed only" end
    return "idle"
  elseif member.kind == "bot" then
    local audio = member.audio or "program"
    if member.buffered then return "buffered" end
    return audio
  elseif member.kind == "bridge" then
    return "E2EE bridge"
  end
  local presence = member.presence or "offline"
  local mic = member.mic == "off" and "mic off" or "mic on"
  if presence == "active" then return mic end
  return presence .. " · " .. mic
end

local function activity(member)
  if member.state == "playing" then return "playing", "info" end
  if member.speaking then return "speaking", "success" end
  return "quiet", "muted"
end

local function scene_id(value)
  return (tostring(value):gsub("[^%w%-_]", "-")):sub(1, 48)
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

-- Section labels are one row, not a title plus a free-floating count. The
-- detail sits in its own column so two sections scan at the same rhythm.
local function section_title(label, detail)
  return ui.row({
    widths = { 0.84, 0.16 },
    gap = "sm",
    children = {
      ui.text(label, { style = "caption", ink = "muted" }),
      ui.text(detail or "", { style = "caption", ink = "muted" }),
    },
  })
end

-- An empty region is a place, not a missing label. A surface with one glyph
-- and one sentence says what will appear here without pretending a preview or
-- message already exists.
local function empty_card(glyph, title, detail, height)
  return ui.bar({
    h = height,
    ink = "surface_sunken",
    radius = "md",
    border = true,
    pad = "md",
    children = {
      ui.row({
        widths = { 0.08, 0.92 },
        gap = "sm",
        children = {
          ui.text(glyph, { style = "title", ink = "muted" }),
          ui.column({
            gap = "xs",
            children = {
              ui.text(title, { style = "subheading", ink = "muted" }),
              ui.text(detail, { style = "caption", ink = "muted" }),
            },
          }),
        },
      }),
    },
  })
end

local function identity_ink(member)
  if member.kind == "agent" then return "info" end
  if member.kind == "bot" or member.kind == "bridge" then return "warning" end
  return "primary"
end

-- A small round-ish identity block. Canvas has no image primitive here, so a
-- single initial is the honest fallback; the host can replace it with a real
-- avatar surface later without changing the layout.
local function avatar(name, height, ink)
  local initial = tostring(name or "?"):sub(1, 1):upper()
  ink = ink or "primary"
  return ui.bar({
    h = height,
    pad = "xs",
    ink = ink,
    opacity = 0.18,
    radius = "pill",
    children = {
      ui.text(initial, { style = "caption", ink = ink }),
    },
  })
end

local function metric(label, value, detail, ink)
  return ui.column({
    gap = "xs",
    children = {
      ui.text(label, { style = "caption", ink = "muted" }),
      ui.text(value, { style = "title", ink = ink or "text" }),
      ui.text(detail, { style = "caption", ink = "muted" }),
    },
  })
end

local function metric_row(room)
  local members = room.members or {}
  local speech = bus_of(room, "speech")
  local program = bus_of(room, "program")
  local vr = count(members, function(member) return member.presenter == "holodeck" end)
  local speaking = count(members, function(member) return member.speaking == true end)
  local speech_publishers = speech and speech.publishers or 0
  local program_publishers = program and program.publishers or 0
  return ui.row({
    widths = { 0.25, 0.25, 0.25, 0.25 },
    gap = "sm",
    children = {
      metric("In room", tostring(#members), ("%d in VR"):format(vr)),
      metric("Speaking", tostring(speaking), "now", speaking > 0 and "success" or "muted"),
      metric("Speech bus", tostring(speech_publishers),
        speech and ("%d ms"):format(speech.buffer_ms) or "idle"),
      metric("Program bus", tostring(program_publishers),
        program and (program.lossless and "lossless" or "program") or "idle"),
    },
  })
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
  return ui.bar({
    ink = selected and "primary" or "surface",
    opacity = selected and 0.16 or 0.0,
    radius = "sm",
    pad = "xs",
    children = {
      ui.row({
        widths = { 0.1, 0.64, 0.26 },
        gap = "sm",
        children = {
          ui.icon("dot", { ink = live and "success" or "muted", size = "small" }),
          ui.column({
            gap = "xs",
            children = {
              ui.text(room.name, { style = "body", ink = selected and "primary" or "text" }),
              ui.text(detail, { style = "caption", ink = "muted" }),
            },
          }),
          control(selected and "Here" or "Join", {
            id = "join-" .. scene_id(room.id),
            action = "join-room",
            body = { id = room.id },
            quiet = true,
            small = true,
          }),
        },
      }),
    },
  })
end

local function workspace_card(snapshot)
  local workspace = snapshot.workspace or {}
  return ui.bar({
    ink = "surface_sunken",
    radius = "sm",
    pad = "sm",
    children = {
      ui.row({
        widths = { 0.18, 0.82 },
        gap = "sm",
        children = {
          avatar(workspace.name or "Rooms", 32, "primary"),
          ui.column({
            gap = "xs",
            children = {
              ui.text(workspace.name or "Rooms", { style = "subheading" }),
              ui.text("Personal workspace", { style = "caption", ink = "muted" }),
            },
          }),
        },
      }),
    },
  })
end

local function text_channel(name)
  return ui.bar({
    ink = "surface",
    opacity = 0.0,
    radius = "sm",
    pad = "xs",
    children = {
      ui.row({
        widths = { 0.12, 0.88 },
        gap = "sm",
        children = {
          ui.text("#", { style = "caption", ink = "muted" }),
          ui.text(name, { style = "body", ink = "muted" }),
        },
      }),
    },
  })
end

local function sidebar(snapshot, state)
  local here = (state and not state.leave and state.room)
    or (snapshot.self and not (state and state.leave) and snapshot.self.room)
  local children = {
    workspace_card(snapshot),
    ui.spacer({ size = "sm" }),
    ui.row({
      widths = { 0.78, 0.22 },
      children = {
        ui.text("ROOMS", { style = "caption", ink = "muted" }),
        control("+", { id = "new-room", action = "new-room", quiet = true, small = true }),
      },
    }),
    ui.spacer({ size = "xs" }),
  }
  for _, room in ipairs(snapshot.rooms or {}) do
    local selected = here == room.id
      and snapshot.room and snapshot.room.joined
    children[#children + 1] = room_row(room, selected)
  end
  children[#children + 1] = ui.spacer({ size = "md" })
  children[#children + 1] = ui.fill({ h = 4, ink = "muted", opacity = 0.14 })
  children[#children + 1] = ui.spacer({ size = "sm" })
  children[#children + 1] = ui.text("TEXT", { style = "caption", ink = "muted" })
  children[#children + 1] = text_channel("general")
  children[#children + 1] = text_channel("links")
  children[#children + 1] = ui.spacer({ grow = true })
  children[#children + 1] = ui.fill({ h = 4, ink = "muted", opacity = 0.16 })
  children[#children + 1] = ui.spacer({ size = "sm" })
  local me = snapshot.self or {}
  children[#children + 1] = ui.bar({
    ink = "surface_sunken",
    radius = "sm",
    pad = "sm",
    children = {
      ui.row({
        widths = { 0.18, 0.62, 0.2 },
        gap = "sm",
        children = {
          avatar(me.name or "?", 24, "primary"),
          ui.column({
            gap = "xs",
            children = {
              ui.text(me.name or "You", { style = "body" }),
              ui.text(me.mic == "off" and "mic off" or "mic on", {
                style = "caption", ink = "muted",
              }),
            },
          }),
          ui.text("⌘", { style = "caption", ink = "muted" }),
        },
      }),
    },
  })
  return side_panel(children)
end

local function message_row(message)
  local mine = message.mine == true
  return ui.row({
    widths = { 0.08, 0.92 },
    gap = "sm",
    children = {
      avatar(message.from, 24, mine and "primary" or "muted"),
      ui.column({
        gap = "xs",
        children = {
          ui.text(message.from .. "  ·  now", {
            style = "caption", ink = mine and "primary" or "muted",
          }),
          ui.text(message.text, { style = "body" }),
        },
      }),
    },
  })
end

local function conversation(room)
  local messages = room.messages or {}
  local count_text = "no messages"
  if #messages > 0 then
    count_text = ("%d message%s"):format(#messages, #messages == 1 and "" or "s")
  end
  local children = {
    section_title("CHAT", count_text),
  }
  if #messages == 0 then
    children[#children + 1] = empty_card(
      "▤",
      "The room is quiet",
      "Messages and addressed JARVIS turns will appear here.",
      104
    )
  end
  for index, message in ipairs(messages) do
    children[#children + 1] = message_row(message)
    if index < #messages then
      children[#children + 1] = ui.fill({ h = 4, ink = "muted", opacity = 0.12 })
    end
  end
  return ui.column({ gap = "sm", children = children })
end

local function bus_line(bus)
  local children = {
    ui.row({
      widths = { 0.35, 0.65 },
      children = {
        ui.text(bus.class, { style = "body" }),
        ui.text(("%d ms · %d publishing"):format(bus.buffer_ms, bus.publishers), {
          style = "caption", ink = "muted",
        }),
      },
    }),
  }
  if bus.lossless then
    children[#children + 1] = ui.text("lossless on LAN", {
      style = "caption", ink = "success",
    })
  end
  return ui.column({ gap = "xs", children = children })
end

local function audio_rail(room)
  local buses = room.buses or {}
  local children = {
    section_title("AUDIO", ("%d buses"):format(#buses)),
  }
  for _, bus in ipairs(buses) do
    children[#children + 1] = bus_line(bus)
  end
  for _, bus in ipairs(buses) do
    if bus.now_playing then
      local playing = bus.now_playing
      children[#children + 1] = ui.bar({
        ink = "surface_sunken",
        radius = "sm",
        pad = "sm",
        children = {
          ui.text("♪ " .. playing.title, { style = "subheading" }),
          ui.text(playing.artist, { style = "caption", ink = "muted" }),
          ui.text(("buffered %d ms · lossless on LAN"):format(bus.buffer_ms), {
            style = "caption", ink = "success",
          }),
        },
      })
    end
  end
  return ui.column({ gap = "sm", children = children })
end

local function call_panel(snapshot, state)
  local call = snapshot.call or {}
  local live = snapshot.live == true
  -- The fixture can still demonstrate the local panel by remembering a click.
  -- A live host is authoritative: starting, connected, muted and failed all
  -- come from the process the host actually started.
  local mode = live and (call.state or "idle") or ((state and state.call) or "idle")
  local starting = mode == "starting"
  local failed = mode == "failed"
  local muted = mode == "muted"
  local connected = mode == "connected" or muted
  local ink = failed and "warning" or (starting and "warning" or (connected and "success" or "muted"))
  local label = failed and "failed" or (starting and "starting" or (muted and "muted" or (connected and "in call" or "idle")))
  local children = {
    ui.row({
      widths = { 0.55, 0.45 },
      children = {
        ui.text("THIS MAC", { style = "caption", ink = "muted" }),
        ui.row({
          gap = "xs",
          children = {
            ui.icon("dot", { ink = ink, size = "small" }),
            ui.text(label, { style = "caption", ink = ink }),
          },
        }),
      },
    }),
    ui.spacer({ size = "xs" }),
  }
  if state and state.request then
    local labels = {
      start = "Starting…", stop = "Stopping…", mute = "Muting…", unmute = "Unmuting…",
    }
    children[#children + 1] = control(labels[state.request] or "Applying…", {
      id = "call-request-" .. scene_id(state.request),
      action = "call-request",
      enabled = false,
      quiet = true,
      small = true,
    })
  end
  if failed then
    children[#children + 1] = ui.text(call.error or "The local call did not start.", {
      style = "body", ink = "warning",
    })
    children[#children + 1] = control("Try again", {
      id = "join-call", action = "join-call",
    })
  elseif starting then
    children[#children + 1] = ui.text("Starting microphone and speaker…", {
      style = "body", ink = "muted",
    })
    children[#children + 1] = control("Cancel", {
      id = "leave-call", action = "leave-call", quiet = true,
    })
  elseif not connected then
    children[#children + 1] = ui.text("Your mic is off until you join.", {
      style = "body", ink = "muted",
    })
    children[#children + 1] = control("Join call", {
      id = "join-call", action = "join-call",
    })
  else
    -- The rail is the narrowest column. Say what the transport is, not the
    -- full internal route name, and keep the frame size from pushing the line
    -- under the next column.
    local route = call.route == "browser-webrtc" and "webrtc" or (call.route or "loopback")
    local transport = (call.format or "48 kHz mono") .. " · " .. route
    if call.frame_ms then transport = transport .. (" · %d ms"):format(call.frame_ms) end
    children[#children + 1] = ui.text(transport, { style = "caption", ink = "success" })
    local detail = (muted and "mic muted" or (call.mic_rms and ("rms %.3f"):format(call.mic_rms) or "mic live"))
    if call.frame_count then
      detail = detail .. (" · %d frames"):format(call.frame_count)
    end
    if call.peer_count and call.peer_count > 0 then
      detail = detail .. (" · %d connected"):format(call.peer_count)
    end
    if call.pid then detail = detail .. (" · pid %d"):format(call.pid) end
    children[#children + 1] = ui.text(detail, {
      style = "caption", ink = muted and "warning" or "muted",
    })
    children[#children + 1] = ui.row({
      widths = { 0.5, 0.5 },
      gap = "sm",
      children = {
        control(muted and "Unmute mic" or "Mute mic", {
          id = "toggle-mic", action = "toggle-mic", quiet = true,
        }),
        control("Leave call", {
          id = "leave-call", action = "leave-call", quiet = true,
        }),
      },
    })
  end
  return ui.bar({
    ink = "surface_sunken",
    radius = "md",
    pad = "md",
    children = children,
  })
end

local function member_row(member)
  local label, ink = activity(member)
  local tag = nil
  local tag_ink = "muted"
  if member.presenter == "holodeck" then
    tag, tag_ink = "VR", "success"
  elseif member.kind == "agent" then
    tag, tag_ink = "AGENT", "info"
  elseif member.kind == "bot" then
    tag, tag_ink = "BOT", "warning"
  end
  return ui.row({
    widths = { 0.14, 0.86 },
    gap = "sm",
    children = {
      avatar(member.name, 24, identity_ink(member)),
      ui.column({
        gap = "xs",
        children = {
          ui.row({
            widths = { 0.7, 0.3 },
            children = {
              ui.text(member.name, { style = "body" }),
              tag and ui.text(tag, { style = "caption", ink = tag_ink })
                or ui.text("", { style = "caption" }),
            },
          }),
          ui.row({
            gap = "xs",
            children = {
              ui.icon("dot", { ink = ink, size = "small" }),
              ui.text(label, { style = "caption", ink = ink }),
              ui.text("·", { style = "caption", ink = "muted" }),
              ui.text(role_line(member), { style = "caption", ink = "muted" }),
            },
          }),
        },
      }),
    },
  })
end

local function member_rail(room)
  local members = room.members or {}
  local humans, others = {}, {}
  for _, member in ipairs(members) do
    if member.kind == "human" then
      humans[#humans + 1] = member
    else
      others[#others + 1] = member
    end
  end
  local children = {
    section_title("IN THIS ROOM", tostring(#members)),
    ui.spacer({ size = "xs" }),
  }
  for _, member in ipairs(humans) do
    children[#children + 1] = member_row(member)
  end
  if #others > 0 then
    children[#children + 1] = ui.spacer({ size = "xs" })
    children[#children + 1] = ui.text("AGENTS & BOTS", {
      style = "caption", ink = "muted",
    })
    for _, member in ipairs(others) do
      children[#children + 1] = member_row(member)
    end
  end
  return ui.column({ gap = "sm", children = children })
end

local function screen_preview(screen, id, height, compact)
  if canvas and canvas.host_resource then
    return ui.host_surface(screen.surface, { id = id, h = height })
  end
  -- A host that has not granted surface import still gets a truthful tile. The
  -- alias is shown, never guessed at or turned into a URL. Keep the technical
  -- grant name in the API doc, not in the room: the person sees a missing
  -- preview, not a capability error.
  if compact then
    return ui.bar({
      h = math.min(height, 120),
      pad = "sm",
      ink = "background",
      radius = "sm",
      border = true,
      children = {
        ui.row({
          widths = { 0.16, 0.84 },
          gap = "sm",
          children = {
            ui.text("▣", { style = "title", ink = "muted" }),
            ui.column({
              gap = "xs",
              children = {
                ui.text("Preview unavailable", { style = "body", ink = "muted" }),
                ui.text("alias · " .. tostring(screen.surface), {
                  style = "caption", ink = "muted",
                }),
              },
            }),
          },
        }),
      },
    })
  end
  local top = math.max(0, math.floor((height - 170) / 8) * 4)
  return ui.bar({
    h = height,
    pad = "sm",
    ink = "background",
    radius = "md",
    border = true,
    children = {
      ui.spacer({ size = top }),
      ui.row({
        widths = { 0.33, 0.34, 0.33 },
        children = {
          ui.spacer({}),
          ui.column({
            gap = "sm",
            children = {
              ui.text("▣", { style = "display", ink = "muted" }),
              ui.text("Preview unavailable", { style = "subheading", ink = "muted" }),
              ui.text("alias · " .. tostring(screen.surface), {
                style = "caption", ink = "muted",
              }),
            },
          }),
          ui.spacer({}),
        },
      }),
    },
  })
end

local function screen_tile(screen)
  return ui.card({
    pad = "sm",
    children = {
      ui.row({
        widths = { 0.68, 0.32 },
        gap = "sm",
        children = {
          ui.text(screen.title, { style = "subheading" }),
          ui.text(screen.owner, { style = "caption", ink = "muted" }),
        },
      }),
      screen_preview(screen, scene_id(screen.id), screen_preview_height(), true),
      ui.row({
        widths = { 0.45, 0.55 },
        gap = "sm",
        children = {
          ui.text(screen.kind or "screen", { style = "caption", ink = "muted" }),
          control("Fullscreen", {
            id = "fullscreen-" .. scene_id(screen.id),
            action = "fullscreen-screen", body = { id = screen.id },
            quiet = true, small = true,
          }),
        },
      }),
    },
  })
end

local function screen_stage(room)
  local screens = room.screens or {}
  local children = {
    section_title("SCREENS", #screens > 0 and ("%d shared"):format(#screens) or "none"),
  }
  if #screens == 0 then
    children[#children + 1] = empty_card(
      "▣",
      "Nothing shared yet",
      "A shared window or camera appears here when someone presents.",
      112
    )
    return ui.column({ gap = "sm", children = children })
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

local function composer(state)
  local children = {
    ui.row({
      widths = { 0.56, 0.21, 0.23 },
      gap = "sm",
      children = {
        ui.field("Message the room", {
          id = "message-input",
          action = "message-room",
          h = 28,
          ["local"] = true,
        }),
        control("Ask JARVIS", {
          action = "address-agent", body = { member = "agent:jarvis" },
          quiet = true, small = true,
        }),
        control("Share", { action = "share-surface", quiet = true, small = true }),
      },
    }),
  }
  if state and state.request == "agent" then
    children[#children + 1] = control("Asking JARVIS…", {
      id = "request-agent", action = "request-agent", enabled = false,
      quiet = true, small = true,
    })
  end
  return ui.bar({
    ink = "surface", radius = "md", border = true, pad = "sm", children = children,
  })
end

local function room_header(room, state)
  local members = room.members or {}
  local vr = count(members, function(member) return member.presenter == "holodeck" end)
  local joined = ("joined · %d member%s"):format(#members, #members == 1 and "" or "s")
  if vr > 0 then
    joined = joined .. (" · %d in Holodeck"):format(vr)
  end
  -- A topic is extra copy above the member count. A room without one says the
  -- member count once, not twice.
  local summary = { ui.text(room.topic or joined, { style = "caption", ink = "muted" }) }
  if room.topic then
    summary[#summary + 1] = ui.text(joined, { style = "caption", ink = "muted" })
  end
  return ui.bar({
    ink = "surface",
    radius = "md",
    pad = "md",
    children = {
      ui.row({
        widths = { 0.58, 0.42 },
        gap = "md",
        children = {
          ui.column({
            gap = "xs",
            children = {
              ui.row({
                gap = "xs",
                children = {
                  ui.text("#", { style = "title", ink = "muted" }),
                  ui.text(room.name or room.id, { style = "title" }),
                  ui.text("voice", { style = "caption", ink = "success" }),
                },
              }),
              ui.column({ gap = "xs", children = summary }),
            },
          }),
          ui.row({
            widths = { 0.66, 0.34 },
            gap = "sm",
            children = {
              control("Open Holodeck", {
                action = "enter-holodeck",
                body = { space = room.space or room.id },
                quiet = true,
                small = true,
              }),
              control("Leave", {
                id = "leave-room",
                action = "leave-room",
                quiet = true,
                small = true,
              }),
            },
          }),
        },
      }),
      state and state.request == "holodeck" and control("Opening Holodeck…", {
        id = "request-holodeck",
        action = "request-holodeck",
        enabled = false,
        quiet = true,
        small = true,
      }) or ui.spacer({ size = "none" }),
      ui.fill({ h = 4, ink = "muted", opacity = 0.12 }),
      ui.spacer({ size = "xs" }),
      metric_row(room),
    },
  })
end

local function room_panel(snapshot, state)
  local room = snapshot.room
  return ui.column({
    h = column_height(),
    gap = "md",
    children = {
      room_header(room, state),
      screen_stage(room),
      conversation(room),
      ui.spacer({ grow = true }),
      composer(state),
    },
  })
end

local function rail(snapshot, state)
  local children = { call_panel(snapshot, state) }
  if snapshot.room then
    children[#children + 1] = ui.spacer({ size = "sm" })
    children[#children + 1] = ui.fill({ h = 4, ink = "muted", opacity = 0.12 })
    children[#children + 1] = ui.spacer({ size = "sm" })
    children[#children + 1] = member_rail(snapshot.room)
    children[#children + 1] = ui.spacer({ size = "sm" })
    if #(snapshot.room.buses or {}) > 0 then
      children[#children + 1] = ui.fill({ h = 4, ink = "muted", opacity = 0.12 })
      children[#children + 1] = ui.spacer({ size = "sm" })
      children[#children + 1] = audio_rail(snapshot.room)
    end
    children[#children + 1] = ui.spacer({ grow = true })
    children[#children + 1] = ui.bar({
      ink = "surface_sunken",
      radius = "sm",
      pad = "sm",
      children = {
        ui.text("SHARED SPACE", { style = "caption", ink = "muted" }),
        ui.text(snapshot.room.space or snapshot.room.id, { style = "body" }),
        ui.text("Canvas · Holodeck twin", { style = "caption", ink = "muted" }),
      },
    })
  end
  return side_panel(children)
end

local function lobby_panel(snapshot)
  local children = {
    ui.text("No room selected", { style = "title" }),
    ui.text("Pick a room to see who is here.", {
      style = "caption", ink = "muted",
    }),
  }
  if snapshot and snapshot.error then
    children[#children + 1] = ui.text(snapshot.error, {
      style = "caption", ink = "warning",
    })
  end
  return ui.card({ pad = "md", children = children })
end

local function header(snapshot)
  local workspace = snapshot.workspace or { name = "Rooms", presence = "offline" }
  local ink = workspace.presence == "connected" and "success" or "warning"
  local members = (snapshot.room and snapshot.room.members) or {}
  local current = (snapshot.room and (snapshot.room.name or snapshot.room.id))
    or (snapshot.self and snapshot.self.room) or "no room"
  local me = (snapshot.self and snapshot.self.name) or "You"
  return ui.bar({
    h = 68,
    pad = "md",
    ink = "surface",
    opacity = 0.55,
    border = true,
    children = {
      ui.row({
        widths = { 0.48, 0.52 },
        gap = "md",
        children = {
          ui.column({
            gap = "xs",
            children = {
              ui.text(workspace.name, { style = "title" }),
              ui.text(("%s · %d member%s"):format(
                current, #members, #members == 1 and "" or "s"), {
                style = "caption", ink = "muted",
              }),
            },
          }),
          ui.row({
            widths = { 0.3, 0.42, 0.28 },
            gap = "sm",
            children = {
              ui.bar({
                h = 28,
                ink = "surface_sunken",
                radius = "pill",
                pad = "xs",
                children = {
                  ui.row({
                    align = "center",
                    gap = "xs",
                    children = {
                      ui.icon("dot", { ink = ink, size = "small" }),
                      ui.text(workspace.presence, { style = "caption", ink = ink }),
                    },
                  }),
                },
              }),
              ui.bar({
                h = 28,
                ink = "surface_sunken",
                radius = "pill",
                pad = "xs",
                children = {
                  ui.row({
                    widths = { 0.72, 0.28 },
                    gap = "xs",
                    children = {
                      ui.text("⌕ Search", { style = "caption", ink = "muted" }),
                      ui.text("⌘K", { style = "caption", ink = "muted" }),
                    },
                  }),
                },
              }),
              ui.bar({
                h = 28,
                ink = "surface_sunken",
                radius = "pill",
                pad = "xs",
                children = {
                  ui.row({
                    widths = { 0.24, 0.76 },
                    gap = "xs",
                    children = {
                      avatar(me, 20, "primary"),
                      ui.text(me, { style = "mono", ink = "primary" }),
                    },
                  }),
                },
              }),
            },
          }),
        },
      }),
    },
  })
end

local function room_view(snapshot, state)
  local shown = snapshot
  if state.leave and snapshot.room then
    shown = {}
    for key, value in pairs(snapshot) do shown[key] = value end
    shown.room = nil
    if snapshot.self then
      shown.self = {}
      for key, value in pairs(snapshot.self) do shown.self[key] = value end
      shown.self.room = nil
    end
  end
  return ui.column({
    gap = "md",
    children = {
      header(shown),
      ui.row({
        widths = { COLUMNS.sidebar, COLUMNS.main, COLUMNS.rail },
        gap = "md",
        children = {
          sidebar(shown, state),
          shown.room and room_panel(shown, state) or lobby_panel(snapshot),
          rail(shown, state),
        },
      }),
    },
  })
end

local function focused_screen(snapshot, state)
  local id = (state and state.focus) or (snapshot.view and snapshot.view.focus)
  if not id then return nil end
  for _, screen in ipairs((snapshot.room and snapshot.room.screens) or {}) do
    if screen.id == id then return screen end
  end
  return nil
end

local function fullscreen_view(snapshot, screen)
  local height = math.max(400, math.floor((viewport_height() - 150) / 4) * 4)
  return ui.column({
    gap = "sm",
    children = {
      ui.bar({
        ink = "surface",
        border = true,
        pad = "md",
        children = {
          ui.row({
            widths = { 0.2, 0.65, 0.15 },
            gap = "md",
            children = {
              control("Back to room", { action = "exit-fullscreen" }),
              ui.column({
                gap = "xs",
                children = {
                  ui.text(screen.title, { style = "subheading" }),
                  ui.text("shared by " .. screen.owner .. " · " ..
                    (snapshot.room.name or snapshot.room.id), {
                      style = "caption", ink = "muted",
                    }),
                },
              }),
              -- A disabled marker, not a second way to leave. The host reads
              -- the scene to echo the focused screen through the next snapshot,
              -- so a live input update cannot silently close fullscreen.
              control("Focused", {
                id = "focus-current-" .. scene_id(screen.id),
                action = "focus-current",
                enabled = false,
                quiet = true,
                small = true,
              }),
            },
          }),
        },
      }),
      screen_preview(screen, "focus-screen", height, false),
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
        control("Open rooms", { action = "open-rooms" }),
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
      control("Open room", { action = "open-room", body = { id = room.id } }),
      first and control("Fullscreen", {
        id = "fullscreen-" .. scene_id(first.id),
        action = "fullscreen-screen", body = { id = first.id },
      }) or control("Share", { action = "share-surface" }),
    },
  })
  return ui.card({ pad = "sm", children = children })
end

function M.render(snapshot)
  snapshot = snapshot or fixture()
  local state = view_state(snapshot)
  local mode = snapshot.view and snapshot.view.mode or "room"
  local focused = focused_screen(snapshot, state)
  local tree
  if focused then
    tree = fullscreen_view(snapshot, focused)
  elseif mode == "widget" then
    tree = widget(snapshot)
  else
    tree = room_view(snapshot, state)
  end
  ui.render(tree, theme())
  return tree
end


M.fixture = fixture

return M
