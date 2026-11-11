//! Rooms Spore.
//!
//! Rooms, membership and admission are records in the `rooms` namespace; this
//! module owns the rules and the snapshot Canvas renders. It has no network,
//! no clock and no media authority: `soil:records` is the whole storage story,
//! and a media token is a host capability, not something a Spore signs.
//!
//! Actions:
//!   help
//!   list
//!   view <room> <member>
//!   open <room> [name]
//!   join <room> <member> [name]
//!   leave <room> <member>
//!   say <room> <member> <text>
//!   gate <room> <open|closed>
//!   request <room> <session> [name]
//!   admit <room> <session>
//!   deny <room> <session>
//!   code <room>
//!   call.join <room> <member> <endpoint@address>
//!   call.mute <on|off>
//!   call.tick [frames]
//!   call.leave

wit_bindgen::generate!({ path: "wit", world: "rooms", generate_all });

use std::cell::RefCell;

use serde_json::{Value, json};
use soil::mic::capture as mic;
use soil::records::records;
use soil::voice::session as voice;
use wasi::audio::playback as audio;

const NAMESPACE: &str = "rooms";
const ROOM_SCHEMA: &str = "fungos.room/room@1";
const MEMBER_SCHEMA: &str = "fungos.room/member@1";
const MESSAGE_SCHEMA: &str = "fungos.room/message@1";
const ADMISSION_SCHEMA: &str = "fungos.room/admission@1";
const MAX_RECORDS: usize = 4_000;
const MAX_MESSAGES: usize = 200;
const MAX_TEXT: usize = 4_000;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Admission {
    Waiting,
    Allowed,
    Denied,
}

impl Admission {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Allowed => "allowed",
            Self::Denied => "denied",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "waiting" => Some(Self::Waiting),
            "allowed" => Some(Self::Allowed),
            "denied" => Some(Self::Denied),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct Room {
    id: String,
    name: String,
    gated: bool,
}

#[derive(Debug, Clone)]
struct Member {
    room: String,
    id: String,
    name: String,
    mic: String,
}

#[derive(Debug, Clone)]
struct Message {
    room: String,
    seq: u64,
    from: String,
    name: String,
    text: String,
}

#[derive(Debug, Clone)]
struct AdmissionRecord {
    room: String,
    session: String,
    name: String,
    status: Admission,
}

#[derive(Debug, Default, Clone)]
struct State {
    rooms: Vec<Room>,
    members: Vec<Member>,
    messages: Vec<Message>,
    guests: Vec<AdmissionRecord>,
}

impl State {
    fn room(&self, id: &str) -> Option<&Room> {
        self.rooms.iter().find(|room| room.id == id)
    }

    fn room_mut(&mut self, id: &str) -> Option<&mut Room> {
        self.rooms.iter_mut().find(|room| room.id == id)
    }

    fn members_of(&self, room: &str) -> Vec<&Member> {
        self.members
            .iter()
            .filter(|member| member.room == room)
            .collect()
    }

    fn messages_of(&self, room: &str) -> Vec<&Message> {
        let mut messages = self
            .messages
            .iter()
            .filter(|message| message.room == room)
            .collect::<Vec<_>>();
        messages.sort_by_key(|message| message.seq);
        messages
    }

    fn guest(&self, room: &str, session: &str) -> Option<&AdmissionRecord> {
        self.guests
            .iter()
            .find(|guest| guest.room == room && guest.session == session)
    }

    fn guest_mut(&mut self, room: &str, session: &str) -> Option<&mut AdmissionRecord> {
        self.guests
            .iter_mut()
            .find(|guest| guest.room == room && guest.session == session)
    }

    /// The link is the grant for an open room; a closed one needs an admission.
    fn may_join(&self, room: &str, session: &str) -> bool {
        match self.room(room) {
            Some(room) if room.gated => matches!(
                self.guest(room.id.as_str(), session)
                    .map(|guest| &guest.status),
                Some(Admission::Allowed)
            ),
            _ => true,
        }
    }

    fn open(&mut self, id: &str, name: Option<&str>) -> &Room {
        if self.room(id).is_none() {
            self.rooms.push(Room {
                id: id.to_owned(),
                name: name.map(str::to_owned).unwrap_or_else(|| room_name(id)),
                gated: false,
            });
        }
        self.room(id).expect("room was just created")
    }

