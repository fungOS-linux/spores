//! Lights view: a GPUI Spore that reads Hue state through `soil:http` and
//! presents its own texture through `soil:present`.
//!
//! The bridge key never enters the guest. Requests name the `hue.local`
//! capability and the `geode://home/hue/bridge` handle; the host binds the
//! former to the bridge it was configured with and substitutes the latter into
//! the v1 URL path.

wit_bindgen::generate!({
    path: "wit",
    world: "runner",
    with: {
        "wasi:webgpu/webgpu@0.3.0-rc.2": wgpu_wasi_webgpu::wasi::webgpu::webgpu,
    },
    generate_all,
});

use std::borrow::Cow;

use anyhow::Result;
use cli_http::{Http, Method, Request, Response};
use gpui::{
    AnyElement, AnyWindowHandle, AssetSource, Context, FontWeight, HeadlessAppContext, IntoElement,
    ParentElement, PlatformTextSystem as _, Render, SharedString, Styled, Window, div, prelude::*,
    px, relative, rgb, size,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::switch::Switch;
use gpui_component::{Icon, IconName, Selectable as _, Sizable as _, Size, Theme, ThemeMode};
use gpui_wgpu::{CosmicTextSystem, WgpuContext, WgpuHeadlessRenderer};
use hue_client::{Bridge, brightness, group_power, light_power};
use lights_core::{Action, Group, Hit, Layout, Light, Rect, Snapshot, layout};
use soil::http::http as http_abi;
use soil::present::{cursor, input, surface};

const FONT: &[u8] = include_bytes!("../assets/IBMPlexSans-Regular.ttf");
const FONT_FAMILY: &str = "IBM Plex Sans";

/// The few Lucide marks the view asks for, embedded so a wasip2 Spore never
/// reaches for a browser CDN. `gpui-component` resolves `IconName` to these
/// paths through the app's `AssetSource`.
struct LightsAssets;

impl AssetSource for LightsAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: Option<&'static [u8]> = match path {
            "icons/sun.svg" => Some(include_bytes!("../assets/icons/sun.svg")),
            "icons/moon.svg" => Some(include_bytes!("../assets/icons/moon.svg")),
            "icons/refresh-cw.svg" => Some(include_bytes!("../assets/icons/refresh-cw.svg")),
            "icons/palette.svg" => Some(include_bytes!("../assets/icons/palette.svg")),
            "icons/chevron-right.svg" => Some(include_bytes!("../assets/icons/chevron-right.svg")),
            "icons/chevron-down.svg" => Some(include_bytes!("../assets/icons/chevron-down.svg")),
            "icons/circle-check.svg" => Some(include_bytes!("../assets/icons/circle-check.svg")),
            "icons/triangle-alert.svg" => {
                Some(include_bytes!("../assets/icons/triangle-alert.svg"))
            }
            _ => None,
        };
        Ok(bytes.map(Cow::Borrowed))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

/// The capability name the host binds to a bridge. This is deliberately not an
/// IP address: moving the bridge must not require rebuilding the Spore.
const HUE_ORIGIN: &str = "http://hue.local";
/// A non-secret Geode handle. The host leases the key for one request.
const HUE_CREDENTIAL: &str = "geode://home/hue/bridge";

const BG: u32 = 0x070b12;
const PANEL: u32 = 0x0d141e;
const PANEL_RAISED: u32 = 0x131e2b;
const TRACK: u32 = 0x1b2938;
const TEXT: u32 = 0xe7eef7;
const MUTED: u32 = 0x8b9bb0;
const DIM: u32 = 0x5d7088;
const AMBER: u32 = 0xf5b95b;
const ROSE: u32 = 0xff6b8a;
const GREEN: u32 = 0x32e88b;

struct Component;
export!(Component);

impl Guest for Component {
    async fn run() -> u32 {
        match run_lights() {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("lights-view: {error:#}");
                1
            }
        }
    }
}

struct HostHttp;

