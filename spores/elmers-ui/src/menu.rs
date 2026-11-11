//! A right-click menu any view can hang off the pointer.
//!
//! The menu is deliberately dumb: it knows how to draw a titled list of rows at
//! a window position and to report which row was picked. What a row *means* is
//! the calling view's business, so the runs list, the graph and anything added
//! later share one menu that looks and behaves the same.

use std::rc::Rc;

use gpui::{
    App, Deferred, Pixels, Point, Position, SharedString, Window, deferred, div, prelude::*, px,
    rgb,
};

const PANEL: u32 = 0x141a22;
const HAIRLINE: u32 = 0x2a3542;
const TEXT: u32 = 0xdbe4ee;
const MUTED: u32 = 0x6d7b8c;

/// How wide a menu is. One width for every menu in the app: a menu that resizes
/// with its longest label makes the same list look like different lists.
pub const WIDTH: f32 = 232.0;
const ROW_H: f32 = 22.0;
const TITLE_H: f32 = 18.0;

#[derive(Clone, Debug)]
pub struct Item {
    /// What the view gets back when this row is picked.
    pub id: &'static str,
    pub label: SharedString,
    /// Greyed text on the right: a key, a count, a reason.
    pub hint: SharedString,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct Menu {
    /// Window position of the menu's top-left corner.
    pub at: Point<Pixels>,
    pub title: SharedString,
    pub items: Vec<Item>,
}

impl Menu {
    pub fn new(at: Point<Pixels>, title: impl Into<SharedString>) -> Self {
        Self {
            at,
            title: title.into(),
            items: Vec::new(),
        }
    }

    pub fn item(
        mut self,
        id: &'static str,
        label: impl Into<SharedString>,
        hint: impl Into<SharedString>,
    ) -> Self {
        self.items.push(Item {
            id,
            label: label.into(),
            hint: hint.into(),
            enabled: true,
        });
        self
    }

    /// How tall this menu will be, so a caller can keep it on screen.
    pub fn height(&self) -> f32 {
        8.0 + TITLE_H + self.items.len() as f32 * ROW_H
    }

    /// The menu, drawn above everything else. `on_pick` gets the id of the row
    /// that was clicked; closing is the view's job, because only it knows what
    /// else a click should do.
    pub fn element<F>(&self, on_pick: F) -> Deferred
    where
        F: Fn(&str, &mut Window, &mut App) + 'static,
    {
        let mut panel = div()
            .w(px(WIDTH))
            .flex()
            .flex_col()
            .py(px(4.0))
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(HAIRLINE))
            .rounded(px(6.0))
            .shadow_lg()
            .child(
                div()
                    .px(px(10.0))
                    .h(px(TITLE_H))
                    .flex()
                    .items_center()
                    .text_size(px(10.0))
                    .text_color(rgb(MUTED))
                    .child(self.title.clone()),
            );
        let on_pick = Rc::new(on_pick);
        for (index, item) in self.items.iter().enumerate() {
            let id = item.id;
            let pick = on_pick.clone();
            let enabled = item.enabled;
            let mut row = div()
                .id(("menu", index))
                .h(px(ROW_H))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(11.0))
                .text_color(rgb(if enabled { TEXT } else { MUTED }));
            if enabled {
                row = row.on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                    pick(id, window, cx)
                });
            }
            panel = panel.child(
                row.child(div().flex_1().child(item.label.clone())).child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(MUTED))
                        .child(item.hint.clone()),
                ),
            );
        }
        deferred(place(panel, self.at))
    }
}

/// Put an element at a window position.
///
/// `div` has no `absolute()` builder in this fork, so the style is set directly;
/// `deferred` keeps the menu out of its parent's layout and paints it over the
/// top — the same two pieces Zed's own menus are made of.
fn place(mut element: gpui::Div, at: Point<Pixels>) -> gpui::Div {
    let style = element.style();
    style.position = Some(Position::Absolute);
    style.inset.left = Some(at.x.into());
    style.inset.top = Some(at.y.into());
    element
}
