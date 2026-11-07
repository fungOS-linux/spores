//! The DFM model view as a Spore guest.
//!
//! Same idea as `soil-dfm-viz`, but on the Spore side of the boundary: standard
//! `wasi:webgpu`, no window and no swapchain. The guest renders into a texture
//! and hands it to Canvas, which owns the window, the z-order and the input.
//!
//! The scene is baked in at build time. A Spore guest has no filesystem unless
//! it imports `soil:vfs`, and the canvas runner used to verify this links only
//! `wasi:webgpu` and `soil:canvas-gpu`; reading the scene at runtime is a
//! separate grant and a separate change.

wit_bindgen::generate!({
    path: "wit",
    world: "runner",
    with: {
        "wasi:webgpu/webgpu@0.3.0-rc.2": wgpu_wasi_webgpu::wasi::webgpu::webgpu,
    },
    generate_all,
});

use bytemuck::{Pod, Zeroable};
use soil::canvas_gpu::surface;

const SCENE: &[u8] = include_bytes!("../../model/data.json");

const SHADER: &str = r#"
struct Globals {
  view_proj: mat4x4<f32>,
  cam_right: vec4<f32>,
  cam_up: vec4<f32>,
  eye: vec4<f32>,
  light: vec4<f32>,
  base: vec4<f32>,
  highlight: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;

struct VIn {
  @location(0) pos: vec3<f32>,
  @location(1) nrm: vec3<f32>,
  @location(2) tint: f32,
};
struct VOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) nrm: vec3<f32>,
  @location(1) world: vec3<f32>,
  @location(2) tint: f32,
};

@vertex
fn vs_mesh(v: VIn) -> VOut {
  var o: VOut;
  o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
  o.nrm = v.nrm;
  o.world = v.pos;
  o.tint = v.tint;
  return o;
}

@fragment
fn fs_mesh(i: VOut) -> @location(0) vec4<f32> {
  let n = normalize(i.nrm);
  let to_eye = normalize(g.eye.xyz - i.world);
  let key = max(dot(n, normalize(g.light.xyz)), 0.0);
  let hemi = mix(vec3<f32>(0.045, 0.06, 0.085), vec3<f32>(0.34, 0.38, 0.44), n.y * 0.5 + 0.5);
  let rim = pow(1.0 - max(dot(n, to_eye), 0.0), 2.8) * 0.42;
  let albedo = mix(g.base.rgb, g.highlight.rgb, i.tint);
  var c = albedo * (hemi + key * 0.62);
  c = c + vec3<f32>(0.18, 0.30, 0.48) * rim;
  c = c + g.highlight.rgb * i.tint * 0.22;
  // Canvas presents Rgba8Unorm, so the guest owns the transfer function.
  return vec4<f32>(pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2)), 1.0);
}

struct MIn {
  @location(0) center: vec3<f32>,
  @location(1) corner: vec2<f32>,
  @location(2) tint: f32,
};
struct MOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) local: vec2<f32>,
  @location(1) tint: f32,
};

@vertex
fn vs_marker(m: MIn) -> MOut {
  var o: MOut;
  let d = distance(g.eye.xyz, m.center);
  let px = 12.0 * d * 0.0016;
  let world = m.center + (g.cam_right.xyz * m.corner.x + g.cam_up.xyz * m.corner.y) * px;
  o.clip = g.view_proj * vec4<f32>(world, 1.0);
  o.local = m.corner;
  o.tint = m.tint;
  return o;
}

