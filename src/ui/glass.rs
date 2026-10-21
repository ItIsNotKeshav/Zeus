//! Frosted glass under a vibrant header: what egui has already drawn beneath
//! a rectangle is blurred in place, as macOS blurs content scrolling under a
//! toolbar. egui has no blur of its own, so this is an OpenGL paint callback
//! that runs just before the header is drawn over it.
//!
//! The region is copied twice: sharp at full size, and at half of it,
//! where separable Gaussian passes blur it lightly, as macOS does: the
//! letters under a toolbar stay faintly legible. The last pass
//! writes it back, blurred at the top and easing to sharp over the bottom
//! `fade`, so the messages do not stop at a line. Callers keep it within
//! the header, as macOS does.
//!
//! Menus frost more deeply ([`rounded`]): a smaller copy, blurred more
//! times, written back only within the menu's rounded corners. The work is
//! done on frames that paint, and egui paints only when something changes,
//! so an idle window spends nothing on it.

use std::sync::{Arc, Mutex};

use eframe::egui_glow;
use eframe::glow::{self, HasContext};

/// How lightly a bar frosts: its copy is half the size, blurred once, so
/// what scrolls under it stays faintly legible.
const BAR: Strength = Strength {
    downscale: 2,
    passes: 1,
};
/// How deeply a menu frosts: a quarter-size copy blurred three times, so
/// only the colours of what lies under it come through, as iOS's do.
const MENU: Strength = Strength {
    downscale: 4,
    passes: 3,
};
/// Sets of render targets kept, one for each size frosted in a frame (the
/// header, the composer, a menu), so none is reallocated every frame.
const TARGET_SETS: usize = 4;
/// How much more saturated the blur is than what it covers, as glass that
/// gathers the colours behind it.
const SATURATION: f32 = 1.3;

#[derive(Clone, Copy)]
struct Strength {
    /// How much smaller the blurred copy is than the region.
    downscale: i32,
    /// Horizontal and vertical passes over the small copy. Each widens the blur.
    passes: usize,
}

/// A paint callback that frosts `rect`, the bottom `fade` points easing from
/// blurred to sharp (the top ones, for a negative `fade`), within corners
/// rounded by `radius` points, the card's it lies across.
pub fn shape(rect: egui::Rect, fade: f32, radius: f32) -> egui::Shape {
    frost(rect, fade, radius, BAR)
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
    egui::Shape::Callback(egui::PaintCallback {
        rect,
        callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
            let fade = (fade * info.pixels_per_point).round() as i32;
            let radius = radius * info.pixels_per_point;
            let gl = painter.gl();
            let mut glass = GLASS.lock().unwrap_or_else(|p| p.into_inner());
            // A window made anew has a new context; the old one took its
            // objects with it. A context that cannot frost is asked once.
            let context = Arc::as_ptr(gl) as usize;
            if glass
                .as_ref()
                .is_none_or(|(made_for, _)| *made_for != context)
            {
                // SAFETY: called on the painting thread with its context current.
                *glass = Some((context, unsafe { Glass::new(gl) }));
            }
            if let Some((_, Some(glass))) = glass.as_mut() {
                // SAFETY: as above.
                unsafe { glass.frost(gl, &info, fade, radius, strength) };
            }
        })),
    })
}

/// The objects for the current context, keyed by it, or nothing where it
/// cannot run them.
static GLASS: Mutex<Option<(usize, Option<Glass>)>> = Mutex::new(None);

/// The two programs, and the render targets for each size frosted lately.
struct Glass {
    blur: glow::Program,
    compose: glow::Program,
    vao: glow::VertexArray,
    sets: Vec<Targets>,
    /// Counts frosts, to find the set used least lately.
    clock: u64,
}

/// One full-size and two small render targets.
struct Targets {
    sharp: Target,
    small: [Target; 2],
    used: u64,
}

struct Target {
    texture: glow::Texture,
    framebuffer: glow::Framebuffer,
    size: (i32, i32),
}

const VERTEX: &str = r"
out vec2 v_uv;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_uv = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
";