impl Http for HostHttp {
    fn fetch(&mut self, request: Request) -> std::result::Result<Response, String> {
        let method = match request.method {
            Method::Get => http_abi::Method::Get,
            Method::Put => http_abi::Method::Put,
            Method::Post => http_abi::Method::Post,
        };
        let response = http_abi::fetch(&http_abi::Request {
            url: request.url,
            method,
            headers: request
                .headers
                .into_iter()
                .map(|(name, value)| http_abi::Header { name, value })
                .collect(),
            body: request.body,
            credential: request.credential,
            max_bytes: hue_client::MAX_BODY,
            timeout_ms: hue_client::TIMEOUT_MS,
        })
        .map_err(|error| format!("http: {error:?}"))?;
        Ok(Response {
            status: response.status,
            body: response.body,
        })
    }
}

struct LightsView {
    snapshot: Snapshot,
    selected: String,
    active_scene: Option<String>,
    status: String,
    error: bool,
    width: f32,
    height: f32,
    hovered: Option<Action>,
}

impl LightsView {
    fn layout(&self) -> Layout {
        layout(&self.snapshot, &self.selected, self.width, self.height)
    }

    fn hit(&self, x: f32, y: f32) -> Option<Action> {
        self.layout().hit(x, y)
    }

    fn refresh(&mut self, http: &mut dyn Http) {
        let bridge = match Bridge::with_credential(HUE_ORIGIN, HUE_CREDENTIAL) {
            Ok(bridge) => bridge,
            Err(error) => {
                self.status = error;
                self.error = true;
                return;
            }
        };
        match bridge
            .snapshot(http)
            .and_then(|value| Snapshot::parse(&value))
        {
            Ok(snapshot) => {
                self.snapshot = snapshot;
                if self.snapshot.group(&self.selected).is_none() {
                    self.selected = self.snapshot.initial_group();
                    self.active_scene = None;
                }
                self.status = "Updated from the bridge".to_string();
                self.error = false;
            }
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
    }

    fn perform(&mut self, action: Action, http: &mut dyn Http) {
        let bridge = match Bridge::with_credential(HUE_ORIGIN, HUE_CREDENTIAL) {
            Ok(bridge) => bridge,
            Err(error) => {
                self.status = error;
                self.error = true;
                return;
            }
        };
        match action {
            Action::SelectGroup(id) => {
                self.selected = id;
                self.active_scene = None;
                self.status.clear();
                self.error = false;
            }
            Action::ToggleGroup(id) => {
                let on = self.snapshot.group(&id).is_some_and(|group| group.on);
                let result = bridge.put(http, &format!("/groups/{id}/action"), group_power(!on));
                self.active_scene = None;
                self.finish(result, http);
            }
            Action::Scene { group, scene } => {
                let result = bridge.put(
                    http,
                    &format!("/groups/{group}/action"),
                    serde_json::json!({ "scene": scene }),
                );
                if result.is_ok() {
                    self.active_scene = Some(scene);
                }
                self.finish(result, http);
            }
            Action::ToggleLight(id) => {
                let on = self.snapshot.lights.get(&id).is_some_and(|light| light.on);
                let result = bridge.put(http, &format!("/lights/{id}/state"), light_power(!on));
                self.active_scene = None;
                self.finish(result, http);
            }
            Action::Brightness { light, value } => {
                let result = bridge.put(http, &format!("/lights/{light}/state"), brightness(value));
                self.active_scene = None;
                self.finish(result, http);
            }
            Action::Refresh => self.refresh(http),
        }
    }

    fn finish(
        &mut self,
        result: std::result::Result<serde_json::Value, String>,
        http: &mut dyn Http,
    ) {
        match result {
            Ok(_) => self.refresh(http),
            Err(error) => {
                self.status = error;
                self.error = true;
            }
        }
    }

    fn step_group(&mut self, delta: i32) {
        if self.snapshot.groups.is_empty() {
            return;
        }
        let current = self
            .snapshot
            .groups
            .iter()
            .position(|group| group.id == self.selected)
            .unwrap_or(0) as i32;
        let next = (current + delta).rem_euclid(self.snapshot.groups.len() as i32) as usize;
        self.selected = self.snapshot.groups[next].id.clone();
        self.active_scene = None;
        self.status.clear();
        self.error = false;
    }

    fn selected_group(&self) -> Option<&Group> {
        self.snapshot.group(&self.selected)
    }

    fn summary(&self) -> String {
        let total = self.snapshot.lights.len();
        let on = self
            .snapshot
            .lights
            .values()
            .filter(|light| light.on)
            .count();
        format!("{on} of {total} on")
    }

    fn hovered(&self, action: &Action) -> bool {
        self.hovered.as_ref() == Some(action)
    }
}

fn run_lights() -> Result<()> {
    let (width, height) = surface::size();
    if width == 0 || height == 0 {
        anyhow::bail!("surface has zero extent");
    }

    let instance = wgpu_wasi_webgpu::instance();
    let adapter = wit_bindgen::block_on(instance.request_adapter(&Default::default()))?;
    let (device, queue) = wit_bindgen::block_on(adapter.request_device(&Default::default()))?;
    let (context, format) = WgpuContext::from_offscreen(instance, adapter, device, queue)?;

    let text = std::sync::Arc::new(CosmicTextSystem::new_without_system_fonts(FONT_FAMILY));
    text.add_fonts(vec![Cow::Borrowed(FONT)])?;
    let factory_context = context.clone();
    let mut cx =
        HeadlessAppContext::with_platform(text, std::sync::Arc::new(LightsAssets), move || {
            Ok(Some(Box::new(WgpuHeadlessRenderer::with_context(
                factory_context.clone(),
                format,
            )?)))
        });
    cx.update(|app| {
        gpui_component::init(app);
        Theme::change(ThemeMode::Dark, None, app);
        let theme = Theme::global_mut(app);
        theme.font_family = FONT_FAMILY.into();
        theme.mono_font_family = FONT_FAMILY.into();
        theme.warning = rgb(AMBER).into();
        theme.button_warning = rgb(AMBER).into();
        theme.button_warning_hover = rgb(0xffc978).into();
        theme.button_warning_active = rgb(0xd89a3f).into();
        theme.button_warning_foreground = rgb(0x1b1205).into();
        theme.ring = rgb(AMBER).into();
    });

    let mut http = HostHttp;
    let mut snapshot = Snapshot::default();
    let (status, error) = match Bridge::with_credential(HUE_ORIGIN, HUE_CREDENTIAL) {
        Ok(bridge) => match bridge
            .snapshot(&mut http)
            .and_then(|value| Snapshot::parse(&value))
        {
            Ok(value) => {
                snapshot = value;
                ("Updated from the bridge".to_string(), false)
            }
            Err(message) => (message, true),
        },
        Err(message) => (message, true),
    };
    let selected = snapshot.initial_group();
    let view = LightsView {
        snapshot,
        selected,
        active_scene: None,
        status,
        error,
        width: width as f32,
        height: height as f32,
        hovered: None,
    };

    let scale = surface::scale() as f32;
    let window = cx.open_window_scaled(
        size(px(width as f32), px(height as f32)),
        scale,
        move |_, cx| cx.new(|_| view),
    )?;
    let any: AnyWindowHandle = window.into();
    let mut window_size = (width, height);
    let mut dirty = true;

    loop {
        let size_now = surface::size();
        if size_now != window_size {
            window_size = size_now;
            window.update(&mut cx, |view, window, _| {
                view.width = size_now.0 as f32;
                view.height = size_now.1 as f32;
                window.resize(size(px(size_now.0 as f32), px(size_now.1 as f32)));
            })?;
            dirty = true;
        }

        if dirty {
            cx.update_window(any, |_, window, app| {
                let _ = window.draw(app);
            })?;
            let image = cx.capture_screenshot(any)?;
            present_frame(&context, image.width(), image.height(), image.as_raw())?;
            dirty = false;
        }

        if !input::interactive() {
            break;
        }

        let mut any_event = false;
        while let Some(event) = input::poll(0) {
            any_event = true;
            match event {
                input::Event::KeyInput(key) if key.pressed => {
                    if key.key == "escape" {
                        return Ok(());
                    }
                    window.update(&mut cx, |view, _, cx| {
                        match key.key.as_str() {
                            "r" => view.perform(Action::Refresh, &mut http),
                            "arrow-down" => view.step_group(1),
                            "arrow-up" => view.step_group(-1),
                            "space" => {
                                if let Some(group) = view.selected_group() {
                                    let action = Action::ToggleGroup(group.id.clone());
                                    view.perform(action, &mut http);
                                }
                            }
                            _ => {}
                        }
                        cx.notify();
                    })?;
                }
                input::Event::Moved(pointer) => {
                    window.update(&mut cx, |view, _, cx| {
                        let hit = view.hit(pointer.x, pointer.y);
                        if hit != view.hovered {
                            view.hovered = hit;
                            cx.notify();
                        }
                    })?;
                }
                input::Event::Button(button)
                    if button.pressed && button.button == input::PointerButton::Left =>
                {
                    window.update(&mut cx, |view, _, cx| {
                        if let Some(action) = view.hit(button.x, button.y) {
                            view.perform(action, &mut http);
                            cx.notify();
                        }
                    })?;
                }
                input::Event::Scrolled(scroll) => {
                    window.update(&mut cx, |view, _, cx| {
                        view.step_group(if scroll.lines < 0.0 { 1 } else { -1 });
                        cx.notify();
                    })?;
                }
                _ => {}
            }
        }
        if any_event {
            dirty = true;
        }
        let over_hit = window.update(&mut cx, |view, _, _| view.hovered.is_some())?;
        let _ = cursor::set(if over_hit {
            cursor::CursorStyle::Pointer
        } else {
            cursor::CursorStyle::Default
        });
        if !any_event {
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    }
    Ok(())
}

fn present_frame(
    context: &WgpuContext,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> anyhow::Result<()> {
    let texture = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lights frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    context.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let texture = wgpu_wasi_webgpu::host_texture(texture);
    surface::present(&texture).map_err(|error| anyhow::anyhow!("present: {error}"))?;
    Ok(())
}

impl Render for LightsView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let layout = self.layout();
        let mut stack: Vec<AnyElement> = Vec::new();
        stack.push(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .w(px(self.width))
                .h(px(self.height))
                .bg(rgb(BG))
                .into_any_element(),
        );

        if layout.mode == lights_core::Mode::Wide {
            stack.push(self.rail(&layout));
        } else {
            stack.push(self.top_header(&layout));
        }
        stack.push(self.content_header(&layout));
        stack.extend(self.room_hits(&layout));
        stack.extend(self.scene_hits(&layout));
        stack.extend(self.card_hits(&layout));
        if let Some(power) = &layout.group_power {
            stack.push(self.power_button(power));
        }
        stack.push(self.refresh_button(&layout.refresh));
        let success_fits = layout.mode != lights_core::Mode::Compact || layout.content.h >= 520.0;
        if !self.status.is_empty() && (self.error || success_fits) {
            stack.push(self.status_line(&layout));
        }

        div()
            .size_full()
            .font_family(FONT_FAMILY)
            .text_color(rgb(TEXT))
            .children(stack)
    }
}

impl LightsView {
    fn rail(&self, layout: &Layout) -> AnyElement {
        div()
            .absolute()
            .left(px(0.0))
            .top(px(0.0))
            .w(px(layout.rail.w))
            .h(px(layout.rail.h))
            .bg(rgb(PANEL))
            .child(
                div()
                    .absolute()
                    .left(px(22.0))
                    .top(px(20.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        Icon::new(IconName::Palette)
                            .with_size(Size::Small)
                            .text_color(rgb(AMBER)),
                    )
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Lights"),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(22.0))
                    .top(px(50.0))
                    .w(px(layout.rail.w - 44.0))
                    .text_size(px(11.0))
                    .text_color(rgb(MUTED))
                    .truncate()
                    .child(SharedString::from(self.snapshot.bridge_name.clone())),
            )
            .child(
                div()
                    .absolute()
                    .left(px(22.0))
                    .top(px(70.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().w(px(5.0)).h(px(5.0)).rounded_lg().bg(rgb(AMBER)))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(rgb(MUTED))
                            .child(SharedString::from(self.summary())),
                    ),
            )
            .into_any_element()
    }

    fn top_header(&self, layout: &Layout) -> AnyElement {
        div()
            .absolute()
            .left(px(16.0))
            .top(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(
                        Icon::new(IconName::Palette)
                            .with_size(Size::XSmall)
                            .text_color(rgb(AMBER)),
                    )
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Lights"),
                    ),
            )
            .child(
                div()
                    .mt(px(2.0))
                    .text_size(px(10.0))
                    .text_color(rgb(MUTED))
                    .truncate()
                    .child(SharedString::from(self.snapshot.bridge_name.clone())),
            )
            .when(layout.mode != lights_core::Mode::Watch, |this| {
                this.child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(9.0))
                        .text_color(rgb(DIM))
                        .child(SharedString::from(self.summary())),
                )
            })
            .into_any_element()
    }

    fn content_header(&self, layout: &Layout) -> AnyElement {
        let group = self.selected_group();
        let title = group.map_or("All lights", |group| group.name.as_str());
        let count = self.snapshot.lights_in(&self.selected).len();
        let subtitle = match group {
            Some(group) if group.kind != "Group" => format!(
                "{} lights · {} · {}",
                count,
                group.kind,
                if group.on { "on" } else { "off" }
            ),
            _ => format!("{count} lights · {}", self.snapshot.bridge_name),
        };
        let (x, y) = if layout.mode == lights_core::Mode::Wide {
            (layout.header.x + 22.0, 24.0)
        } else {
            (layout.content.x, layout.content.y)
        };
        let max_w = if layout.mode == lights_core::Mode::Wide {
            layout.header.w - 150.0
        } else {
            layout.content.w - 100.0
        };
        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(max_w.max(100.0)))
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .truncate()
                    .child(SharedString::from(title.to_string())),
            )
            .child(
                div()
                    .mt(px(3.0))
                    .text_size(px(10.0))
                    .text_color(rgb(MUTED))
                    .truncate()
                    .child(SharedString::from(subtitle)),
            )
            .into_any_element()
    }

    fn room_hits(&self, layout: &Layout) -> Vec<AnyElement> {
        let mut elements = Vec::new();
        for hit in &layout.rooms {
            let Action::SelectGroup(id) = &hit.action else {
                continue;
            };
            let Some(group) = self.snapshot.group(id) else {
                continue;
            };
            let selected = *id == self.selected;
            let count = self.snapshot.lights_in(id).len();
            let bg = if selected {
                PANEL_RAISED
            } else if self.hovered(&hit.action) {
                0x182534
            } else {
                PANEL
            };
            let rect = if layout.mode == lights_core::Mode::Wide {
                hit.rect.inset(6.0)
            } else {
                hit.rect
            };
            elements.push(
                div()
                    .absolute()
                    .left(px(rect.x))
                    .top(px(rect.y))
                    .w(px(rect.w))
                    .h(px(rect.h))
                    .rounded_lg()
                    .bg(rgb(bg))
                    .when(selected, |this| {
                        this.child(
                            div()
                                .absolute()
                                .left(px(0.0))
                                .top(px(0.0))
                                .h_full()
                                .w(px(3.0))
                                .rounded_lg()
                                .bg(rgb(AMBER)),
                        )
                    })
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .child(if layout.mode == lights_core::Mode::Wide {
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(if selected {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .truncate()
                                    .child(SharedString::from(group.name.clone())),
                            )
                            .child(div().text_size(px(9.0)).text_color(rgb(DIM)).child(
                                SharedString::from(format!(
                                    "{count} {}",
                                    if count == 1 { "light" } else { "lights" }
                                )),
                            ))
                            .into_any_element()
                    } else {
                        div()
                            .min_w(px(0.0))
                            .text_size(px(11.0))
                            .font_weight(if selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .truncate()
                            .child(SharedString::from(group.name.clone()))
                            .into_any_element()
                    })
                    .child(
                        div()
                            .w(px(7.0))
                            .h(px(7.0))
                            .rounded_lg()
                            .bg(rgb(if group.on { AMBER } else { DIM })),
                    )
                    .into_any_element(),
            );
        }
        elements
    }

    fn scene_hits(&self, layout: &Layout) -> Vec<AnyElement> {
        let mut elements = Vec::new();
        for hit in &layout.scenes {
            let Action::Scene { scene, .. } = &hit.action else {
                continue;
            };
            let Some(scene) = self.snapshot.scenes.iter().find(|item| item.id == *scene) else {
                continue;
            };
            let selected = self.active_scene.as_ref() == Some(&scene.id);
            let button = Button::new(format!("scene-{}", scene.id))
                .label(scene.name.clone())
                .xsmall()
                .rounded(px(16.0))
                .selected(selected)
                .tooltip(format!("Recall {}", scene.name))
                .absolute()
                .left(px(hit.rect.x))
                .top(px(hit.rect.y))
                .w(px(hit.rect.w))
                .h(px(hit.rect.h));
            let button = if selected {
                button.warning()
            } else {
                button.secondary()
            };
            elements.push(button.into_any_element());
        }
        elements
    }

    fn card_hits(&self, layout: &Layout) -> Vec<AnyElement> {
        let mut elements = Vec::new();
        if layout.mode == lights_core::Mode::Watch {
            for hit in &layout.cards {
                let Action::ToggleLight(id) = &hit.action else {
                    continue;
                };
                let Some(light) = self.snapshot.lights.get(id) else {
                    continue;
                };
                elements.push(self.watch_row(hit, light));
            }
            return elements;
        }
        for hit in &layout.cards {
            let Action::ToggleLight(id) = &hit.action else {
                continue;
            };
            let Some(light) = self.snapshot.lights.get(id) else {
                continue;
            };
            elements.push(self.light_card(hit, light));
            let rect = hit.rect;
            let preset_w = (rect.w - 30.0 - 8.0) / 3.0;
            for (index, (label, value)) in [("25", 64u8), ("50", 127), ("100", 254)]
                .into_iter()
                .enumerate()
            {
                let preset = Rect::new(
                    rect.x + 15.0 + index as f32 * (preset_w + 4.0),
                    rect.y + rect.h - 38.0,
                    preset_w,
                    26.0,
                );
                let active = light
                    .brightness
                    .is_some_and(|bri| (bri as i32 - value as i32).abs() < 10);
                let button = Button::new(format!("preset-{id}-{value}"))
                    .label(label)
                    .xsmall()
                    .rounded(px(6.0))
                    .absolute()
                    .left(px(preset.x))
                    .top(px(preset.y))
                    .w(px(preset.w))
                    .h(px(preset.h));
                let button = if active {
                    button.warning()
                } else {
                    button.ghost()
                };
                elements.push(button.into_any_element());
            }
        }
        elements
    }

    fn light_card(&self, hit: &Hit, light: &Light) -> AnyElement {
        let rect = hit.rect;
        let accent = if !light.reachable {
            ROSE
        } else if light.on {
            AMBER
        } else {
            DIM
        };
        let fraction = light
            .brightness
            .map(|bri| f32::from(bri) / 254.0)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let mut card = div()
            .absolute()
            .left(px(rect.x))
            .top(px(rect.y))
            .w(px(rect.w))
            .h(px(rect.h))
            .rounded_lg()
            .bg(rgb(if light.on { PANEL_RAISED } else { PANEL }))
            .border_1()
            .border_color(rgb(if light.on { 0x263647 } else { 0x18222e }))
            .px(px(15.0))
            .py(px(13.0))
            .child(
                div()
                    .w(px(rect.w - 76.0))
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .truncate()
                    .child(SharedString::from(light.name.clone())),
            )
            .child(
                div()
                    .mt(px(3.0))
                    .text_size(px(10.0))
                    .text_color(rgb(accent))
                    .child(SharedString::from(if !light.reachable {
                        "Unreachable".to_string()
                    } else if light.on {
                        format!("On · {}%", percent(light.brightness))
                    } else {
                        "Off".to_string()
                    })),
            )
            .child(
                div()
                    .mt(px(14.0))
                    .relative()
                    .h(px(6.0))
                    .w_full()
                    .rounded_lg()
                    .bg(rgb(TRACK))
                    .child(
                        div()
                            .absolute()
                            .left(px(0.0))
                            .top(px(0.0))
                            .h(px(6.0))
                            .w(relative(fraction))
                            .rounded_lg()
                            .bg(rgb(accent)),
                    ),
            );
        card = card.child(
            div()
                .absolute()
                .left(px(rect.w - 75.0))
                .top(px(12.0))
                .w(px(60.0))
                .h(px(26.0))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Switch::new(format!("light-switch-{}", light.id))
                        .checked(light.on)
                        .with_size(Size::Small)
                        .color(rgb(AMBER))
                        .accessibility_label(format!(
                            "Turn {} {}",
                            light.name,
                            if light.on { "off" } else { "on" }
                        )),
                ),
        );
        card.into_any_element()
    }

    fn watch_row(&self, hit: &Hit, light: &Light) -> AnyElement {
        let rect = hit.rect;
        div()
            .absolute()
            .left(px(rect.x))
            .top(px(rect.y))
            .w(px(rect.w))
            .h(px(rect.h))
            .rounded_lg()
            .bg(rgb(if light.on { PANEL_RAISED } else { PANEL }))
            .px(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .min_w(px(0.0))
                    .text_size(px(11.0))
                    .truncate()
                    .child(SharedString::from(light.name.clone())),
            )
            .child(if light.reachable {
                div()
                    .relative()
                    .w(px(28.0))
                    .h(px(16.0))
                    .rounded_lg()
                    .bg(rgb(if light.on { AMBER } else { TRACK }))
                    .child(
                        div()
                            .absolute()
                            .left(px(if light.on { 13.0 } else { 2.0 }))
                            .top(px(2.0))
                            .w(px(12.0))
                            .h(px(12.0))
                            .rounded_lg()
                            .bg(rgb(if light.on { 0x1b1205 } else { MUTED })),
                    )
                    .into_any_element()
            } else {
                Icon::new(IconName::TriangleAlert)
                    .with_size(Size::XSmall)
                    .text_color(rgb(ROSE))
                    .into_any_element()
            })
            .into_any_element()
    }

    fn power_button(&self, hit: &Hit) -> AnyElement {
        let Action::ToggleGroup(id) = &hit.action else {
            return div().into_any_element();
        };
        let on = self.snapshot.group(id).is_some_and(|group| group.on);
        div()
            .absolute()
            .left(px(hit.rect.x))
            .top(px(hit.rect.y))
            .w(px(hit.rect.w))
            .h(px(hit.rect.h))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(if on { AMBER } else { MUTED }))
                    .child("Power"),
            )
            .child(
                Switch::new(format!("group-switch-{id}"))
                    .checked(on)
                    .with_size(Size::Small)
                    .color(rgb(AMBER))
                    .accessibility_label(if on {
                        "Turn this room off"
                    } else {
                        "Turn this room on"
                    }),
            )
            .into_any_element()
    }

    fn refresh_button(&self, hit: &Hit) -> AnyElement {
        let button = Button::new("refresh-bridge")
            .icon(IconName::RefreshCw)
            .xsmall()
            .ghost()
            .tooltip("Read the bridge again")
            .w_full()
            .h_full();
        let button = if hit.rect.w >= 64.0 {
            button.label("Refresh")
        } else {
            button
        };
        div()
            .absolute()
            .left(px(hit.rect.x))
            .top(px(hit.rect.y))
            .w(px(hit.rect.w))
            .h(px(hit.rect.h))
            .child(button)
            .into_any_element()
    }

    fn status_line(&self, layout: &Layout) -> AnyElement {
        let (x, y, w) = if layout.mode == lights_core::Mode::Wide {
            (22.0, layout.rail.h - 70.0, layout.rail.w - 44.0)
        } else {
            (
                16.0,
                layout.content.y + layout.content.h - 20.0,
                layout.content.w,
            )
        };
        div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .px(px(2.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(9.0))
            .text_color(rgb(if self.error { ROSE } else { GREEN }))
            .child(
                Icon::new(if self.error {
                    IconName::TriangleAlert
                } else {
                    IconName::CircleCheck
                })
                .with_size(Size::XSmall),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .child(SharedString::from(self.status.clone())),
            )
            .into_any_element()
    }
}

fn percent(brightness: Option<u8>) -> u8 {
    brightness
        .map(|value| ((u16::from(value) * 100) / 254) as u8)
        .unwrap_or(0)
}
