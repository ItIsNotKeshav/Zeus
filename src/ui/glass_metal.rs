//! Frosted glass on macOS, where Zeus draws through wgpu's Metal backend:
//! the same frost as `glass.rs` draws with OpenGL, ported to wgpu.
//!
//! OpenGL could read back the frame from inside a paint callback; a wgpu
//! render pass cannot be read while it is open. The callback says it
//! [reads the backdrop](egui_wgpu::CallbackTrait::reads_backdrop), so egui
//! ends the pass just before it. [`Frost::backdrop`] then copies the region
//! sharp, draws it at a smaller size, and blurs that with separable
//! Gaussian passes; [`Frost::paint`] writes it back in the next pass,
//! blurred at the top and easing to sharp over the bottom `fade`, within
//! rounded corners. The strengths, shaders and saturation are those of
//! `glass.rs`. The work is done on frames that paint, and egui paints only
//! when something changes, so an idle window spends nothing on it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use eframe::egui_wgpu::{self, CallbackResources, CallbackTrait};
use eframe::wgpu;

/// How lightly a bar frosts: its copy is half the size, blurred once, so
/// what scrolls under it stays faintly legible.
const BAR: Strength = Strength {
    downscale: 2,
    passes: 1,
    progressive: false,
};
/// How deeply a menu frosts: a quarter-size copy blurred three times, so
/// only the colours of what lies under it come through, as iOS's do.
const MENU: Strength = Strength {
    downscale: 4,
    passes: 3,
    progressive: false,
};
/// How the chat list's header frosts: as deeply as a menu behind the
/// header, easing through the copy after one pass to sharp below it.
const PROGRESSIVE: Strength = Strength {
    downscale: 4,
    passes: 3,
    progressive: true,
};
/// Sets of textures kept, one for each size frosted in a frame (the
/// header, the composer, a menu), so none is reallocated every frame.
const TARGET_SETS: usize = 4;
/// How much more saturated the blur is than what it covers, as glass that
/// gathers the colours behind it.
const SATURATION: f32 = 1.3;
/// Uniform offsets must be aligned to this on every backend.
const SLOT: u64 = 256;

#[derive(Clone, Copy)]
struct Strength {
    /// How much smaller the blurred copy is than the region.
    downscale: u32,
    /// Horizontal and vertical passes over the small copy. Each widens the blur.
    passes: usize,
    /// Whether the copy after the first pass is kept, for the lighter blur
    /// a progressive frost eases through on its way to sharp.
    progressive: bool,
}

/// A paint callback that frosts `rect`, the bottom `fade` points easing from
/// blurred to sharp (the top ones, for a negative `fade`), within corners
/// rounded by `radius` points, the card's it lies across.
pub fn shape(rect: egui::Rect, fade: f32, radius: f32) -> egui::Shape {
    frost(rect, fade, radius, BAR)
}

/// A paint callback that frosts `rect` progressively: as deeply as a menu
/// down to the bottom `fade` points, then through a lighter blur to sharp.
pub fn progressive(rect: egui::Rect, fade: f32) -> egui::Shape {
    frost(rect, fade, 0.0, PROGRESSIVE)
}

/// A paint callback that frosts `rect` deeply, as under a menu, within
/// corners rounded by `radius` points.
pub fn rounded(rect: egui::Rect, radius: f32) -> egui::Shape {
    frost(rect, 0.0, radius, MENU)
}

