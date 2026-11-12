//! The lights view without GPUI or HTTP: bridge state in, layout and hit
//! targets out.
//!
//! Keeping this pure means the responsive layout and the pointer math are
//! tested on the host. The GPUI Spore only paints these rectangles and turns
//! a hit into one Hue request.

use std::collections::BTreeMap;

use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Light {
    pub id: String,
    pub name: String,
    pub on: bool,
    pub brightness: Option<u8>,
    pub reachable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub on: bool,
    pub brightness: Option<u8>,
    pub light_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub group: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub bridge_name: String,
    pub lights: BTreeMap<String, Light>,
    pub groups: Vec<Group>,
    pub scenes: Vec<Scene>,
}

impl Snapshot {
    pub fn parse(value: &Value) -> Result<Self, String> {
        let bridge_name = value
            .get("config")
            .and_then(|config| config.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("Hue Bridge")
            .to_string();

        let mut lights = BTreeMap::new();
        if let Some(object) = value.get("lights").and_then(Value::as_object) {
            for (id, light) in object {
                let state = light.get("state").unwrap_or(&Value::Null);
                lights.insert(
                    id.clone(),
                    Light {
                        id: id.clone(),
                        name: text(light, "name", id),
                        on: state.get("on").and_then(Value::as_bool).unwrap_or(false),
                        brightness: state
                            .get("bri")
                            .and_then(Value::as_u64)
                            .map(|value| value.min(254) as u8),
                        reachable: state
                            .get("reachable")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                    },
                );
            }
        }

        let mut groups = Vec::new();
        if let Some(object) = value.get("groups").and_then(Value::as_object) {
            for (id, group) in object {
                let action = group.get("action").unwrap_or(&Value::Null);
                let light_ids = group
                    .get("lights")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                let name = if id == "0" {
                    "All lights".to_string()
                } else {
                    text(group, "name", id)
                };
                let kind = if id == "0" {
                    "All".to_string()
                } else {
                    text(group, "type", "Group")
                };
                groups.push(Group {
                    id: id.clone(),
                    name,
                    kind,
                    on: action.get("on").and_then(Value::as_bool).unwrap_or(false),
                    brightness: action
                        .get("bri")
                        .and_then(Value::as_u64)
                        .map(|value| value.min(254) as u8),
                    light_ids,
                });
            }
        }
        groups.sort_by_key(|group| numeric_key(&group.id));
        // `/groups/0` is addressable but absent from the collection on some
        // bridge firmware. Keep the row so the UI has one honest all-lights
        // switch; its state comes from the separate fetch when available.
        if !groups.iter().any(|group| group.id == "0") {
            groups.insert(
                0,
                Group {
                    id: "0".to_string(),
                    name: "All lights".to_string(),
                    kind: "All".to_string(),
                    on: lights.values().any(|light| light.on),
                    brightness: None,
                    light_ids: lights.keys().cloned().collect(),
                },
            );
        }

        let mut scenes = Vec::new();
        if let Some(object) = value.get("scenes").and_then(Value::as_object) {
            for (id, scene) in object {
                scenes.push(Scene {
                    id: id.clone(),
                    name: text(scene, "name", id),
                    group: scene
                        .get("group")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                });
            }
        }
        scenes.sort_by(|left, right| left.name.cmp(&right.name));

        Ok(Self {
            bridge_name,
            lights,
            groups,
            scenes,
        })
    }

    pub fn group(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|group| group.id == id)
    }

    pub fn lights_in(&self, group_id: &str) -> Vec<&Light> {
        let mut lights: Vec<&Light> = if group_id == "0" {
            self.lights.values().collect()
        } else {
            let Some(group) = self.group(group_id) else {
                return Vec::new();
            };
            group
                .light_ids
                .iter()
                .filter_map(|id| self.lights.get(id))
                .collect()
        };
        lights.sort_by_key(|light| numeric_key(&light.id));
        lights
    }