/// Nine taps folded into five linear samples.
const BLUR: &str = r"
uniform sampler2D u_texture;
uniform vec2 u_step;
in vec2 v_uv;
out vec4 o_color;
void main() {
    vec2 near = u_step * 1.3846153846;
    vec2 far = u_step * 3.2307692308;
    o_color = texture(u_texture, v_uv) * 0.2270270270
        + (texture(u_texture, v_uv + near) + texture(u_texture, v_uv - near)) * 0.3162162162
        + (texture(u_texture, v_uv + far) + texture(u_texture, v_uv - far)) * 0.0702702703;
}
";

const COMPOSE: &str = r"
uniform sampler2D u_sharp;
uniform sampler2D u_blur;
uniform float u_fade;
uniform float u_saturation;
uniform vec2 u_size;
uniform float u_radius;
in vec2 v_uv;
out vec4 o_color;
void main() {
    vec4 blur = texture(u_blur, v_uv);
    float luma = dot(blur.rgb, vec3(0.2126, 0.7152, 0.0722));
    // Colours are premultiplied, so they stay within their alpha.
    blur.rgb = clamp(mix(vec3(luma), blur.rgb, u_saturation), 0.0, blur.a);
    // A negative fade eases towards the top instead, for a bar at the bottom.
    float edge = u_fade < 0.0 ? 1.0 - v_uv.y : v_uv.y;
    float frost = u_fade != 0.0 ? smoothstep(0.0, abs(u_fade), edge) : 1.0;
    // Outside rounded corners what is there stays, antialiased at the edge.
    vec2 half_size = u_size * 0.5;
    vec2 q = abs(v_uv * u_size - half_size) - (half_size - vec2(u_radius));
    float outside = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - u_radius;
    frost *= clamp(0.5 - outside, 0.0, 1.0);
    o_color = mix(texture(u_sharp, v_uv), blur, frost);
}
";

impl Glass {
    /// The programs and an empty vertex array, or nothing where the context
    /// cannot run them; the header then goes without the blur.
    unsafe fn new(gl: &glow::Context) -> Option<Self> {
        let version = egui_glow::ShaderVersion::get(gl);
        if !version.is_new_shader_interface() {
            log::info!("frosted header unavailable: {version:?} shaders");
            return None;
        }
        let header = version.version_declaration();
        unsafe {
            let blur = program(gl, header, BLUR)?;
            let compose = program(gl, header, COMPOSE)?;
            let vao = gl.create_vertex_array().ok()?;
            Some(Self {
                blur,
                compose,
                vao,
                sets: Vec::new(),
                clock: 0,
            })
        }
    }

    /// The targets already `size` pixels, else the ones used least lately,
    /// or new ones while there are fewer than [`TARGET_SETS`].
    unsafe fn targets(&mut self, gl: &glow::Context, size: (i32, i32)) -> Option<&mut Targets> {
        self.clock += 1;
        let at = match self.sets.iter().position(|set| set.sharp.size == size) {
            Some(at) => at,
            None if self.sets.len() < TARGET_SETS => {
                // SAFETY: called on the painting thread with its context current.
                let set = unsafe {
                    Targets {
                        sharp: Target::new(gl)?,
                        small: [Target::new(gl)?, Target::new(gl)?],
                        used: 0,
                    }
                };
                self.sets.push(set);
                self.sets.len() - 1
            }
            None => (0..self.sets.len()).min_by_key(|&at| self.sets[at].used)?,
        };
        let set = &mut self.sets[at];
        set.used = self.clock;
        Some(set)
    }