fn frost(rect: egui::Rect, fade: f32, radius: f32, strength: Strength) -> egui::Shape {
    // Settings' Blur off: nothing is frosted, and what floats above is solid.
    if !crate::backdrop::blur() {
        return egui::Shape::Noop;
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    egui::Shape::Callback(egui_wgpu::Callback::new_paint_callback(
        rect,
        Frost {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            fade,
            radius,
            strength,
        },
    ))
}

struct Frost {
    /// Matches this callback's paint to its backdrop.
    id: u64,
    fade: f32,
    radius: f32,
    strength: Strength,
}

impl CallbackTrait for Frost {
    fn reads_backdrop(&self) -> bool {
        true
    }

    fn backdrop(
        &self,
        info: &egui::PaintCallbackInfo,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
        resources: &CallbackResources,
    ) {
        let Some(glass) = resources.get::<Mutex<Option<Glass>>>() else {
            return;
        };
        let mut glass = glass.lock().unwrap_or_else(|p| p.into_inner());
        // The pipelines are made for the window's format; a window drawing
        // to another one gets new ones.
        if glass
            .as_ref()
            .is_none_or(|glass| glass.format != target.format())
        {
            *glass = Some(Glass::new(device, target.format()));
        }
        if let Some(glass) = glass.as_mut() {
            glass.backdrop(self, info, device, encoder, target);
        }
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        let Some(glass) = resources.get::<Mutex<Option<Glass>>>() else {
            return;
        };
        let mut glass = glass.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(glass) = glass.as_mut() {
            glass.paint(self.id, render_pass);
        }
    }

    fn prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        // The objects are made on the first frost, once the format is known.
        if !resources.contains::<Mutex<Option<Glass>>>() {
            resources.insert(Mutex::new(None::<Glass>));
        }
        Vec::new()
    }
}

/// The pipelines, and the textures for each size frosted lately.
struct Glass {
    format: wgpu::TextureFormat,
    sampler: wgpu::Sampler,
    texture_layout: wgpu::BindGroupLayout,
    blur: wgpu::RenderPipeline,
    compose_layout: wgpu::BindGroupLayout,
    compose: wgpu::RenderPipeline,
    sets: Vec<Targets>,
    /// Counts frosts, to find the set used least lately.
    clock: u64,
    /// Frosts blurred and waiting for their paint, by callback.
    ready: Vec<Ready>,
}

/// One full-size and three small textures: two to blur between, and the
/// lighter blur a progressive frost keeps.
struct Targets {
    sharp: Target,
    small: [Target; 3],
    used: u64,
}

struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// What a frost's paint draws with: the region in pixels and the bind group
/// of its textures and uniforms.
struct Ready {
    id: u64,
    region: [u32; 4],
    bind_group: wgpu::BindGroup,
}

/// Uniforms of one blur pass: the distance between taps.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Step {
    step: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Compose {
    size: [f32; 2],
    fade: f32,
    saturation: f32,
    radius: f32,
    _pad: [f32; 3],
}

const SHADER: &str = r"
struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> Out {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: Out;
    out.position = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    // Texture rows run from the top.
    out.uv = vec2<f32>(p.x, 1.0 - p.y);
    return out;
}

@group(0) @binding(0) var t_source: texture_2d<f32>;
@group(0) @binding(1) var s_linear: sampler;

struct Step { step: vec2<f32> };
@group(1) @binding(0) var<uniform> u_step: Step;

// Nine taps folded into five linear samples; no step copies the source.
@fragment
fn fs_blur(in: Out) -> @location(0) vec4<f32> {
    let near = u_step.step * 1.3846153846;
    let far = u_step.step * 3.2307692308;
    return textureSample(t_source, s_linear, in.uv) * 0.2270270270
        + (textureSample(t_source, s_linear, in.uv + near)
            + textureSample(t_source, s_linear, in.uv - near)) * 0.3162162162
        + (textureSample(t_source, s_linear, in.uv + far)
            + textureSample(t_source, s_linear, in.uv - far)) * 0.0702702703;
}

struct Compose {
    size: vec2<f32>,
    fade: f32,
    saturation: f32,
    radius: f32,
};
@group(0) @binding(0) var t_sharp: texture_2d<f32>;
@group(0) @binding(1) var c_linear: sampler;
@group(0) @binding(2) var t_blur: texture_2d<f32>;
@group(0) @binding(3) var t_light: texture_2d<f32>;
@group(0) @binding(4) var<uniform> u: Compose;

