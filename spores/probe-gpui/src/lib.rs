//! Probe: a Spore that shows what the host is telling it.
//!
//! Surface size and scale, pointer position and buttons, the last key with its
//! modifiers, and the mouse shape the Spore asks for. It is a harness for the
//! presenter — every number here comes from `soil:present`, so a wrong one is
//! visible instead of buried in a terminal's behaviour.

wit_bindgen::generate!({
    path: "wit",
    world: "runner",
    with: {
        "wasi:webgpu/webgpu@0.3.0-rc.2": wgpu_wasi_webgpu::wasi::webgpu::webgpu,
    },
    generate_all,
});

use gpui::{
    AnyElement, AnyWindowHandle, AppContext as _, Context, FontWeight, HeadlessAppContext,
    IntoElement,
    PlatformTextSystem as _, Render, SharedString, Window, div, prelude::*, px, rgb, size,
};
use gpui_wgpu::{CosmicTextSystem, WgpuContext, WgpuHeadlessRenderer};
use soil::present::{cursor, input, surface};
use std::{borrow::Cow, sync::Arc};

const LILEX: &[u8] = include_bytes!("../assets/Lilex-Regular.ttf");
const LILEX_BOLD: &[u8] = include_bytes!("../assets/Lilex-Bold.ttf");

const HEADER_H: f32 = 56.0;
const ROW_H: f32 = 30.0;

struct Probe {
    width: u32,
    height: u32,
    scale: f64,
    pointer: (f32, f32),
    presses: u32,
    last_key: SharedString,
    last_event: SharedString,
    interactive: bool,
}

impl Probe {
    fn new() -> Self {
        let (width, height) = surface::size();
        Self {
            width,
            height,
            scale: surface::scale(),
            pointer: (0.0, 0.0),
            presses: 0,
            last_key: SharedString::from("—"),
            last_event: SharedString::from("nothing yet"),
            interactive: input::interactive(),
        }
    }

    fn row(&self, label: &str, value: impl Into<SharedString>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .h(px(ROW_H))
            .gap(px(16.0))
            .child(
                div()
                    .w(px(190.0))
                    .text_size(px(12.0))
                    .text_color(rgb(0x76828f))
                    .child(SharedString::from(label.to_owned())),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(rgb(0xe6edf5))
                    .child(value.into()),
            )
            .into_any_element()
    }
}

impl Render for Probe {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let pointer = format!("{:.0}, {:.0}", self.pointer.0, self.pointer.1);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x0b0f14))
            .font_family("Lilex")
            .child(
                div()
                    .h(px(HEADER_H))
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(22.0))
                    .bg(rgb(0x0f141b))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xf2f6fb))
                            .child("Probe"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(0x76828f))
                            .child(SharedString::from(format!(
                                "{}×{} at {}x",
                                self.width, self.height, self.scale
                            ))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .p(px(22.0))
                    .gap(px(2.0))
                    // Everything here is what the host said, not what we hope.
                    .child(self.row("surface size", format!("{} × {}", self.width, self.height)))
                    .child(self.row("device scale", format!("{} px per point", self.scale)))
                    .child(self.row("interactive", self.interactive.to_string()))
                    .child(self.row("pointer", pointer))
                    .child(self.row("presses", self.presses.to_string()))
                    .child(self.row("last key", self.last_key.clone()))
                    .child(self.row("last event", self.last_event.clone())),
            )
    }
}

fn present_frame(
    context: &WgpuContext,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> anyhow::Result<()> {
    let texture = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("probe frame"),
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

fn key_label(key: &input::Key) -> String {
    let mut modifiers = String::new();
    if key.control {
        modifiers.push('⌃');
    }
    if key.alt {
        modifiers.push('⌥');
    }
    if key.shift {
        modifiers.push('⇧');
    }
    if key.meta {
        modifiers.push('⌘');
    }
    format!("{modifiers}{}", key.text.clone().unwrap_or_else(|| key.key.clone()))
}

fn run_probe() -> anyhow::Result<()> {
    let (width, height) = surface::size();
    let instance = wgpu_wasi_webgpu::instance();
    let adapter = wit_bindgen::block_on(instance.request_adapter(&Default::default()))?;
    let (device, queue) = wit_bindgen::block_on(adapter.request_device(&Default::default()))?;
    let (context, format) = WgpuContext::from_offscreen(instance, adapter, device, queue)?;

    // A Spore carries its own fonts: the guest has no system to ask, and
    // shaping with no face traps in the text system.
    let text = Arc::new(CosmicTextSystem::new_without_system_fonts("Lilex"));
    text.add_fonts(vec![Cow::Borrowed(LILEX), Cow::Borrowed(LILEX_BOLD)])?;
    let factory_context = context.clone();
    let mut cx = HeadlessAppContext::with_platform(text, Arc::new(()), move || {
        Ok(Some(Box::new(WgpuHeadlessRenderer::with_context(
            factory_context.clone(),
            format,
        )?)))
    });

    let scale = surface::scale() as f32;
    let window = cx.open_window_scaled(
        size(px(width as f32), px(height as f32)),
        scale,
        |_, cx| cx.new(|_| Probe::new()),
    )?;
    let any: AnyWindowHandle = window.into();

    let mut window_size = (width, height);
    let mut dirty = true;
    loop {
        let size_now = surface::size();
        if size_now != window_size {
            window_size = size_now;
            window.update(&mut cx, |view, window, _| {
                view.width = size_now.0;
                view.height = size_now.1;
                window.resize(gpui::size(px(size_now.0 as f32), px(size_now.1 as f32)));
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
        // Drain what is queued, then wait for the next event.
        let mut any_event = false;
        while let Some(event) = input::poll(0) {
            any_event = true;
            match event {
                input::Event::KeyInput(key) => {
                    if key.key == "escape" {
                        return Ok(());
                    }
                    window.update(&mut cx, |view, _, cx| {
                        view.last_key = SharedString::from(key_label(&key));
                        view.last_event = SharedString::from(format!("key {}", key.key));
                        cx.notify();
                    })?;
                }
                input::Event::Moved(pointer) => {
                    window.update(&mut cx, |view, _, cx| {
                        view.pointer = (pointer.x, pointer.y);
                        view.last_event = SharedString::from("pointer moved");
                        cx.notify();
                    })?;
                }
                input::Event::Button(button) => {
                    if button.pressed {
                        window.update(&mut cx, |view, _, cx| {
                            view.presses = view.presses.saturating_add(1);
                            view.last_event =
                                SharedString::from(format!("button {} pressed", match button.button {
                                    input::PointerButton::Left => "left",
                                    input::PointerButton::Right => "right",
                                    input::PointerButton::Middle => "middle",
                                }));
                            cx.notify();
                        })?;
                    }
                }
                input::Event::Scrolled(scroll) => {
                    window.update(&mut cx, |view, _, cx| {
                        view.last_event =
                            SharedString::from(format!("scrolled {:.1}", scroll.lines));
                        cx.notify();
                    })?;
                }
            }
        }
        if any_event {
            dirty = true;
        }
        // The cursor is the Spore's to ask for: crosshair anywhere in the body.
        let over_body = window.update(&mut cx, |view, _, _| {
            view.pointer.1 > HEADER_H && view.pointer.0 < view.width as f32
        })?;
        let _ = cursor::set(if over_body {
            cursor::CursorStyle::Crosshair
        } else {
            cursor::CursorStyle::Default
        });
        if !any_event {
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    }
    Ok(())
}

struct Component;

impl Guest for Component {
    async fn run() -> u32 {
        match run_probe() {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("probe: {error:#}");
                1
            }
        }
    }
}

export!(Component);