@fragment
fn fs_marker(i: MOut) -> @location(0) vec4<f32> {
  let r = length(i.local);
  let core = 1.0 - smoothstep(0.42, 0.50, r);
  let ring = smoothstep(0.72, 0.80, r) * (1.0 - smoothstep(0.96, 1.0, r));
  let alpha = max(core, ring);
  if (alpha <= 0.01) { discard; }
  var rgb: vec3<f32>;
  if (i.tint < 0.5) { rgb = vec3<f32>(1.0, 0.64, 0.17); }
  else if (i.tint < 1.5) { rgb = vec3<f32>(0.42, 0.66, 1.0); }
  else { rgb = vec3<f32>(0.90, 0.28, 0.30); }
  return vec4<f32>(rgb, alpha);
}
"#;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    cam_right: [f32; 4],
    cam_up: [f32; 4],
    eye: [f32; 4],
    light: [f32; 4],
    base: [f32; 4],
    highlight: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct MeshVert {
    pos: [f32; 3],
    nrm: [f32; 3],
    tint: f32,
    _pad: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct MarkVert {
    center: [f32; 3],
    corner: [f32; 2],
    tint: f32,
    _pad: f32,
}

struct Scene {
    verts: Vec<MeshVert>,
    markers: Vec<([f32; 3], f32)>,
    radius: f32,
}

fn build_scene() -> Result<Scene, String> {
    let doc: serde_json::Value =
        serde_json::from_slice(SCENE).map_err(|e| format!("scene json: {e}"))?;
    let pos: Vec<f32> = doc["mesh"]["positions"]
        .as_array()
        .ok_or("mesh.positions")?
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();
    let idx: Vec<u32> = doc["mesh"]["indices"]
        .as_array()
        .ok_or("mesh.indices")?
        .iter()
        .map(|v| v.as_u64().unwrap_or(0) as u32)
        .collect();
    let assign: Vec<i64> = doc["mesh"]["assign"]
        .as_array()
        .ok_or("mesh.assign")?
        .iter()
        .map(|v| v.as_i64().unwrap_or(-1))
        .collect();

    let findings = doc["findings"].as_array().cloned().unwrap_or_default();
    let mut pointed: Vec<i64> = Vec::new();
    for f in &findings {
        for p in f["patch_ids"].as_array().cloned().unwrap_or_default() {
            if let Some(v) = p.as_i64() {
                pointed.push(v);
            }
        }
    }

    let mut nrm = vec![0.0f32; pos.len()];
    for t in 0..assign.len() {
        let (a, b, c) = (
            idx[t * 3] as usize,
            idx[t * 3 + 1] as usize,
            idx[t * 3 + 2] as usize,
        );
        let p = |i: usize| [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]];
        let (pa, pb, pc) = (p(a), p(b), p(c));
        let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let v = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for corner in [a, b, c] {
            nrm[corner * 3] += n[0];
            nrm[corner * 3 + 1] += n[1];
            nrm[corner * 3 + 2] += n[2];
        }
    }

    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for i in 0..pos.len() / 3 {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(pos[i * 3 + axis]);
            hi[axis] = hi[axis].max(pos[i * 3 + axis]);
        }
    }
    let centre = [
        (lo[0] + hi[0]) * 0.5,
        (lo[1] + hi[1]) * 0.5,
        (lo[2] + hi[2]) * 0.5,
    ];
    let radius = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2))
        .sqrt()
        * 0.5;

    let mut verts = Vec::with_capacity(assign.len() * 3);
    for t in 0..assign.len() {
        let tint = if pointed.contains(&assign[t]) { 1.0 } else { 0.0 };
        for corner in 0..3 {
            let v = idx[t * 3 + corner] as usize;
            let n = [nrm[v * 3], nrm[v * 3 + 1], nrm[v * 3 + 2]];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-9);
            verts.push(MeshVert {
                pos: [
                    pos[v * 3] - centre[0],
                    pos[v * 3 + 1] - centre[1],
                    pos[v * 3 + 2] - centre[2],
                ],
                nrm: [n[0] / len, n[1] / len, n[2] / len],
                tint,
                _pad: 0.0,
            });
        }
    }

    let mut markers = Vec::new();
    for f in &findings {
        let Some(a) = f["anchor"].as_array() else { continue };
        if a.len() != 3 {
            continue;
        }
        let severity = match f["severity"].as_str().unwrap_or("note") {
            "blocker" => 2.0,
            "warning" => 0.0,
            _ => 1.0,
        };
        markers.push((
            [
                a[0].as_f64().unwrap_or(0.0) as f32 - centre[0],
                a[1].as_f64().unwrap_or(0.0) as f32 - centre[1],
                a[2].as_f64().unwrap_or(0.0) as f32 - centre[2],
            ],
            severity,
        ));
    }

    Ok(Scene {
        verts,
        markers,
        radius: radius.max(1e-3),
    })
}

// ---------------------------------------------------------------- maths

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    [v[0] / l, v[1] / l, v[2] / l]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn mul(a: [[f32; 4]; 4], b: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut out = [[0.0f32; 4]; 4];
    for col in 0..4 {
        for row in 0..4 {
            out[col][row] = (0..4).map(|k| a[k][row] * b[col][k]).sum();
        }
    }
    out
}

fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> [[f32; 4]; 4] {
    let f = 1.0 / (fovy * 0.5).tan();
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, (near * far) / (near - far), 0.0],
    ]
}

fn look_at(eye: [f32; 3], centre: [f32; 3], up: [f32; 3]) -> [[f32; 4]; 4] {
    let f = normalize([
        centre[0] - eye[0],
        centre[1] - eye[1],
        centre[2] - eye[2],
    ]);
    let s = normalize(cross(f, up));
    let u = cross(s, f);
    [
        [s[0], u[0], -f[0], 0.0],
        [s[1], u[1], -f[1], 0.0],
        [s[2], u[2], -f[2], 0.0],
        [
            -(s[0] * eye[0] + s[1] * eye[1] + s[2] * eye[2]),
            -(u[0] * eye[0] + u[1] * eye[1] + u[2] * eye[2]),
            f[0] * eye[0] + f[1] * eye[1] + f[2] * eye[2],
            1.0,
        ],
    ]
}

