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
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

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
use hue_client::{Bridge, brightness, color, color_temp, group_power, light_power};
use lights_core::{Action, ColorSwatch, Group, Hit, Layout, Light, Rect, Snapshot, layout};
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

/// How often a drag sends a value to the bridge. Hue's documentation asks for
/// at most ten commands a second per light, and the local snapshot already
/// moves at pointer speed, so the network only has to keep up with the eye.
const DRAG_INTERVAL: Duration = Duration::from_millis(90);

struct Drag {
    action: Action,
    rect: Rect,
    last_sent: Option<Instant>,
}

/// Whether the first snapshot has landed. The surface comes up before the
/// bridge answers, so the first frame is a skeleton rather than a blank card.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Loading,
    Ready,
}

struct LightsView {
    snapshot: Snapshot,
    selected: String,
    active_scene: Option<String>,
    /// The swatch the wearer last picked per light. The bridge reports xy, not
    /// which swatch it came from, so this is UI memory and never claims to be
    /// bridge state.
    active_color: BTreeMap<String, ColorSwatch>,
    status: String,
    error: bool,
    phase: Phase,
    updated_at: Option<Instant>,
    stale: bool,
    width: f32,
    height: f32,
    hovered: Option<Action>,
    dragging: Option<Drag>,
}

impl LightsView {
    fn layout(&self) -> Layout {
        layout(&self.snapshot, &self.selected, self.width, self.height)
    }

    fn hit(&self, x: f32, y: f32) -> Option<Action> {
        self.layout().hit(x, y)
    }

    fn refresh(&mut self, http: &mut dyn Http) {
        let started = Instant::now();
        self.phase = Phase::Ready;
        let bridge = match Bridge::with_credential(HUE_ORIGIN, HUE_CREDENTIAL) {
            Ok(bridge) => bridge,
            Err(error) => {
                self.fail(error);
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
                self.status.clear();
                self.error = false;
                self.stale = false;
                self.updated_at = Some(started);
            }
            Err(error) => self.fail(error),
        }
    }

    /// Keep whatever the last good snapshot said and mark it stale: an
    /// unreachable bridge is not evidence that every light turned off.
    fn fail(&mut self, error: String) {
        self.status = error;
        self.error = true;
        self.stale = !self.snapshot.lights.is_empty();
        self.phase = Phase::Ready;
    }