    fn join(&mut self, room: &str, member: &str, name: Option<&str>) -> Result<(), String> {
        if !self.may_join(room, member) {
            return Err("this room is closed; the host has to admit you".into());
        }
        self.open(room, None);
        let name = name.map(str::to_owned).unwrap_or_else(|| member.to_owned());
        match self
            .members
            .iter_mut()
            .find(|entry| entry.room == room && entry.id == member)
        {
            Some(existing) => existing.name = name,
            None => self.members.push(Member {
                room: room.to_owned(),
                id: member.to_owned(),
                name,
                mic: "off".into(),
            }),
        }
        Ok(())
    }

    fn gate(&mut self, room: &str, gated: bool) -> Result<(), String> {
        let Some(entry) = self.room_mut(room) else {
            return Err(format!("no room {room}"));
        };
        entry.gated = gated;
        if !gated {
            // Opening the room empties the queue. Denied is kept so the record
            // still says what happened; an open room ignores it anyway.
            for guest in self
                .guests
                .iter_mut()
                .filter(|guest| guest.room == room && guest.status == Admission::Waiting)
            {
                guest.status = Admission::Denied;
            }
        }
        Ok(())
    }

    fn request(&mut self, room: &str, session: &str, name: Option<&str>) -> Admission {
        self.open(room, None);
        if !self.room(room).is_some_and(|entry| entry.gated) {
            return Admission::Allowed;
        }
        let name = name
            .map(str::to_owned)
            .unwrap_or_else(|| session.to_owned());
        match self.guest_mut(room, session) {
            Some(guest) => guest.status.clone(),
            None => {
                self.guests.push(AdmissionRecord {
                    room: room.to_owned(),
                    session: session.to_owned(),
                    name,
                    status: Admission::Waiting,
                });
                Admission::Waiting
            }
        }
    }

    fn decide(&mut self, room: &str, session: &str, decision: Admission) -> Result<(), String> {
        match self.guest_mut(room, session) {
            Some(guest) => {
                guest.status = decision;
                Ok(())
            }
            None => Err(format!("{session} has not asked to join {room}")),
        }
    }