fn saturate_blur(blur: vec4<f32>) -> vec4<f32> {
    let luma = dot(blur.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    // Colours are premultiplied, so they stay within their alpha.
    let rgb = clamp(mix(vec3<f32>(luma), blur.rgb, u.saturation), vec3<f32>(0.0), vec3<f32>(blur.a));
    return vec4<f32>(rgb, blur.a);
}

@fragment
fn fs_compose(in: Out) -> @location(0) vec4<f32> {
    // Measured from the bottom, as glass.rs measures it; a negative fade
    // eases towards the top instead, for a bar at the bottom.
    let up = 1.0 - in.uv.y;
    let edge = select(up, 1.0 - up, u.fade < 0.0);
    let fade = abs(u.fade);
    var frost = select(1.0, smoothstep(0.0, fade, edge), u.fade != 0.0);
    // A progressive frost is light at the bottom of its fade and deep from
    // just past its top; where both blurs are one texture this changes nothing.
    let deep = select(1.0, smoothstep(fade * 0.3, min(fade * 1.25, 1.0), edge), u.fade != 0.0);
    let blur = saturate_blur(mix(
        textureSample(t_light, c_linear, in.uv),
        textureSample(t_blur, c_linear, in.uv),
        deep,
    ));
    // Outside rounded corners what is there stays, antialiased at the edge.
    let half_size = u.size * 0.5;
    let q = abs(in.uv * u.size - half_size) - (half_size - vec2<f32>(u.radius));
    let outside = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - u.radius;
    frost = frost * clamp(0.5 - outside, 0.0, 1.0);
    return mix(textureSample(t_sharp, c_linear, in.uv), blur, frost);
}
";

impl Glass {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("zeus_glass"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler = wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let uniform = |binding, dynamic| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic,
                min_binding_size: None,
            },
            count: None,
        };
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("zeus_glass_texture"),
            entries: &[texture(0), sampler],
        });
        let step_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("zeus_glass_step"),
            entries: &[uniform(0, true)],
        });
        let compose_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("zeus_glass_compose"),
            entries: &[
                texture(0),
                sampler,
                texture(2),
                texture(3),
                uniform(4, false),
            ],
        });
        let pipeline = |label, layouts: &[Option<&wgpu::BindGroupLayout>], entry| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: layouts,
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    // Written over what is there, as glass.rs does with
                    // blending off.
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let blur = pipeline(
            "zeus_glass_blur",
            &[Some(&texture_layout), Some(&step_layout)],
            "fs_blur",
        );
        let compose = pipeline("zeus_glass_compose", &[Some(&compose_layout)], "fs_compose");
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("zeus_glass"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            format,
            sampler,
            texture_layout,
            blur,
            compose_layout,
            compose,
            sets: Vec::new(),
            clock: 0,
            ready: Vec::new(),
        }
    }

    /// The textures already `size` pixels, else the ones used least lately
    /// made anew, or new ones while there are fewer than [`TARGET_SETS`].
    fn targets(&mut self, device: &wgpu::Device, size: (u32, u32), small: (u32, u32)) -> usize {
        self.clock += 1;
        let fits = |set: &Targets| set.sharp.size() == size && set.small[0].size() == small;
        let at = match self.sets.iter().position(fits) {
            Some(at) => at,
            None => {
                let set = Targets {
                    sharp: Target::new(device, self.format, size),
                    small: std::array::from_fn(|_| Target::new(device, self.format, small)),
                    used: 0,
                };
                if self.sets.len() < TARGET_SETS {
                    self.sets.push(set);
                    self.sets.len() - 1
                } else {
                    let at = (0..self.sets.len())
                        .min_by_key(|&at| self.sets[at].used)
                        .unwrap_or(0);
                    self.sets[at] = set;
                    at
                }
            }
        };
        self.sets[at].used = self.clock;
        at
    }

    /// Copies and blurs what `target` holds under `info`'s rectangle, for
    /// `frost`'s paint.
    fn backdrop(
        &mut self,
        frost: &Frost,
        info: &egui::PaintCallbackInfo,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::Texture,
    ) {
        // A paint that never came (its rectangle left the screen) leaves
        // nothing behind for long.
        self.ready
            .retain(|ready| frost.id.wrapping_sub(ready.id) < 16);
        let region = info.viewport_in_pixels();
        let left = region.left_px.max(0) as u32;
        let top = region.top_px.max(0) as u32;
        let right = ((region.left_px + region.width_px).max(0) as u32).min(target.width());
        let bottom = ((region.top_px + region.height_px).max(0) as u32).min(target.height());
        if right <= left || bottom <= top {
            return;
        }
        let (w, h) = (right - left, bottom - top);
        let strength = frost.strength;
        let small = (
            (w / strength.downscale).max(1),
            (h / strength.downscale).max(1),
        );
        let at = self.targets(device, (w, h), small);
        let set = &self.sets[at];

        // The sharp copy.
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: left,
                    y: top,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            set.sharp.texture.as_image_copy(),
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );

        // Every pass's step, each in its own slot: no step for the smaller
        // copy, then across and down.
        let steps = [
            [0.0, 0.0],
            [1.0 / small.0 as f32, 0.0],
            [0.0, 1.0 / small.1 as f32],
        ];
        let mut bytes = vec![0u8; SLOT as usize * steps.len()];
        for (slot, step) in steps.iter().enumerate() {
            let step = Step {
                step: *step,
                _pad: [0.0; 2],
            };
            let at = slot * SLOT as usize;
            bytes[at..at + size_of::<Step>()].copy_from_slice(bytemuck::bytes_of(&step));
        }
        let step_buffer = buffer(device, &bytes);
        let step_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zeus_glass_step"),
            layout: &self.blur.get_bind_group_layout(1),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &step_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<Step>() as u64),
                }),
            }],
        });
        let source = |target: &Target| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("zeus_glass_source"),
                layout: &self.texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&target.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        };
        let sharp = source(&set.sharp);
        let smalls = [source(&set.small[0]), source(&set.small[1])];
        let mut pass = |from: &wgpu::BindGroup, to: &Target, slot: u32| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("zeus_glass_blur"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &to.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.blur);
            pass.set_bind_group(0, from, &[]);
            pass.set_bind_group(1, &step_group, &[slot * SLOT as u32]);
            pass.draw(0..3, 0..1);
        };
        // The small copy to blur.
        pass(&sharp, &set.small[0], 0);
        for at in 0..strength.passes {
            if at == 1 && strength.progressive {
                // The light blur, kept for the progressive frost's fade.
                pass(&smalls[0], &set.small[2], 0);
            }
            pass(&smalls[0], &set.small[1], 1);
            pass(&smalls[1], &set.small[0], 2);
        }

        // Without a lighter copy kept, the deep blur stands in for it.
        let light = if strength.progressive && strength.passes > 1 {
            2
        } else {
            0
        };
        let px = info.pixels_per_point;
        let uniforms = Compose {
            size: [w as f32, h as f32],
            fade: (frost.fade * px).round() / h as f32,
            saturation: SATURATION,
            radius: frost.radius * px,
            _pad: [0.0; 3],
        };
        let compose_buffer = buffer(device, bytemuck::bytes_of(&uniforms));
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zeus_glass_compose"),
            layout: &self.compose_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&set.sharp.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&set.small[0].view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&set.small[light].view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: compose_buffer.as_entire_binding(),
                },
            ],
        });
        self.ready.push(Ready {
            id: frost.id,
            region: [left, top, w, h],
            bind_group,
        });
    }

    /// Writes the blurred region back, within the clip egui has set.
    fn paint(&mut self, id: u64, render_pass: &mut wgpu::RenderPass<'static>) {
        let Some(at) = self.ready.iter().position(|ready| ready.id == id) else {
            return;
        };
        let ready = self.ready.swap_remove(at);
        let [x, y, w, h] = ready.region.map(|v| v as f32);
        render_pass.set_viewport(x, y, w, h, 0.0, 1.0);
        render_pass.set_pipeline(&self.compose);
        render_pass.set_bind_group(0, &ready.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

impl Target {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, size: (u32, u32)) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("zeus_glass"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { texture, view }
    }

    fn size(&self) -> (u32, u32) {
        (self.texture.width(), self.texture.height())
    }
}

/// A uniform buffer holding `bytes`, made for one frame: frosts of one
/// frame are encoded together, so a shared buffer rewritten for each would
/// give them all the last one's values.
fn buffer(device: &wgpu::Device, bytes: &[u8]) -> wgpu::Buffer {
    use wgpu::util::DeviceExt as _;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("zeus_glass_uniforms"),
        contents: bytes,
        usage: wgpu::BufferUsages::UNIFORM,
    })
}