    fn perform(&mut self, action: Action, http: &mut dyn Http) {
        let bridge = match Bridge::with_credential(HUE_ORIGIN, HUE_CREDENTIAL) {
            Ok(bridge) => bridge,
            Err(error) => {
                self.fail(error);
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
            Action::Color { light, swatch } => {
                let (r, g, b) = swatch.rgb();
                let result = bridge.put(http, &format!("/lights/{light}/state"), color(r, g, b));
                if result.is_ok() {
                    self.active_color.insert(light, swatch);
                    self.active_scene = None;
                }
                self.finish(result, http);
            }
            // The track and the ramp are drag targets: pressing one starts a
            // drag, which sends its own values.
            Action::Brightness { .. } | Action::ColorTemp { .. } => {}
            Action::Refresh => self.refresh(http),
        }
    }

    fn put_state(&mut self, http: &mut dyn Http, light: &str, body: serde_json::Value) {
        let bridge = match Bridge::with_credential(HUE_ORIGIN, HUE_CREDENTIAL) {
            Ok(bridge) => bridge,
            Err(error) => {
                self.fail(error);
                return;
            }
        };
        if let Err(error) = bridge.put(http, &format!("/lights/{light}/state"), body) {
            self.fail(error);
        }
    }

    /// Move the drag's target to `x`, keeping the local snapshot in step so the
    /// control tracks the pointer even between bridge commands.
    fn drag_to(&mut self, x: f32, http: &mut dyn Http, force: bool) {
        let Some(drag) = &self.dragging else {
            return;
        };
        let fraction = ((x - drag.rect.x) / drag.rect.w.max(1.0)).clamp(0.0, 1.0);
        let action = drag.action.clone();
        let due = force
            || drag
                .last_sent
                .is_none_or(|sent| sent.elapsed() >= DRAG_INTERVAL);
        match action {
            Action::Brightness { light } => {
                let level = (fraction * 254.0).round().clamp(1.0, 254.0) as u8;
                if let Some(target) = self.snapshot.lights.get_mut(&light) {
                    target.on = true;
                    target.brightness = Some(level);
                }
                if due {
                    self.put_state(http, &light, brightness(level));
                }
            }
            Action::ColorTemp { light } => {
                // Left is warm (500 mired), right is daylight (153).
                let mired = (500.0 - fraction * (500.0 - 153.0)).round() as u16;
                if let Some(target) = self.snapshot.lights.get_mut(&light) {
                    target.on = true;
                    target.ct = Some(mired);
                    target.colormode = Some("ct".to_string());
                }
                if due {
                    self.put_state(http, &light, color_temp(mired));
                }
            }
            _ => return,
        }
        if due && let Some(drag) = self.dragging.as_mut() {
            drag.last_sent = Some(Instant::now());
        }
    }

    /// Land the value the pointer let go on, then read the bridge once so the
    /// screen settles on what the bulbs actually did.
    fn end_drag(&mut self, x: f32, http: &mut dyn Http) {
        if self.dragging.is_none() {
            return;
        }
        self.drag_to(x, http, true);
        self.dragging = None;
        self.refresh(http);
    }

    fn finish(
        &mut self,
        result: std::result::Result<serde_json::Value, String>,
        http: &mut dyn Http,
    ) {
        match result {
            Ok(_) => self.refresh(http),
            Err(error) => self.fail(error),
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
        if self.phase == Phase::Loading {
            return "Reading the bridge".to_string();
        }
        if self.error && self.snapshot.lights.is_empty() {
            return "No snapshot".to_string();
        }
        let total = self.snapshot.lights.len();
        let on = self
            .snapshot
            .lights
            .values()
            .filter(|light| light.on)
            .count();
        format!("{on} of {total} on")
    }

    /// The one line at the bottom of the surface: a short status, with the
    /// bridge's raw message kept for the notice where there is room for it.
    fn status_text(&self) -> String {
        if self.error {
            return "Bridge unreachable · click to retry".to_string();
        }
        match self.updated_at {
            Some(at) => format!("Updated {}", ago(at.elapsed())),
            None => "Reading the bridge".to_string(),
        }
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
    // Open the window before the first read. A bridge that is slow to answer
    // costs a skeleton frame, not a blank surface, and a bridge that never
    // answers still shows the error instead of hanging on a white rectangle.
    let view = LightsView {
        snapshot: Snapshot::default(),
        selected: "0".to_string(),
        active_scene: None,
        active_color: BTreeMap::new(),
        status: String::new(),
        error: false,
        phase: Phase::Loading,
        updated_at: None,
        stale: false,
        width: width as f32,
        height: height as f32,
        hovered: None,
        dragging: None,
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
    let mut first_read = true;
    let mut last_tick = Instant::now();

    loop {
        if first_read {
            // The skeleton goes out before the blocking read, so the wearer
            // sees the layout of the thing they are waiting for.
            first_read = false;
            draw_and_present(&mut cx, any, &context)?;
            window.update(&mut cx, |view, _, cx| {
                view.refresh(&mut http);
                view.selected = view.snapshot.initial_group();
                cx.notify();
            })?;
            dirty = true;
        }

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

        // The status line counts up, so the surface is one frame stale even
        // when nothing has been touched.
        if !dirty && last_tick.elapsed() >= Duration::from_secs(1) {
            last_tick = Instant::now();
            if window.update(&mut cx, |view, _, _| view.updated_at.is_some())? {
                dirty = true;
            }
        }

        if dirty {
            draw_and_present(&mut cx, any, &context)?;
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
                        if view.dragging.is_some() {
                            view.drag_to(pointer.x, &mut http, false);
                        } else {
                            let hit = view.hit(pointer.x, pointer.y);
                            if hit != view.hovered {
                                view.hovered = hit;
                            }
                        }
                        cx.notify();
                    })?;
                }
                input::Event::Button(button)
                    if button.pressed && button.button == input::PointerButton::Left =>
                {
                    window.update(&mut cx, |view, _, cx| {
                        let target = view
                            .layout()
                            .hits
                            .iter()
                            .rev()
                            .find(|hit| hit.rect.contains(button.x, button.y))
                            .cloned();
                        match target {
                            // A track is a drag target: the press already sets
                            // the value, so a tap and a drag behave the same.
                            Some(hit) if is_track(&hit.action) => {
                                view.dragging = Some(Drag {
                                    action: hit.action,
                                    rect: hit.rect,
                                    last_sent: None,
                                });
                                view.drag_to(button.x, &mut http, true);
                            }
                            Some(hit) => view.perform(hit.action, &mut http),
                            None => {}
                        }
                        cx.notify();
                    })?;
                }
                input::Event::Button(button)
                    if !button.pressed && button.button == input::PointerButton::Left =>
                {
                    window.update(&mut cx, |view, _, cx| {
                        view.end_drag(button.x, &mut http);
                        cx.notify();
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

fn draw_and_present(
    cx: &mut HeadlessAppContext,
    any: AnyWindowHandle,
    context: &WgpuContext,
) -> anyhow::Result<()> {
    cx.update_window(any, |_, window, app| {
        let _ = window.draw(app);
    })?;
    let image = cx.capture_screenshot(any)?;
    present_frame(context, image.width(), image.height(), image.as_raw())
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
        if self.phase == Phase::Loading {
            stack.extend(self.skeleton(&layout));
        } else {
            stack.extend(self.room_hits(&layout));
            stack.extend(self.scene_hits(&layout));
            stack.extend(self.card_hits(&layout));
            stack.extend(self.control_hits(&layout));
            if let Some(power) = &layout.group_power {
                stack.push(self.power_button(power));
            }
            if layout.cards.is_empty() {
                stack.push(self.empty_notice(&layout));
            }
        }
        stack.push(self.refresh_button(&layout.refresh));
        if self.phase == Phase::Ready {
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
        let subtitle = if self.phase == Phase::Loading {
            "Reading the bridge".to_string()
        } else if self.error && count == 0 {
            "No snapshot yet".to_string()
        } else if count == 0 {
            "No lights".to_string()
        } else {
            match group {
                Some(group) if group.kind != "Group" => format!(
                    "{} lights · {} · {}",
                    count,
                    group.kind,
                    if group.on { "on" } else { "off" }
                ),
                _ => format!("{count} lights · {}", self.snapshot.bridge_name),
            }
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
        }
        elements
    }

    /// What sits in the card area when there is nothing to draw: an unreachable
    /// bridge on the first read, or a room the bridge says has no lights.
    fn empty_notice(&self, layout: &Layout) -> AnyElement {
        let (icon, title, body) = if self.error {
            (
                IconName::TriangleAlert,
                "Can't reach the bridge",
                self.status.clone(),
            )
        } else {
            (
                IconName::Palette,
                "No lights here",
                "This room has nothing the bridge will list.".to_string(),
            )
        };
        div()
            .absolute()
            .left(px(layout.content.x))
            .top(px(layout.content.y + 56.0))
            .w(px(layout.content.w))
            .h(px(96.0))
            .rounded_lg()
            .border_1()
            .border_color(rgb(0x18222e))
            .bg(rgb(PANEL))
            .px(px(18.0))
            .py(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(13.0))
                    .when(self.error, |this| this.text_color(rgb(ROSE)))
                    .child(Icon::new(icon).with_size(Size::Small))
                    .child(SharedString::from(title)),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .text_size(px(10.0))
                    .text_color(rgb(MUTED))
                    .truncate()
                    .child(SharedString::from(body)),
            )
            .into_any_element()
    }

    /// Brightness tracks, color ramps, and swatches. They paint after the card
    /// so a busy light can still be dragged.
    fn control_hits(&self, layout: &Layout) -> Vec<AnyElement> {
        let mut elements = Vec::new();
        for hit in &layout.controls {
            match &hit.action {
                Action::Brightness { light } => {
                    let Some(light) = self.snapshot.lights.get(light) else {
                        continue;
                    };
                    elements.push(self.track(hit, light));
                }
                Action::ColorTemp { light } => {
                    let Some(light) = self.snapshot.lights.get(light) else {
                        continue;
                    };
                    elements.push(self.ramp(hit, light));
                }
                Action::Color { light, swatch } => {
                    let active = self.active_color.get(light) == Some(swatch);
                    elements.push(self.swatch(hit, *swatch, active));
                }
                _ => {}
            }
        }
        elements
    }

    /// The brightness bar. Its hit rect is taller than the bar so a fingertip
    /// can find it on a phone; the bar itself stays thin.
    fn track(&self, hit: &Hit, light: &Light) -> AnyElement {
        let rect = hit.rect;
        let fraction = light
            .brightness
            .map(|bri| f32::from(bri) / 254.0)
            .unwrap_or(0.0)
            .clamp(0.0, 1.0);
        let accent = if light.on { AMBER } else { DIM };
        let y = rect.y + rect.h / 2.0 - 3.0;
        div()
            .absolute()
            .left(px(rect.x))
            .top(px(rect.y))
            .w(px(rect.w))
            .h(px(rect.h))
            .child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(y - rect.y))
                    .w_full()
                    .h(px(6.0))
                    .rounded_full()
                    .bg(rgb(TRACK)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(y - rect.y))
                    .w(relative(fraction))
                    .h(px(6.0))
                    .rounded_full()
                    .bg(rgb(accent)),
            )
            .child(
                div()
                    .absolute()
                    .left(px((rect.w * fraction - 6.0).clamp(-6.0, rect.w - 6.0)))
                    .top(px(rect.h / 2.0 - 6.0))
                    .w(px(12.0))
                    .h(px(12.0))
                    .rounded_full()
                    .bg(rgb(if light.on { 0xfff0d2 } else { MUTED })),
            )
            .into_any_element()
    }

    /// The color-temperature ramp: warm on the left, daylight on the right,
    /// with a marker where the bulb currently sits.
    fn ramp(&self, hit: &Hit, light: &Light) -> AnyElement {
        let rect = hit.rect;
        let fraction = light
            .ct
            .map(|ct| 1.0 - (f32::from(ct) - 153.0) / (500.0 - 153.0))
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        let y = rect.y + rect.h / 2.0 - 4.0;
        let segments = 18;
        let mut bar = div()
            .absolute()
            .left(px(0.0))
            .top(px(y - rect.y))
            .w_full()
            .h(px(8.0))
            .rounded_full()
            .flex()
            .overflow_hidden();
        for index in 0..segments {
            let t = index as f32 / (segments - 1) as f32;
            let mired = (500.0 - t * (500.0 - 153.0)).round() as u16;
            let (r, g, b) = hue_client::mired_to_rgb(mired);
            bar = bar.child(
                div()
                    .flex_1()
                    .h_full()
                    .bg(rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))),
            );
        }
        div()
            .absolute()
            .left(px(rect.x))
            .top(px(rect.y))
            .w(px(rect.w))
            .h(px(rect.h))
            .when(!light.on, |this| this.opacity(0.45))
            .child(bar)
            .child(
                div()
                    .absolute()
                    .left(px((rect.w * fraction - 5.0).clamp(-5.0, rect.w - 5.0)))
                    .top(px(rect.h / 2.0 - 7.0))
                    .w(px(10.0))
                    .h(px(14.0))
                    .rounded_full()
                    .bg(rgb(TEXT))
                    .border_1()
                    .border_color(rgb(0x0b0f16)),
            )
            .into_any_element()
    }

    fn swatch(&self, hit: &Hit, swatch: ColorSwatch, active: bool) -> AnyElement {
        let (r, g, b) = swatch.rgb();
        let colour = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
        let dot = (hit.rect.w - 12.0).clamp(12.0, 20.0);
        div()
            .absolute()
            .left(px(hit.rect.x + (hit.rect.w - dot) / 2.0))
            .top(px(hit.rect.y + (hit.rect.h - dot) / 2.0))
            .w(px(dot))
            .h(px(dot))
            .rounded_full()
            .bg(rgb(colour))
            .border_2()
            .border_color(rgb(if active { TEXT } else { colour }))
            .when(!active, |this| this.opacity(0.85))
            .into_any_element()
    }

    /// Placeholder cards while the first read is in flight.
    fn skeleton(&self, layout: &Layout) -> Vec<AnyElement> {
        let mut elements = Vec::new();
        let gap = 10.0;
        let columns = layout.columns.max(1) as f32;
        let width = (layout.content.w - gap * (columns - 1.0)) / columns;
        let top = layout.content.y + 56.0;
        for row in 0..2 {
            for column in 0..layout.columns {
                let y = top + row as f32 * (layout.card_h + gap);
                if y + layout.card_h > layout.content.y + layout.content.h {
                    continue;
                }
                let x = layout.content.x + column as f32 * (width + gap);
                elements.push(
                    div()
                        .absolute()
                        .left(px(x))
                        .top(px(y))
                        .w(px(width))
                        .h(px(layout.card_h))
                        .rounded_lg()
                        .bg(rgb(PANEL))
                        .border_1()
                        .border_color(rgb(0x141d28))
                        .child(
                            div()
                                .absolute()
                                .left(px(15.0))
                                .top(px(14.0))
                                .w(px(width * 0.45))
                                .h(px(10.0))
                                .rounded_full()
                                .bg(rgb(0x18232f)),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(15.0))
                                .top(px(34.0))
                                .w(px(width * 0.28))
                                .h(px(7.0))
                                .rounded_full()
                                .bg(rgb(0x141d28)),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(15.0))
                                .top(px(66.0))
                                .w(px(width - 30.0))
                                .h(px(6.0))
                                .rounded_full()
                                .bg(rgb(0x18232f)),
                        )
                        .into_any_element(),
                );
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
        let state = if !light.reachable {
            "Unreachable".to_string()
        } else if light.on {
            format!("On · {}%", percent(light.brightness))
        } else {
            "Off".to_string()
        };
        let dot = if light.on {
            self.dot_colour(light)
        } else {
            None
        };
        let full = rect.h >= 160.0;
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
            .child(
                div()
                    .absolute()
                    .left(px(15.0))
                    .top(px(if full { 12.0 } else { 8.0 }))
                    .w(px((rect.w - if full { 96.0 } else { 30.0 }).max(20.0)))
                    .text_size(px(if full { 14.0 } else { 12.0 }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .truncate()
                    .child(SharedString::from(light.name.clone())),
            )
            .child(
                div()
                    .absolute()
                    .left(px(15.0))
                    .top(px(if full { 34.0 } else { 27.0 }))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .when_some(dot, |this, colour| {
                        this.child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(rgb(colour)))
                    })
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(accent))
                            .child(SharedString::from(state)),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .left(px(rect.w - 75.0))
                    .top(px(if full { 12.0 } else { 22.0 }))
                    .w(px(60.0))
                    .h(px(if full { 26.0 } else { 24.0 }))
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
        if self.stale {
            card = card.opacity(0.75);
        }
        card.into_any_element()
    }

    /// The dot next to a light's state: the swatch that was just picked, or the
    /// color the bridge reports through `xy`/`ct`.
    fn dot_colour(&self, light: &Light) -> Option<u32> {
        let pack = |(r, g, b): (u8, u8, u8)| u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
        if let Some(swatch) = self.active_color.get(&light.id) {
            return Some(pack(swatch.rgb()));
        }
        match light.colormode.as_deref() {
            Some("xy") => light.xy.map(|[x, y]| {
                pack(hue_client::xy_to_rgb(
                    f64::from(x) / 1000.0,
                    f64::from(y) / 1000.0,
                ))
            }),
            Some("ct") => light.ct.map(|ct| pack(hue_client::mired_to_rgb(ct))),
            _ => None,
        }
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
        let rect = layout.status;
        let (colour, icon) = if self.error {
            (ROSE, IconName::TriangleAlert)
        } else if self.stale {
            (AMBER, IconName::TriangleAlert)
        } else {
            (GREEN, IconName::CircleCheck)
        };
        let text = self.status_text();
        div()
            .absolute()
            .left(px(rect.x))
            .top(px(rect.y))
            .w(px(rect.w))
            .h(px(rect.h))
            .px(px(2.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .text_size(px(9.0))
            .text_color(rgb(colour))
            .child(Icon::new(icon).with_size(Size::XSmall))
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .child(SharedString::from(text)),
            )
            .into_any_element()
    }
}

fn is_track(action: &Action) -> bool {
    matches!(action, Action::Brightness { .. } | Action::ColorTemp { .. })
}

fn ago(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    match seconds {
        0 => "just now".to_string(),
        1..=59 => format!("{seconds}s ago"),
        60..=3599 => format!("{}m ago", seconds / 60),
        _ => format!("{}h ago", seconds / 3600),
    }
}

fn percent(brightness: Option<u8>) -> u8 {
    brightness
        .map(|value| ((u16::from(value) * 100) / 254) as u8)
        .unwrap_or(0)
}