    pub fn scenes_for(&self, group_id: &str) -> Vec<&Scene> {
        self.scenes
            .iter()
            .filter(|scene| scene.group.as_deref() == Some(group_id))
            .collect()
    }

    /// `"0"` is the bridge's all-lights group. If the bridge omitted it, the
    /// first real room is the initial selection.
    pub fn initial_group(&self) -> String {
        if self.groups.iter().any(|group| group.id == "0") {
            "0".to_string()
        } else {
            self.groups
                .first()
                .map(|group| group.id.clone())
                .unwrap_or_else(|| "0".to_string())
        }
    }
}

fn text(value: &Value, key: &str, fallback: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn numeric_key(id: &str) -> (u64, String) {
    (id.parse().unwrap_or(u64::MAX), id.to_string())
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }

    pub fn inset(&self, amount: f32) -> Self {
        Self {
            x: self.x + amount,
            y: self.y + amount,
            w: (self.w - amount * 2.0).max(0.0),
            h: (self.h - amount * 2.0).max(0.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    SelectGroup(String),
    ToggleGroup(String),
    Scene { group: String, scene: String },
    ToggleLight(String),
    Brightness { light: String, value: u8 },
    Refresh,
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub rect: Rect,
    pub action: Action,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Wide,
    Compact,
    Watch,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub mode: Mode,
    pub rail: Rect,
    pub header: Rect,
    pub content: Rect,
    pub rooms: Vec<Hit>,
    pub scenes: Vec<Hit>,
    pub cards: Vec<Hit>,
    pub group_power: Option<Hit>,
    pub refresh: Hit,
    pub hits: Vec<Hit>,
}

impl Layout {
    pub fn hit(&self, x: f32, y: f32) -> Option<Action> {
        self.hits
            .iter()
            .rev()
            .find(|hit| hit.rect.contains(x, y))
            .map(|hit| hit.action.clone())
    }
}

const WIDE: f32 = 760.0;
const WATCH: f32 = 560.0;
const RAIL: f32 = 232.0;
const PAD: f32 = 22.0;
const ROOM_H: f32 = 44.0;
const CHIP_H: f32 = 32.0;
const CARD_H: f32 = 126.0;

pub fn layout(snapshot: &Snapshot, selected: &str, width: f32, height: f32) -> Layout {
    if width >= WIDE {
        wide(snapshot, selected, width, height)
    } else if height < WATCH {
        watch(snapshot, selected, width, height)
    } else {
        compact(snapshot, selected, width, height)
    }
}

fn wide(snapshot: &Snapshot, selected: &str, width: f32, height: f32) -> Layout {
    let rail = Rect::new(0.0, 0.0, RAIL, height);
    let header = Rect::new(RAIL, 0.0, width - RAIL, 96.0);
    let content = Rect::new(RAIL + PAD, 96.0, width - RAIL - PAD * 2.0, height - 96.0);
    let mut rooms = Vec::new();
    let mut y = 78.0;
    for group in &snapshot.groups {
        if y + ROOM_H > height - 48.0 {
            break;
        }
        rooms.push(Hit {
            rect: Rect::new(0.0, y, RAIL, ROOM_H),
            action: Action::SelectGroup(group.id.clone()),
        });
        y += ROOM_H;
    }
    let refresh = Hit {
        rect: Rect::new(PAD, height - 42.0, RAIL - PAD * 2.0, 30.0),
        action: Action::Refresh,
    };
    let group_power = snapshot.group(selected).map(|group| Hit {
        rect: Rect::new(width - PAD - 104.0, 28.0, 104.0, 34.0),
        action: Action::ToggleGroup(group.id.clone()),
    });
    let columns = if content.w >= 520.0 { 3 } else { 2 };
    let (scenes, cards, hits) = body(snapshot, selected, content, columns);
    let mut all = Vec::new();
    all.extend(cards.clone());
    if let Some(power) = &group_power {
        all.push(power.clone());
    }
    all.extend(scenes.clone());
    all.extend(rooms.clone());
    all.push(refresh.clone());
    all.extend(hits);
    Layout {
        mode: Mode::Wide,
        rail,
        header,
        content,
        rooms,
        scenes,
        cards,
        group_power,
        refresh,
        hits: all,
    }
}

fn compact(snapshot: &Snapshot, selected: &str, width: f32, height: f32) -> Layout {
    let rail = Rect::new(0.0, 0.0, width, 0.0);
    let header = Rect::new(0.0, 0.0, width, 64.0);
    let mut rooms = Vec::new();
    let (mut x, mut y) = (16.0, 68.0);
    for group in &snapshot.groups {
        let w = (group.name.chars().count() as f32 * 7.2 + 42.0).clamp(76.0, 148.0);
        if x + w > width - 16.0 {
            x = 16.0;
            y += 38.0;
        }
        if y + CHIP_H > 150.0 {
            break;
        }
        rooms.push(Hit {
            rect: Rect::new(x, y, w, CHIP_H),
            action: Action::SelectGroup(group.id.clone()),
        });
        x += w + 8.0;
    }
    let content_y = y + CHIP_H + 12.0;
    let content = Rect::new(16.0, content_y, width - 32.0, height - content_y);
    let group_power = snapshot.group(selected).map(|group| Hit {
        rect: Rect::new(width - 100.0, content_y + 4.0, 84.0, 30.0),
        action: Action::ToggleGroup(group.id.clone()),
    });
    let refresh = Hit {
        rect: Rect::new(width - 86.0, 18.0, 70.0, 28.0),
        action: Action::Refresh,
    };
    let (scenes, cards, hits) = body(snapshot, selected, content, 2);
    let mut all = Vec::new();
    all.extend(cards.clone());
    if let Some(power) = &group_power {
        all.push(power.clone());
    }
    all.extend(scenes.clone());
    all.extend(rooms.clone());
    all.push(refresh.clone());
    all.extend(hits);
    Layout {
        mode: Mode::Compact,
        rail,
        header,
        content,
        rooms,
        scenes,
        cards,
        group_power,
        refresh,
        hits: all,
    }
}

fn watch(snapshot: &Snapshot, selected: &str, width: f32, height: f32) -> Layout {
    let rail = Rect::new(0.0, 0.0, width, 0.0);
    let header = Rect::new(0.0, 0.0, width, 46.0);
    let mut rooms = Vec::new();
    let chip_h = 28.0;
    let (mut x, mut y) = (12.0, 50.0);
    for group in &snapshot.groups {
        let w = (group.name.chars().count() as f32 * 6.4 + 42.0).clamp(64.0, 128.0);
        if x + w > width - 12.0 {
            x = 12.0;
            y += chip_h + 2.0;
        }
        if y + chip_h > 176.0 {
            break;
        }
        rooms.push(Hit {
            rect: Rect::new(x, y, w, chip_h),
            action: Action::SelectGroup(group.id.clone()),
        });
        x += w + 6.0;
    }
    let content = Rect::new(
        12.0,
        y + chip_h + 8.0,
        width - 24.0,
        height - y - chip_h - 16.0,
    );
    let group_power = snapshot.group(selected).map(|group| Hit {
        rect: Rect::new(width - 76.0, content.y, 64.0, 28.0),
        action: Action::ToggleGroup(group.id.clone()),
    });
    let refresh = Hit {
        rect: Rect::new(width - 62.0, 10.0, 50.0, 24.0),
        action: Action::Refresh,
    };
    let mut cards = Vec::new();
    let mut hits = Vec::new();
    let lights = snapshot.lights_in(selected);
    let mut row_y = content.y + 34.0;
    for light in lights {
        if row_y + 30.0 > height - 30.0 {
            break;
        }
        let row = Rect::new(content.x, row_y, content.w, 30.0);
        cards.push(Hit {
            rect: row,
            action: Action::ToggleLight(light.id.clone()),
        });
        row_y += 31.0;
    }
    hits.extend(cards.clone());
    if let Some(power) = &group_power {
        hits.push(power.clone());
    }
    hits.extend(rooms.clone());
    hits.push(refresh.clone());
    Layout {
        mode: Mode::Watch,
        rail,
        header,
        content,
        rooms,
        scenes: Vec::new(),
        cards,
        group_power,
        refresh,
        hits,
    }
}

fn body(
    snapshot: &Snapshot,
    selected: &str,
    content: Rect,
    columns: usize,
) -> (Vec<Hit>, Vec<Hit>, Vec<Hit>) {
    let mut scenes = Vec::new();
    let scene_list = snapshot.scenes_for(selected);
    let (mut x, mut y) = (content.x, content.y + 48.0);
    for scene in scene_list {
        let w = (scene.name.chars().count() as f32 * 7.4 + 26.0).clamp(84.0, 158.0);
        if x + w > content.x + content.w {
            x = content.x;
            y += CHIP_H + 7.0;
        }
        if y + CHIP_H > content.y + content.h {
            break;
        }
        scenes.push(Hit {
            rect: Rect::new(x, y, w, CHIP_H),
            action: Action::Scene {
                group: selected.to_string(),
                scene: scene.id.clone(),
            },
        });
        x += w + 7.0;
    }
    let cards_y = if scenes.is_empty() {
        content.y + 48.0
    } else {
        y + CHIP_H + 16.0
    };
    let gap = 10.0;
    let card_w = (content.w - gap * (columns.saturating_sub(1) as f32)) / columns.max(1) as f32;
    let mut cards = Vec::new();
    let mut details = Vec::new();
    let lights = snapshot.lights_in(selected);
    for (index, light) in lights.into_iter().enumerate() {
        let column = index % columns.max(1);
        let row = index / columns.max(1);
        let rect = Rect::new(
            content.x + column as f32 * (card_w + gap),
            cards_y + row as f32 * (CARD_H + gap),
            card_w,
            CARD_H,
        );
        if rect.y + rect.h > content.y + content.h {
            break;
        }
        cards.push(Hit {
            rect,
            action: Action::ToggleLight(light.id.clone()),
        });
        let toggle = Rect::new(rect.x + rect.w - 72.0, rect.y + 12.0, 60.0, 26.0);
        details.push(Hit {
            rect: toggle,
            action: Action::ToggleLight(light.id.clone()),
        });
        let preset_w = (rect.w - 30.0 - 8.0) / 3.0;
        for (preset, value) in [64u8, 127, 254].into_iter().enumerate() {
            details.push(Hit {
                rect: Rect::new(
                    rect.x + 15.0 + preset as f32 * (preset_w + 4.0),
                    rect.y + rect.h - 38.0,
                    preset_w,
                    26.0,
                ),
                action: Action::Brightness {
                    light: light.id.clone(),
                    value,
                },
            });
        }
    }
    (scenes, cards, details)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bridge() -> Snapshot {
        Snapshot::parse(&serde_json::json!({
            "config": {"name": "Hue Bridge"},
            "lights": {
                "1": {"name": "Sofa", "state": {"on": true, "bri": 128, "reachable": true}},
                "2": {"name": "Lamp", "state": {"on": false, "bri": 254, "reachable": false}},
                "3": {"name": "Hall", "state": {"on": true, "bri": 64, "reachable": true}}
            },
            "groups": {
                "0": {"name": "All lights", "type": "LightGroup", "lights": ["1", "2", "3"], "action": {"on": true, "bri": 128}},
                "1": {"name": "Living room", "type": "Room", "lights": ["1", "2"], "action": {"on": true, "bri": 128}},
                "2": {"name": "Hall", "type": "Room", "lights": ["3"], "action": {"on": true, "bri": 64}}
            },
            "scenes": {
                "abc": {"name": "Relax", "group": "1"},
                "def": {"name": "Bright", "group": "1"},
                "ghi": {"name": "Night", "group": "2"}
            }
        }))
        .unwrap()
    }

    #[test]
    fn parses_groups_lights_and_group_scenes() {
        let snapshot = bridge();
        assert_eq!(snapshot.bridge_name, "Hue Bridge");
        assert_eq!(snapshot.groups.len(), 3);
        assert_eq!(snapshot.group("1").unwrap().name, "Living room");
        assert_eq!(snapshot.lights_in("0").len(), 3);
        assert_eq!(snapshot.lights_in("1").len(), 2);
        assert_eq!(snapshot.scenes_for("1").len(), 2);
        assert!(snapshot.scenes_for("0").is_empty());
        assert_eq!(snapshot.initial_group(), "0");
    }

    #[test]
    fn missing_all_lights_group_is_synthesized_from_the_lights_that_exist() {
        let snapshot = Snapshot::parse(&serde_json::json!({
            "lights": {"1": {"name": "Sofa", "state": {"on": true}}},
            "groups": {"1": {"name": "Room", "lights": ["1"], "action": {"on": true}}},
            "scenes": {}
        }))
        .unwrap();
        let all = snapshot.group("0").expect("all lights group");
        assert_eq!(all.name, "All lights");
        assert!(all.on);
        assert_eq!(all.light_ids, ["1"]);
        assert_eq!(snapshot.initial_group(), "0");
    }

    #[test]
    fn wide_layout_puts_rooms_cards_and_presets_in_reach() {
        let snapshot = bridge();
        let layout = layout(&snapshot, "1", 1280.0, 900.0);
        assert_eq!(layout.mode, Mode::Wide);
        assert_eq!(layout.rooms.len(), 3);
        assert_eq!(layout.cards.len(), 2);
        assert_eq!(layout.scenes.len(), 2);
        assert!(
            layout
                .hit(30.0, layout.rooms[1].rect.y + 10.0)
                .is_some_and(|action| action == Action::SelectGroup("1".into()))
        );
        let card = &layout.cards[0];
        assert!(layout.hit(card.rect.x + 20.0, card.rect.y + 20.0).is_some());
        assert!(matches!(
            layout.hit(card.rect.x + card.rect.w - 45.0, card.rect.y + 25.0),
            Some(Action::ToggleLight(_))
        ));
        assert!(matches!(
            layout.hit(card.rect.x + 20.0, card.rect.y + card.rect.h - 24.0),
            Some(Action::Brightness { value: 64, .. })
        ));
    }

    #[test]
    fn compact_layout_fits_a_phone() {
        let snapshot = bridge();
        let layout = layout(&snapshot, "1", 454.0, 800.0);
        assert_eq!(layout.mode, Mode::Compact);
        assert!(!layout.rooms.is_empty());
        assert!(layout.cards.iter().all(|card| card.rect.x >= 0.0
            && card.rect.x + card.rect.w <= 454.0
            && card.rect.y + card.rect.h <= 800.0));
    }

    #[test]
    fn watch_layout_keeps_a_toggle_row_per_visible_light() {
        let snapshot = bridge();
        let layout = layout(&snapshot, "1", 320.0, 480.0);
        assert_eq!(layout.mode, Mode::Watch);
        assert!(!layout.cards.is_empty());
        assert!(
            layout
                .hit(layout.cards[0].rect.x + 10.0, layout.cards[0].rect.y + 10.0)
                .is_some_and(|action| matches!(action, Action::ToggleLight(_)))
        );
    }

    #[test]
    fn group_power_is_its_own_hit() {
        let snapshot = bridge();
        let layout = layout(&snapshot, "1", 900.0, 700.0);
        let power = layout.group_power.as_ref().unwrap();
        assert_eq!(
            layout.hit(power.rect.x + 5.0, power.rect.y + 5.0),
            Some(Action::ToggleGroup("1".into()))
        );
    }
}