    fn say(
        &mut self,
        room: &str,
        from: &str,
        name: Option<&str>,
        text: &str,
    ) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("a room message cannot be empty".into());
        }
        if text.len() > MAX_TEXT {
            return Err(format!("a room message is at most {MAX_TEXT} bytes"));
        }
        if self.room(room).is_none() {
            return Err(format!("no room {room}"));
        }
        let name = name
            .map(str::to_owned)
            .or_else(|| {
                self.members
                    .iter()
                    .find(|member| member.room == room && member.id == from)
                    .map(|member| member.name.clone())
            })
            .unwrap_or_else(|| from.to_owned());
        let seq = self
            .messages
            .iter()
            .filter(|message| message.room == room)
            .map(|message| message.seq)
            .max()
            .unwrap_or(0)
            + 1;
        self.messages.push(Message {
            room: room.to_owned(),
            seq,
            from: from.to_owned(),
            name,
            text: text.to_owned(),
        });
        // The conversation is a window, not a transcript service.
        let mut seqs = self
            .messages
            .iter()
            .filter(|message| message.room == room)
            .map(|message| message.seq)
            .collect::<Vec<_>>();
        seqs.sort_unstable();
        if seqs.len() > MAX_MESSAGES {
            let cutoff = seqs[seqs.len() - MAX_MESSAGES];
            self.messages
                .retain(|message| message.room != room || message.seq > cutoff);
        }
        Ok(())
    }

    fn snapshot(&self, room: Option<&str>, member: &str) -> Value {
        let rooms = self
            .rooms
            .iter()
            .map(|room| {
                json!({
                    "id": room.id,
                    "name": room.name,
                    "kind": "voice",
                    "members": self.members_of(&room.id).len(),
                    "unread": 0,
                    "vr": 0,
                })
            })
            .collect::<Vec<_>>();
        let room_value = room.and_then(|id| self.room(id)).map(|room| {
            let members = self
                .members_of(&room.id)
                .iter()
                .map(|member| {
                    json!({
                        "id": member.id,
                        "name": member.name,
                        "kind": "human",
                        "presence": "active",
                        "speaking": false,
                        "mic": member.mic,
                        "presenter": "canvas",
                        "audio": "speech",
                    })
                })
                .collect::<Vec<_>>();
            let messages = self
                .messages_of(&room.id)
                .iter()
                .map(|message| {
                    json!({
                        "seq": message.seq,
                        "from": message.from,
                        "name": message.name,
                        "text": message.text,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "id": room.id,
                "name": room.name,
                "space": room.id,
                "joined": true,
                "members": members,
                "messages": messages,
                "buses": [
                    {"class": "speech", "buffer_ms": 40, "lossless": false, "publishers": members.len()},
                    {"class": "program", "buffer_ms": 250, "lossless": true, "publishers": 0},
                ],
                "screens": [],
            })
        });
        let waiting = room
            .map(|id| {
                self.guests
                    .iter()
                    .filter(|guest| guest.room == id && guest.status == Admission::Waiting)
                    .map(|guest| json!({"session": guest.session, "name": guest.name}))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let gated = room
            .and_then(|id| self.room(id))
            .map(|room| room.gated)
            .unwrap_or(false);
        let self_name = self
            .members
            .iter()
            .find(|entry| entry.id == member && room == Some(entry.room.as_str()))
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| member.to_owned());
        json!({
            "revision": self.rooms.len() + self.members.len() + self.messages.len(),
            "live": true,
            "workspace": {"name": "FPL Rooms", "presence": "connected"},
            "rooms": rooms,
            "self": {"member": member, "name": self_name, "room": room},
            "room": room_value,
            "view": {"selected": room.map(|id| format!("room:{id}"))},
            "access": {
                "host": false,
                "room": {"gated": gated, "waiting": waiting.len(), "status": if gated { "closed" } else { "open" }},
                "waiting": waiting,
            },
        })
    }
}

/// A room code is the room name; the bar shows it as words.
fn room_name(code: &str) -> String {
    let name = code
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() {
        code.to_owned()
    } else {
        name
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn load() -> Result<State, String> {
    let mut state = State::default();
    let mut cursor = None;
    let mut seen = 0;
    loop {
        let page = records::scan(NAMESPACE, cursor.as_deref(), 500)
            .map_err(|error| format!("records {NAMESPACE}: {error:?}"))?;
        for entry in page.entries {
            if entry.deleted {
                continue;
            }
            seen += 1;
            let body = serde_json::from_slice::<Value>(&entry.body)
                .map_err(|error| format!("decode {}: {error}", entry.key.id))?;
            match entry.schema.as_str() {
                ROOM_SCHEMA => state.rooms.push(Room {
                    id: text(&body, "id"),
                    name: text(&body, "name"),
                    gated: body.get("gated").and_then(Value::as_bool).unwrap_or(false),
                }),
                MEMBER_SCHEMA => state.members.push(Member {
                    room: text(&body, "room"),
                    id: text(&body, "id"),
                    name: text(&body, "name"),
                    mic: text(&body, "mic"),
                }),
                MESSAGE_SCHEMA => state.messages.push(Message {
                    room: text(&body, "room"),
                    seq: body.get("seq").and_then(Value::as_u64).unwrap_or(0),
                    from: text(&body, "from"),
                    name: text(&body, "name"),
                    text: text(&body, "text"),
                }),
                ADMISSION_SCHEMA => state.guests.push(AdmissionRecord {
                    room: text(&body, "room"),
                    session: text(&body, "session"),
                    name: text(&body, "name"),
                    status: Admission::parse(&text(&body, "status")).unwrap_or(Admission::Waiting),
                }),
                _ => {}
            }
            if seen >= MAX_RECORDS {
                return Ok(state);
            }
        }
        match page.next {
            Some(next) if Some(next.as_str()) != cursor.as_deref() => cursor = Some(next),
            _ => return Ok(state),
        }
    }
}

fn put(id: &str, schema: &str, body: Value) -> Result<(), String> {
    let body = serde_json::to_vec(&body).map_err(|error| format!("encode {id}: {error}"))?;
    records::upsert(&records::Entry {
        key: records::Key {
            collection: NAMESPACE.to_owned(),
            id: id.to_owned(),
        },
        schema: schema.to_owned(),
        revision: 0,
        updated_at_ms: 0,
        origin: String::new(),
        deleted: false,
        body,
    })
    .map_err(|error| format!("write {id}: {error:?}"))?;
    Ok(())
}

fn remove(id: &str) -> Result<(), String> {
    records::delete(&records::Key {
        collection: NAMESPACE.to_owned(),
        id: id.to_owned(),
    })
    .map_err(|error| format!("delete {id}: {error:?}"))
}

fn save(state: &State) -> Result<(), String> {
    for room in &state.rooms {
        put(
            &format!("room:{}", room.id),
            ROOM_SCHEMA,
            json!({"id": room.id, "name": room.name, "space": room.id, "gated": room.gated}),
        )?;
    }
    for member in &state.members {
        put(
            &format!("member:{}:{}", member.room, member.id),
            MEMBER_SCHEMA,
            json!({"room": member.room, "id": member.id, "name": member.name, "mic": member.mic}),
        )?;
    }
    for message in &state.messages {
        put(
            &format!("message:{}:{}", message.room, message.seq),
            MESSAGE_SCHEMA,
            json!({
                "room": message.room,
                "seq": message.seq,
                "from": message.from,
                "name": message.name,
                "text": message.text,
            }),
        )?;
    }
    for guest in &state.guests {
        put(
            &format!("admission:{}:{}", guest.room, guest.session),
            ADMISSION_SCHEMA,
            json!({
                "room": guest.room,
                "session": guest.session,
                "name": guest.name,
                "status": guest.status.as_str(),
            }),
        )?;
    }
    Ok(())
}

/// An open call: three host capabilities, held together.
///
/// The Spore has no threads, so it never pumps on its own: the host calls
/// `call.tick` and one tick moves one frame each way. That keeps the Spore a
/// command surface and the audio clock in the host.
struct Call {
    room: String,
    member: String,
    mic: mic::Mic,
    audio: audio::Output,
    muted: bool,
}

thread_local! {
    static CALL: RefCell<Option<Call>> = const { RefCell::new(None) };
}

fn call_status() -> Value {
    CALL.with(|call| match call.borrow().as_ref() {
        None => json!({"state": "idle"}),
        Some(call) => {
            let status = voice::current();
            json!({
                "state": "connected",
                "room": call.room,
                "member": call.member,
                "muted": call.muted,
                "voice": {
                    "state": format!("{:?}", status.state).to_lowercase(),
                    "frames_sent": status.frames_sent,
                    "frames_received": status.frames_received,
                    "frames_lost": status.frames_lost,
                },
            })
        }
    })
}

fn call_join(args: &[String]) -> Result<String, String> {
    let room = args.get(1).ok_or("call.join needs a room")?.clone();
    let member = args.get(2).ok_or("call.join needs a member")?.clone();
    let peer = args.get(3).ok_or("call.join needs a peer, endpoint@address")?.clone();
    if CALL.with(|call| call.borrow().is_some()) {
        return Err("a call is already open".into());
    }
    let mic = mic::open("a room call").map_err(|error| format!("microphone: {error:?}"))?;
    // One voice session per Spore, so the host owns it rather than handing out
    // a handle: `voice::*` below speaks for this call.
    voice::open(&peer).map_err(|error| format!("voice: {error:?}"))?;
    // The room speaks mono at 48 kHz; the host converts to whatever the device
    // or the Isochrone sink wants.
    let output = audio::open(audio::Format {
        sample_rate: 48_000,
        channels: 1,
    })
    .map_err(|error| format!("audio: {error:?}"))?;
    CALL.with(|call| {
        *call.borrow_mut() = Some(Call {
            room,
            member,
            mic,
            audio: output,
            muted: false,
        })
    });
    Ok(call_status().to_string())
}

fn call_leave() -> Result<String, String> {
    let call = CALL.with(|call| call.borrow_mut().take());
    let Some(call) = call else {
        return Ok(json!({"state": "idle"}).to_string());
    };
    let _ = call.mic.close();
    voice::close();
    let _ = call.audio.close();
    Ok(json!({"state": "idle", "left": call.room}).to_string())
}

fn call_mute(args: &[String]) -> Result<String, String> {
    let muted = match args.get(1).map(String::as_str) {
        Some("on") => true,
        Some("off") => false,
        _ => return Err("call.mute needs on or off".into()),
    };
    CALL.with(|call| {
        let mut call = call.borrow_mut();
        let Some(call) = call.as_mut() else {
            return Err("no call is open".to_string());
        };
        call.muted = muted;
        Ok(call_status().to_string())
    })
}

/// One pump. Each tick waits on the edge it is waiting for — capture while the
/// microphone is open, a received frame while it is muted — so the loop is paced
/// by audio rather than by a timer the guest cannot see. `frames` is how many
/// such waits one call is allowed, not how many frames are forced through.
fn call_tick(args: &[String]) -> Result<String, String> {
    /// Long enough to cover one 20 ms frame, short enough that leaving a call
    /// is noticed immediately.
    const WAIT_MS: u32 = 20;
    let ticks: u32 = args
        .get(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    CALL.with(|call| {
        let mut call = call.borrow_mut();
        let Some(call) = call.as_mut() else {
            return Err("no call is open".to_string());
        };
        for _ in 0..ticks {
            if call.muted {
                // Nothing to send, but the far side still has to be heard, so
                // this is the edge that paces a muted call.
                if let Some(pcm) =
                    voice::receive(WAIT_MS).map_err(|error| format!("voice receive: {error:?}"))?
                {
                    call.audio
                        .write(&pcm)
                        .map_err(|error| format!("audio write: {error:?}"))?;
                }
                continue;
            }
            if let Some(pcm) = call
                .mic
                .read(960, WAIT_MS)
                .map_err(|error| format!("microphone: {error:?}"))?
            {
                voice::send(&pcm.samples).map_err(|error| format!("voice send: {error:?}"))?;
            }
            // Opportunistic: whatever arrived while the microphone was waiting.
            if let Some(pcm) =
                voice::receive(1).map_err(|error| format!("voice receive: {error:?}"))?
            {
                call.audio
                    .write(&pcm)
                    .map_err(|error| format!("audio write: {error:?}"))?;
            }
        }
        Ok(call_status().to_string())
    })
}

fn run_action(args: &[String]) -> Result<String, String> {
    let action = args.first().map(String::as_str).unwrap_or("help");
    let mut state = load()?;
    match action {
        "help" => Ok(json!({
            "actions": [
                "list", "view <room> <member>", "open <room> [name]",
                "join <room> <member> [name]", "leave <room> <member>",
                "say <room> <member> <text>", "gate <room> <open|closed>",
                "request <room> <session> [name]", "admit <room> <session>",
                "deny <room> <session>", "code <room>",
                "call.join <room> <member> <endpoint@address>",
                "call.mute <on|off>", "call.tick [frames]", "call.leave",
            ]
        })
        .to_string()),
        "list" => Ok(state.snapshot(None, "self").to_string()),
        "view" => {
            let room = args.get(1).ok_or("view needs a room")?;
            let member = args.get(2).map(String::as_str).unwrap_or("self");
            Ok(state.snapshot(Some(room), member).to_string())
        }
        "open" => {
            let room = args.get(1).ok_or("open needs a room")?;
            let name = args.get(2).map(String::as_str);
            state.open(room, name);
            save(&state)?;
            Ok(json!({"room": room, "code": room}).to_string())
        }
        "join" => {
            let room = args.get(1).ok_or("join needs a room")?;
            let member = args.get(2).ok_or("join needs a member")?;
            state.join(room, member, args.get(3).map(String::as_str))?;
            save(&state)?;
            Ok(state.snapshot(Some(room), member).to_string())
        }
        "leave" => {
            let room = args.get(1).ok_or("leave needs a room")?;
            let member = args.get(2).ok_or("leave needs a member")?;
            state
                .members
                .retain(|entry| !(entry.room == *room && entry.id == *member));
            remove(&format!("member:{room}:{member}"))?;
            Ok(json!({"left": room}).to_string())
        }
        "say" => {
            let room = args.get(1).ok_or("say needs a room")?;
            let member = args.get(2).ok_or("say needs a member")?;
            if args.len() < 4 {
                return Err("say needs text".into());
            }
            let message = args[3..].join(" ");
            state.say(room, member, None, &message)?;
            save(&state)?;
            Ok(state.snapshot(Some(room), member).to_string())
        }
        "call.join" => call_join(args),
        "call.leave" => call_leave(),
        "call.mute" => call_mute(args),
        "call.tick" => call_tick(args),
        "gate" => {
            let room = args.get(1).ok_or("gate needs a room")?;
            let closed = match args.get(2).map(String::as_str) {
                Some("closed") => true,
                Some("open") => false,
                _ => return Err("gate needs open or closed".into()),
            };
            state.gate(room, closed)?;
            save(&state)?;
            Ok(json!({"room": room, "gated": closed}).to_string())
        }
        "request" => {
            let room = args.get(1).ok_or("request needs a room")?;
            let session = args.get(2).ok_or("request needs a session")?;
            let status = state.request(room, session, args.get(3).map(String::as_str));
            save(&state)?;
            Ok(json!({"room": room, "status": status.as_str()}).to_string())
        }
        "admit" | "deny" => {
            let room = args.get(1).ok_or("admit needs a room")?;
            let session = args.get(2).ok_or("admit needs a session")?;
            let decision = if action == "admit" {
                Admission::Allowed
            } else {
                Admission::Denied
            };
            state.decide(room, session, decision.clone())?;
            save(&state)?;
            Ok(json!({"room": room, "session": session, "status": decision.as_str()}).to_string())
        }
        "code" => {
            let room = args.get(1).ok_or("code needs a room")?;
            Ok(json!({"room": room, "code": room, "name": room_name(room)}).to_string())
        }
        other => Err(format!("unknown action {other}")),
    }
}

struct Component;

impl Guest for Component {
    fn run(args: Vec<String>) -> Result<(), String> {
        // The Spore ABI is stdout plus an error; Canvas reads the snapshot from
        // the printed JSON.
        println!("{}", run_action(&args)?);
        Ok(())
    }
}

export!(Component);

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> State {
        let mut state = State::default();
        state.open("studio", None);
        state
    }

    #[test]
    fn an_open_room_lets_the_link_through() {
        let state = room();
        assert!(state.may_join("studio", "avery"));
        assert!(state.may_join("missing", "avery"));
    }

    #[test]
    fn a_closed_room_waits_for_the_host() {
        let mut state = room();
        state.gate("studio", true).unwrap();
        assert!(!state.may_join("studio", "avery"));
        assert_eq!(
            state.request("studio", "avery", Some("Avery")),
            Admission::Waiting
        );
        assert!(!state.may_join("studio", "avery"));
        state.decide("studio", "avery", Admission::Allowed).unwrap();
        assert!(state.may_join("studio", "avery"));
    }

    #[test]
    fn a_denied_guest_stays_out_and_reopening_clears_the_queue() {
        let mut state = room();
        state.gate("studio", true).unwrap();
        state.request("studio", "avery", None);
        state.decide("studio", "avery", Admission::Denied).unwrap();
        assert!(!state.may_join("studio", "avery"));
        state.gate("studio", false).unwrap();
        assert!(state.may_join("studio", "avery"));
        assert!(
            state
                .guests
                .iter()
                .all(|guest| guest.status != Admission::Waiting)
        );
    }

    #[test]
    fn joining_a_closed_room_is_refused_but_admission_lets_it_in() {
        let mut state = room();
        state.gate("studio", true).unwrap();
        assert!(state.join("studio", "avery", Some("Avery")).is_err());
        state.request("studio", "avery", Some("Avery"));
        state.decide("studio", "avery", Admission::Allowed).unwrap();
        state.join("studio", "avery", Some("Avery")).unwrap();
        assert_eq!(state.members_of("studio").len(), 1);
    }

    #[test]
    fn messages_are_room_scoped_and_ordered() {
        let mut state = room();
        state.open("other", None);
        state
            .say("studio", "avery", Some("Avery"), "hello")
            .unwrap();
        state.say("other", "sam", Some("Sam"), "elsewhere").unwrap();
        state.say("studio", "sam", Some("Sam"), "hi").unwrap();
        let messages = state.messages_of("studio");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].text, "hello");
        assert_eq!(messages[1].seq, 2);
        assert!(state.say("studio", "avery", None, "   ").is_err());
    }

    #[test]
    fn the_snapshot_carries_the_room_the_members_and_the_queue() {
        let mut state = room();
        state.join("studio", "avery", Some("Avery")).unwrap();
        state.gate("studio", true).unwrap();
        state.request("studio", "session-1", Some("Guest"));
        let snapshot = state.snapshot(Some("studio"), "avery");
        assert_eq!(snapshot["room"]["id"], "studio");
        assert_eq!(snapshot["room"]["members"][0]["name"], "Avery");
        assert_eq!(snapshot["access"]["waiting"][0]["name"], "Guest");
        assert_eq!(snapshot["access"]["room"]["status"], "closed");
        assert_eq!(snapshot["rooms"][0]["members"], 1);
    }

    #[test]
    fn room_codes_read_as_names() {
        assert_eq!(room_name("fleet-pebble-89059"), "Fleet Pebble 89059");
        assert_eq!(room_name("studio"), "Studio");
    }
}