// ---------------------------------------------------------------- render

fn render() -> Result<(), String> {
    let (width, height) = surface::size();
    let aspect = width as f32 / height.max(1) as f32;
    let scene = build_scene()?;

    let instance = wgpu_wasi_webgpu::instance();
    let adapter = wit_bindgen::block_on(instance.request_adapter(&Default::default()))
        .map_err(|e| format!("request-adapter: {e}"))?;
    let (device, queue) = wit_bindgen::block_on(adapter.request_device(&Default::default()))
        .map_err(|e| format!("request-device: {e}"))?;

    let globals = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dfm globals"),
        size: std::mem::size_of::<Globals>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("dfm globals layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("dfm globals bind"),
        layout: &bind_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: globals.as_entire_binding(),
        }],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("dfm layout"),
        bind_group_layouts: &[Some(&bind_layout)],
        immediate_size: 0,
    });

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("dfm shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let format = wgpu::TextureFormat::Rgba8Unorm;

    let mesh_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("dfm mesh"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_mesh"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<MeshVert>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_mesh"),
            compilation_options: Default::default(),
            targets: &[Some(format.into())],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });

    let marker_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("dfm markers"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_marker"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<MarkVert>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_marker"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });

    let mesh_verts = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dfm verts"),
        size: (scene.verts.len() * std::mem::size_of::<MeshVert>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&mesh_verts, 0, bytemuck::cast_slice(&scene.verts));

    let corners = [
        [-1.0f32, -1.0],
        [1.0, -1.0],
        [1.0, 1.0],
        [-1.0, -1.0],
        [1.0, 1.0],
        [-1.0, 1.0],
    ];
    let mut mverts = Vec::with_capacity(scene.markers.len() * 6);
    for (center, tint) in &scene.markers {
        for c in corners {
            mverts.push(MarkVert {
                center: *center,
                corner: c,
                tint: *tint,
                _pad: 0.0,
            });
        }
    }
    let marker_verts = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dfm markers"),
        size: (mverts.len().max(1) * std::mem::size_of::<MarkVert>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    if !mverts.is_empty() {
        queue.write_buffer(&marker_verts, 0, bytemuck::cast_slice(&mverts));
    }

    // Camera: the same default orbit the native viewer uses.
    let dist = scene.radius * 3.1;
    let (yaw, pitch) = (0.62f32, 0.42f32);
    let (cp, sp) = (pitch.cos(), pitch.sin());
    let eye = [
        dist * cp * yaw.sin(),
        dist * sp,
        dist * cp * yaw.cos(),
    ];
    let fwd = normalize([-eye[0], -eye[1], -eye[2]]);
    let right = normalize(cross(fwd, [0.0, 1.0, 0.0]));
    let up = cross(right, fwd);
    let view_proj = mul(
        perspective(38.0f32.to_radians(), aspect, 0.1, 20000.0),
        look_at(eye, [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    );
    let g = Globals {
        view_proj,
        cam_right: [right[0], right[1], right[2], 0.0],
        cam_up: [up[0], up[1], up[2], 0.0],
        eye: [eye[0], eye[1], eye[2], 1.0],
        light: [0.45, 0.75, 0.5, 0.0],
        base: [0.30, 0.33, 0.38, 1.0],
        highlight: [1.0, 0.62, 0.10, 1.0],
    };
    queue.write_buffer(&globals, 0, bytemuck::bytes_of(&g));

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("dfm target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let depth = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("dfm depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default());

    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("dfm"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.051,
                        g: 0.063,
                        b: 0.075,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(0, &bind, &[]);
        pass.set_pipeline(&mesh_pipeline);
        pass.set_vertex_buffer(0, mesh_verts.slice(..));
        pass.draw(0..scene.verts.len() as u32, 0..1);

        if !mverts.is_empty() {
            pass.set_pipeline(&marker_pipeline);
            pass.set_vertex_buffer(0, marker_verts.slice(..));
            pass.draw(0..mverts.len() as u32, 0..1);
        }
    }
    queue.submit([encoder.finish()]);

    let texture = wgpu_wasi_webgpu::host_texture(texture);
    surface::present(&texture).map_err(|e| format!("canvas present: {e}"))?;
    Ok(())
}

struct Component;

impl Guest for Component {
    async fn run() -> u32 {
        match render() {
            Ok(()) => 0,
            Err(error) => {
                // stderr is the only channel a guest gets; say what broke.
                eprintln!("dfm-report guest: {error}");
                1
            }
        }
    }
}

export!(Component);