    /// Blurs the region of the bound framebuffer under `info`'s rectangle.
    unsafe fn frost(
        &mut self,
        gl: &glow::Context,
        info: &egui::PaintCallbackInfo,
        fade: i32,
        radius: f32,
        strength: Strength,
    ) {
        let region = info.viewport_in_pixels();
        let clip = info.clip_rect_in_pixels();
        let (x, y, w, h) = (
            region.left_px,
            region.from_bottom_px,
            region.width_px,
            region.height_px,
        );
        if w <= 0 || h <= 0 {
            return;
        }
        let small = (
            (w / strength.downscale).max(1),
            (h / strength.downscale).max(1),
        );
        let (blur, compose, vao) = (self.blur, self.compose, self.vao);
        unsafe {
            let Some(set) = self.targets(gl, (w, h)) else {
                return;
            };
            let screen = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
            set.sharp.resize(gl, (w, h));
            for target in &mut set.small {
                target.resize(gl, small);
            }
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            // The sharp copy, and the small one to blur.
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, screen);
            for (target, filter) in [(&set.sharp, glow::NEAREST), (&set.small[0], glow::LINEAR)] {
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(target.framebuffer));
                gl.blit_framebuffer(
                    x,
                    y,
                    x + w,
                    y + h,
                    0,
                    0,
                    target.size.0,
                    target.size.1,
                    glow::COLOR_BUFFER_BIT,
                    filter,
                );
            }
            gl.bind_vertex_array(Some(vao));
            gl.use_program(Some(blur));
            gl.active_texture(glow::TEXTURE0);
            gl.uniform_1_i32(gl.get_uniform_location(blur, "u_texture").as_ref(), 0);
            let step = gl.get_uniform_location(blur, "u_step");
            gl.viewport(0, 0, small.0, small.1);
            for _ in 0..strength.passes {
                for (from, to, direction) in [(0, 1, (1.0, 0.0)), (1, 0, (0.0, 1.0))] {
                    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(set.small[to].framebuffer));
                    gl.bind_texture(glow::TEXTURE_2D, Some(set.small[from].texture));
                    gl.uniform_2_f32(
                        step.as_ref(),
                        direction.0 / small.0 as f32,
                        direction.1 / small.1 as f32,
                    );
                    gl.draw_arrays(glow::TRIANGLES, 0, 3);
                }
            }
            // Back onto the screen, within the clip.
            gl.bind_framebuffer(glow::FRAMEBUFFER, screen);
            gl.viewport(x, y, w, h);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(
                clip.left_px,
                clip.from_bottom_px,
                clip.width_px,
                clip.height_px,
            );
            gl.use_program(Some(compose));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(set.sharp.texture));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(set.small[0].texture));
            let uniform = |name| gl.get_uniform_location(compose, name);
            gl.uniform_1_i32(uniform("u_sharp").as_ref(), 0);
            gl.uniform_1_i32(uniform("u_blur").as_ref(), 1);
            gl.uniform_1_f32(uniform("u_fade").as_ref(), fade as f32 / h as f32);
            gl.uniform_1_f32(uniform("u_saturation").as_ref(), SATURATION);
            gl.uniform_2_f32(uniform("u_size").as_ref(), w as f32, h as f32);
            gl.uniform_1_f32(uniform("u_radius").as_ref(), radius);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
            // egui restores the rest of its state after a callback.
        }
    }
}

impl Target {
    unsafe fn new(gl: &glow::Context) -> Option<Self> {
        unsafe {
            let texture = gl.create_texture().ok()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            for (parameter, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, parameter, value as i32);
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
            let framebuffer = gl.create_framebuffer().ok()?;
            Some(Self {
                texture,
                framebuffer,
                size: (0, 0),
            })
        }
    }

    /// Gives the texture `size` pixels, when it has another size.
    unsafe fn resize(&mut self, gl: &glow::Context, size: (i32, i32)) {
        if self.size == size {
            return;
        }
        self.size = size;
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA8 as i32,
                size.0,
                size.1,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(None),
            );
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.framebuffer));
            gl.framebuffer_texture_2d(
                glow::DRAW_FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(self.texture),
                0,
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }
}

/// Compiles and links the shared vertex shader with `fragment`.
unsafe fn program(gl: &glow::Context, header: &str, fragment: &str) -> Option<glow::Program> {
    unsafe {
        let program = gl.create_program().ok()?;
        let mut shaders = Vec::new();
        for (kind, source) in [
            (glow::VERTEX_SHADER, VERTEX),
            (glow::FRAGMENT_SHADER, fragment),
        ] {
            let shader = gl.create_shader(kind).ok()?;
            gl.shader_source(shader, &format!("{header}{source}"));
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                log::warn!("frosted header shader: {}", gl.get_shader_info_log(shader));
                return None;
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.link_program(program);
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        if !gl.get_program_link_status(program) {
            log::warn!(
                "frosted header program: {}",
                gl.get_program_info_log(program)
            );
            return None;
        }
        Some(program)
    }
}
