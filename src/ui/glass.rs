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
//! The chat list's header frosts progressively ([`sheet_progressive`]): deeply
//! behind the title, search and filters, so nothing under them can be read,
//! then more lightly through the tail below, then sharp.
//!
//! Menus frost more deeply ([`rounded`]): a smaller copy, blurred more
//! times, written back only within the menu's rounded corners. The work is
//! done on frames that paint, and egui paints only when something changes,
//! so an idle window spends nothing on it.
//!
//! The window's standing glass, over the chat list and the chat (their
//! headers, the pinned line, the composer and the newest-message button),
//! shares one sheet ([`sheet_shape`], [`sheet_rounded`],
//! [`sheet_progressive`]): the first of them to paint copies what lies under
//! all of them at once and blurs every copy in the same passes, and each
//! then lays its blur over what is there. Leaving the window's framebuffer
//! mid-frame is what a frost costs on Apple's tile-based GPUs, where it ends
//! and restarts the render pass, so a frame that scrolls a chat now leaves
//! it once rather than once for every frost (crmne/zapfast#55).
//!
//! So a frost may join the sheet only where everything under every frost on
//! it has been drawn by the time the first of them paints: the panels'
//! contents are drawn first, and the sheet's frosts after them, each on a
//! layer of its own above the panels. A frost whose place changed after it
//! joined (a sliding view) frosts on its own, as menus and dialogs always do.

use std::sync::{Arc, Mutex, Weak};

use eframe::egui_glow;
use eframe::glow::{self, HasContext};

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
/// Sets of render targets kept, one for each size frosted in a frame (the
/// header, the composer, a menu), so none is reallocated every frame.
const TARGET_SETS: usize = 4;
/// How much more saturated the blur is than what it covers, as glass that
/// gathers the colours behind it.
const SATURATION: f32 = 1.3;
/// The most frosts one sheet takes: the chat's header, pinned line, composer
/// and newest-message button, with room to spare.
const SHEET_PANES: usize = 8;

#[derive(Clone, Copy)]
struct Strength {
    /// How much smaller the blurred copy is than the region.
    downscale: i32,
    /// Horizontal and vertical passes over the small copy. Each widens the blur.
    passes: usize,
    /// Whether the copy after the first pass is kept, for the lighter blur
    /// a progressive frost eases through on its way to sharp.
    progressive: bool,
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
            with_glass(painter.gl(), |gl, glass| {
                // SAFETY: called on the painting thread with its context current.
                unsafe { glass.frost(gl, &info, fade, radius, strength) };
            });
        })),
    })
}

/// Starts this frame's sheet, which the chat's glass shares. Called before
/// the interface is laid out, every pass.
pub fn new_sheet() {
    // Where the desktop shows through, what lies under the glass is not
    // opaque and laying a blur over it would not match a blend of the two:
    // each frost there copies what it covers, as before.
    let sheet = (crate::backdrop::blur() && !crate::backdrop::active() && !sheet_off())
        .then(|| Arc::new(Sheet::default()));
    *SHEET.lock().unwrap_or_else(|p| p.into_inner()) = sheet;
}

/// A paint callback that frosts `rect` on this frame's sheet, the bottom
/// `fade` points easing from blurred to sharp (the top ones, for a negative
/// `fade`), within corners rounded by `radius` points, the card's it lies
/// across.
pub fn sheet_shape(rect: egui::Rect, fade: f32, radius: f32) -> egui::Shape {
    on_sheet(rect, fade, radius, BAR)
}

/// Whether `ZEUS_GLASS_SHEET=0` asks every frost to copy its own, for
/// comparing the two.
fn sheet_off() -> bool {
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OFF.get_or_init(|| {
        std::env::var("ZEUS_GLASS_SHEET")
            .is_ok_and(|value| matches!(value.as_str(), "0" | "off" | "false"))
    })
}

/// Like [`rounded`], on this frame's sheet.
pub fn sheet_rounded(rect: egui::Rect, radius: f32) -> egui::Shape {
    on_sheet(rect, 0.0, radius, MENU)
}

/// A paint callback that frosts `rect` progressively on this frame's sheet:
/// as deeply as a menu down to the bottom `fade` points, then through a
/// lighter blur to sharp.
pub fn sheet_progressive(rect: egui::Rect, fade: f32) -> egui::Shape {
    on_sheet(rect, fade, 0.0, PROGRESSIVE)
}

fn on_sheet(rect: egui::Rect, fade: f32, radius: f32, strength: Strength) -> egui::Shape {
    if !crate::backdrop::blur() {
        return egui::Shape::Noop;
    }
    let sheet = SHEET.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let Some(sheet) = sheet else {
        return frost(rect, fade, radius, strength);
    };
    let pane = {
        let mut panes = sheet.panes.lock().unwrap_or_else(|p| p.into_inner());
        // A sheet is for the few frosts of the chat's own glass; any more
        // frost on their own.
        if panes.len() >= SHEET_PANES {
            return frost(rect, fade, radius, strength);
        }
        panes.push(Pane { rect, strength });
        panes.len() - 1
    };
    egui::Shape::Callback(egui::PaintCallback {
        rect,
        callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
            let fade = (fade * info.pixels_per_point).round() as i32;
            let radius = radius * info.pixels_per_point;
            with_glass(painter.gl(), |gl, glass| {
                // SAFETY: called on the painting thread with its context current.
                unsafe {
                    if !glass.lay(gl, &info, &sheet, pane, fade, radius) {
                        glass.frost(gl, &info, fade, radius, strength);
                    }
                }
            });
        })),
    })
}

/// Runs `paint` with the objects for `gl`, making them first if the window
/// is new; nothing where the context cannot frost.
fn with_glass(gl: &Arc<glow::Context>, paint: impl FnOnce(&glow::Context, &mut Glass)) {
    let mut glass = GLASS.lock().unwrap_or_else(|p| p.into_inner());
    // A window made anew has a new context; the old one took its
    // objects with it. A context that cannot frost is asked once.
    if glass
        .as_ref()
        .is_none_or(|(made_for, _)| !made_for_this(made_for, gl))
    {
        // SAFETY: called on the painting thread with its context current.
        *glass = Some((Arc::downgrade(gl), unsafe { Glass::new(gl) }));
    }
    if let Some((_, Some(glass))) = glass.as_mut() {
        paint(gl, glass);
    }
}

/// This frame's sheet, while there is one.
static SHEET: Mutex<Option<Arc<Sheet>>> = Mutex::new(None);

/// The frosts of one frame that share a copy of the window: the first of
/// them to paint blurs all of them, and each then lays its own in place.
#[derive(Default)]
struct Sheet {
    /// Each frost's place and strength, in the order they joined.
    panes: Mutex<Vec<Pane>>,
    /// Where each one's blur was put, once the first has painted, and how
    /// many have been laid since.
    laid: Mutex<Option<(Vec<Option<Tile>>, usize)>>,
}

#[derive(Clone, Copy)]
struct Pane {
    rect: egui::Rect,
    strength: Strength,
}

/// A frosted region in the window, in pixels from its bottom left, and where
/// its small blurred copy is in the atlas.
#[derive(Clone, Copy)]
struct Tile {
    region: Region,
    at: (i32, i32),
    size: (i32, i32),
    passes: usize,
    /// Whether the copy after the first pass is kept, as for a lone
    /// progressive frost.
    progressive: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Region {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl Region {
    fn of(viewport: egui::epaint::ViewportInPixels) -> Self {
        Self {
            x: viewport.left_px,
            y: viewport.from_bottom_px,
            w: viewport.width_px,
            h: viewport.height_px,
        }
    }

    /// Whether it lies wholly within a window `screen` pixels in size; a
    /// copy of anything outside would be undefined.
    fn within(self, screen: [u32; 2]) -> bool {
        let fits = |start: i32, length: i32, limit: u32| {
            start >= 0 && length > 0 && i64::from(start) + i64::from(length) <= i64::from(limit)
        };
        fits(self.x, self.w, screen[0]) && fits(self.y, self.h, screen[1])
    }
}

/// Where each pane's small copy goes: one above the other, each `downscale`
/// times smaller than its region, and the size of the atlas that holds them.
/// A pane outside the window, or of no size, gets no tile.
fn layout(regions: &[(Option<Region>, Strength)]) -> (Vec<Option<Tile>>, (i32, i32)) {
    let mut size = (0, 0);
    let tiles = regions
        .iter()
        .map(|&(region, strength)| {
            let region = region?;
            let small = (
                (region.w / strength.downscale).max(1),
                (region.h / strength.downscale).max(1),
            );
            let tile = Tile {
                region,
                at: (0, size.1),
                size: small,
                passes: strength.passes,
                progressive: strength.progressive && strength.passes > 1,
            };
            size = (size.0.max(small.0), size.1 + small.1);
            Some(tile)
        })
        .collect();
    (tiles, size)
}

/// The objects for the current context, keyed by it, or nothing where it
/// cannot run them.
static GLASS: Mutex<Option<(Weak<glow::Context>, Option<Glass>)>> = Mutex::new(None);

/// Whether the objects made for `made_for` belong to `context`. The weak
/// reference keeps the old context's allocation, so a context made after it
/// closes cannot take its address. A bare address often went to the next
/// window's context, and the stale object names then drew over that
/// window's own textures, garbling it after a close and reopen.
fn made_for_this<T>(made_for: &Weak<T>, context: &Arc<T>) -> bool {
    std::ptr::eq(made_for.as_ptr(), Arc::as_ptr(context))
}

/// The two programs, and the render targets for each size frosted lately.
struct Glass {
    blur: glow::Program,
    compose: glow::Program,
    vao: glow::VertexArray,
    sets: Vec<Targets>,
    /// Counts frosts, to find the set used least lately.
    clock: u64,
    /// What a sheet blurs with, or nothing where its programs would not
    /// build; its frosts then go on their own.
    sheet: Option<SheetObjects>,
}

/// The programs a sheet blurs and lays with, and the atlas its small copies
/// are blurred back and forth between, with a third for the lighter blur
/// progressive frosts keep.
struct SheetObjects {
    blur: glow::Program,
    lay: glow::Program,
    atlas: [Target; 3],
}

/// One full-size and three small render targets: two to blur between, and
/// the lighter blur a progressive frost keeps.
struct Targets {
    sharp: Target,
    small: [Target; 3],
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
uniform sampler2D u_light;
uniform float u_fade;
uniform float u_saturation;
uniform vec2 u_size;
uniform float u_radius;
in vec2 v_uv;
out vec4 o_color;
vec4 saturate(vec4 blur) {
    float luma = dot(blur.rgb, vec3(0.2126, 0.7152, 0.0722));
    // Colours are premultiplied, so they stay within their alpha.
    blur.rgb = clamp(mix(vec3(luma), blur.rgb, u_saturation), 0.0, blur.a);
    return blur;
}
void main() {
    // A negative fade eases towards the top instead, for a bar at the bottom.
    float edge = u_fade < 0.0 ? 1.0 - v_uv.y : v_uv.y;
    float frost = u_fade != 0.0 ? smoothstep(0.0, abs(u_fade), edge) : 1.0;
    // Where both blurs are the same texture this mix changes nothing; a
    // progressive frost is light at the bottom of its fade and deep from
    // just past its top.
    float fade = abs(u_fade);
    float deep = u_fade != 0.0 ? smoothstep(fade * 0.3, min(fade * 1.25, 1.0), edge) : 1.0;
    vec4 blur = saturate(mix(texture(u_light, v_uv), texture(u_blur, v_uv), deep));
    // Outside rounded corners what is there stays, antialiased at the edge.
    vec2 half_size = u_size * 0.5;
    vec2 q = abs(v_uv * u_size - half_size) - (half_size - vec2(u_radius));
    float outside = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - u_radius;
    frost *= clamp(0.5 - outside, 0.0, 1.0);
    o_color = mix(texture(u_sharp, v_uv), blur, frost);
}
";

/// [`BLUR`] over one tile of an atlas: its own corner of the texture,
/// sampled no further than its edge, as a texture of its own would be.
const SHEET_BLUR: &str = r"
uniform sampler2D u_texture;
uniform vec2 u_step;
uniform vec2 u_origin;
uniform vec2 u_extent;
uniform vec2 u_low;
uniform vec2 u_high;
in vec2 v_uv;
out vec4 o_color;
vec4 tap(vec2 uv) {
    return texture(u_texture, clamp(uv, u_low, u_high));
}
void main() {
    vec2 uv = u_origin + v_uv * u_extent;
    vec2 near = u_step * 1.3846153846;
    vec2 far = u_step * 3.2307692308;
    o_color = tap(uv) * 0.2270270270
        + (tap(uv + near) + tap(uv - near)) * 0.3162162162
        + (tap(uv + far) + tap(uv - far)) * 0.0702702703;
}
";

/// [`COMPOSE`] for a sheet: the tile's blur, faded and rounded as there,
/// laid over what is already in place by blending. Over an opaque window
/// that is the same mix of sharp and blurred, without a sharp copy.
const SHEET_LAY: &str = r"
uniform sampler2D u_blur;
uniform sampler2D u_light;
uniform vec2 u_origin;
uniform vec2 u_extent;
uniform vec2 u_low;
uniform vec2 u_high;
uniform float u_fade;
uniform float u_saturation;
uniform vec2 u_size;
uniform float u_radius;
in vec2 v_uv;
out vec4 o_color;
void main() {
    float edge = u_fade < 0.0 ? 1.0 - v_uv.y : v_uv.y;
    float frost = u_fade != 0.0 ? smoothstep(0.0, abs(u_fade), edge) : 1.0;
    float fade = abs(u_fade);
    float deep = u_fade != 0.0 ? smoothstep(fade * 0.3, min(fade * 1.25, 1.0), edge) : 1.0;
    vec2 uv = clamp(u_origin + v_uv * u_extent, u_low, u_high);
    vec4 blur = mix(texture(u_light, uv), texture(u_blur, uv), deep);
    float luma = dot(blur.rgb, vec3(0.2126, 0.7152, 0.0722));
    blur.rgb = clamp(mix(vec3(luma), blur.rgb, u_saturation), 0.0, blur.a);
    vec2 half_size = u_size * 0.5;
    vec2 q = abs(v_uv * u_size - half_size) - (half_size - vec2(u_radius));
    float outside = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - u_radius;
    frost *= clamp(0.5 - outside, 0.0, 1.0);
    o_color = blur * frost;
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
            let sheet = SheetObjects::new(gl, header);
            if sheet.is_none() {
                log::info!("shared glass unavailable: each frost copies its own");
            }
            Some(Self {
                blur,
                compose,
                vao,
                sets: Vec::new(),
                clock: 0,
                sheet,
            })
        }
    }

    /// Lays `pane`'s blur from `sheet` under `info`, blurring every pane of
    /// the sheet first if this is the first to paint. False where it has
    /// none, or its place moved since it joined: it then frosts on its own.
    unsafe fn lay(
        &mut self,
        gl: &glow::Context,
        info: &egui::PaintCallbackInfo,
        sheet: &Sheet,
        pane: usize,
        fade: i32,
        radius: f32,
    ) -> bool {
        let vao = self.vao;
        let Some(objects) = self.sheet.as_mut() else {
            return false;
        };
        let mut laid = sheet.laid.lock().unwrap_or_else(|p| p.into_inner());
        if laid.is_none() {
            let panes = sheet.panes.lock().unwrap_or_else(|p| p.into_inner());
            // SAFETY: called on the painting thread with its context current.
            let tiles = unsafe { objects.blur_all(gl, vao, info, &panes) };
            *laid = Some((tiles, 0));
        }
        let Some((tiles, count)) = laid.as_mut() else {
            return false;
        };
        let tile = tiles.get(pane).copied().flatten();
        *count += 1;
        // The last pane laid: should egui paint these shapes again, they
        // blur afresh rather than from whatever the atlas holds by then.
        if *count >= tiles.len() {
            *laid = None;
        }
        drop(laid);
        let Some(tile) = tile else {
            return false;
        };
        if tile.region != Region::of(info.viewport_in_pixels()) {
            return false;
        }
        // SAFETY: as above.
        unsafe { objects.lay(gl, vao, info, tile, fade, radius) };
        true
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
                        small: [Target::new(gl)?, Target::new(gl)?, Target::new(gl)?],
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
            let kept = if strength.progressive { 3 } else { 2 };
            for target in &mut set.small[..kept] {
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
            for pass in 0..strength.passes {
                if pass == 1 && strength.progressive {
                    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(set.small[0].framebuffer));
                    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(set.small[2].framebuffer));
                    gl.blit_framebuffer(
                        0,
                        0,
                        small.0,
                        small.1,
                        0,
                        0,
                        small.0,
                        small.1,
                        glow::COLOR_BUFFER_BIT,
                        glow::NEAREST,
                    );
                }
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
            // Without a lighter copy kept, the deep blur stands in for it.
            let light = if strength.progressive && strength.passes > 1 {
                2
            } else {
                0
            };
            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, Some(set.small[light].texture));
            let uniform = |name| gl.get_uniform_location(compose, name);
            gl.uniform_1_i32(uniform("u_sharp").as_ref(), 0);
            gl.uniform_1_i32(uniform("u_blur").as_ref(), 1);
            gl.uniform_1_i32(uniform("u_light").as_ref(), 2);
            gl.uniform_1_f32(uniform("u_fade").as_ref(), fade as f32 / h as f32);
            gl.uniform_1_f32(uniform("u_saturation").as_ref(), SATURATION);
            gl.uniform_2_f32(uniform("u_size").as_ref(), w as f32, h as f32);
            gl.uniform_1_f32(uniform("u_radius").as_ref(), radius);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
            // egui restores the rest of its state after a callback.
        }
    }
}

impl SheetObjects {
    unsafe fn new(gl: &glow::Context, header: &str) -> Option<Self> {
        unsafe {
            Some(Self {
                blur: program(gl, header, SHEET_BLUR)?,
                lay: program(gl, header, SHEET_LAY)?,
                atlas: [Target::new(gl)?, Target::new(gl)?, Target::new(gl)?],
            })
        }
    }

    /// Copies what lies under each pane, small, into the atlas and blurs
    /// them all, leaving every blur in the first of the atlas's two
    /// textures. Gives where each went.
    unsafe fn blur_all(
        &mut self,
        gl: &glow::Context,
        vao: glow::VertexArray,
        info: &egui::PaintCallbackInfo,
        panes: &[Pane],
    ) -> Vec<Option<Tile>> {
        let regions: Vec<_> = panes
            .iter()
            .map(|pane| {
                let at = egui::PaintCallbackInfo {
                    viewport: pane.rect,
                    clip_rect: pane.rect,
                    pixels_per_point: info.pixels_per_point,
                    screen_size_px: info.screen_size_px,
                };
                let region = Region::of(at.viewport_in_pixels());
                (
                    region.within(info.screen_size_px).then_some(region),
                    pane.strength,
                )
            })
            .collect();
        let (tiles, size) = layout(&regions);
        if size.0 <= 0 || size.1 <= 0 {
            return tiles;
        }
        let passes = tiles
            .iter()
            .flatten()
            .map(|tile| tile.passes)
            .max()
            .unwrap_or(0);
        unsafe {
            let screen = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
            let kept = if tiles.iter().flatten().any(|tile| tile.progressive) {
                3
            } else {
                2
            };
            for target in &mut self.atlas[..kept] {
                target.resize(gl, size);
            }
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            // Every copy, small, in one stretch away from the window.
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, screen);
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.atlas[0].framebuffer));
            for tile in tiles.iter().flatten() {
                let Region { x, y, w, h } = tile.region;
                gl.blit_framebuffer(
                    x,
                    y,
                    x + w,
                    y + h,
                    tile.at.0,
                    tile.at.1,
                    tile.at.0 + tile.size.0,
                    tile.at.1 + tile.size.1,
                    glow::COLOR_BUFFER_BIT,
                    glow::LINEAR,
                );
            }
            gl.bind_vertex_array(Some(vao));
            gl.use_program(Some(self.blur));
            gl.active_texture(glow::TEXTURE0);
            let uniform = |name| gl.get_uniform_location(self.blur, name);
            gl.uniform_1_i32(uniform("u_texture").as_ref(), 0);
            let (step, origin, extent, low, high) = (
                uniform("u_step"),
                uniform("u_origin"),
                uniform("u_extent"),
                uniform("u_low"),
                uniform("u_high"),
            );
            let atlas = (size.0 as f32, size.1 as f32);
            for pass in 0..passes {
                if pass == 1 {
                    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.atlas[0].framebuffer));
                    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.atlas[2].framebuffer));
                    for tile in tiles.iter().flatten().filter(|tile| tile.progressive) {
                        let (x, y) = tile.at;
                        let (w, h) = tile.size;
                        gl.blit_framebuffer(
                            x,
                            y,
                            x + w,
                            y + h,
                            x,
                            y,
                            x + w,
                            y + h,
                            glow::COLOR_BUFFER_BIT,
                            glow::NEAREST,
                        );
                    }
                }
                for (from, to, direction) in [(0, 1, (1.0, 0.0)), (1, 0, (0.0, 1.0))] {
                    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.atlas[to].framebuffer));
                    gl.bind_texture(glow::TEXTURE_2D, Some(self.atlas[from].texture));
                    gl.uniform_2_f32(step.as_ref(), direction.0 / atlas.0, direction.1 / atlas.1);
                    for tile in tiles.iter().flatten().filter(|tile| tile.passes > pass) {
                        gl.viewport(tile.at.0, tile.at.1, tile.size.0, tile.size.1);
                        set_tile(gl, tile, atlas, [&origin, &extent, &low, &high]);
                        gl.draw_arrays(glow::TRIANGLES, 0, 3);
                    }
                }
            }
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, screen);
        }
        tiles
    }

    /// Lays `tile`'s blur over the window under `info`, within its clip.
    unsafe fn lay(
        &self,
        gl: &glow::Context,
        vao: glow::VertexArray,
        info: &egui::PaintCallbackInfo,
        tile: Tile,
        fade: i32,
        radius: f32,
    ) {
        let Region { x, y, w, h } = tile.region;
        let clip = info.clip_rect_in_pixels();
        let atlas = self.atlas[0].size;
        let atlas = (atlas.0 as f32, atlas.1 as f32);
        unsafe {
            gl.viewport(x, y, w, h);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(
                clip.left_px,
                clip.from_bottom_px,
                clip.width_px,
                clip.height_px,
            );
            // Premultiplied: the blur where frosted, what is there elsewhere.
            gl.enable(glow::BLEND);
            gl.blend_equation(glow::FUNC_ADD);
            gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
            gl.bind_vertex_array(Some(vao));
            gl.use_program(Some(self.lay));
            // Without a lighter copy kept, the deep blur stands in for it.
            let light = if tile.progressive { 2 } else { 0 };
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.atlas[light].texture));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.atlas[0].texture));
            let uniform = |name| gl.get_uniform_location(self.lay, name);
            gl.uniform_1_i32(uniform("u_blur").as_ref(), 0);
            gl.uniform_1_i32(uniform("u_light").as_ref(), 1);
            set_tile(
                gl,
                &tile,
                atlas,
                [
                    &uniform("u_origin"),
                    &uniform("u_extent"),
                    &uniform("u_low"),
                    &uniform("u_high"),
                ],
            );
            gl.uniform_1_f32(uniform("u_fade").as_ref(), fade as f32 / h as f32);
            gl.uniform_1_f32(uniform("u_saturation").as_ref(), SATURATION);
            gl.uniform_2_f32(uniform("u_size").as_ref(), w as f32, h as f32);
            gl.uniform_1_f32(uniform("u_radius").as_ref(), radius);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.active_texture(glow::TEXTURE0);
            // egui restores the rest of its state after a callback.
        }
    }
}

/// Points a sheet program at `tile` of an atlas `atlas` pixels in size: its
/// corner and extent, and the texel centres at its edges, which samples are
/// held within.
unsafe fn set_tile(
    gl: &glow::Context,
    tile: &Tile,
    atlas: (f32, f32),
    [origin, extent, low, high]: [&Option<glow::UniformLocation>; 4],
) {
    let (x, y) = (tile.at.0 as f32, tile.at.1 as f32);
    let (w, h) = (tile.size.0 as f32, tile.size.1 as f32);
    unsafe {
        gl.uniform_2_f32(origin.as_ref(), x / atlas.0, y / atlas.1);
        gl.uniform_2_f32(extent.as_ref(), w / atlas.0, h / atlas.1);
        gl.uniform_2_f32(low.as_ref(), (x + 0.5) / atlas.0, (y + 0.5) / atlas.1);
        gl.uniform_2_f32(
            high.as_ref(),
            (x + w - 0.5) / atlas.0,
            (y + h - 0.5) / atlas.1,
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A context made after the old one closes is never taken for it, even
    /// where the allocator would hand the new one the old address.
    #[test]
    fn a_reopened_window_gets_new_objects() {
        let first = Arc::new([0u8; 64]);
        let made_for = Arc::downgrade(&first);
        assert!(made_for_this(&made_for, &first));
        drop(first);
        for _ in 0..64 {
            let next = Arc::new([0u8; 64]);
            assert!(!made_for_this(&made_for, &next));
        }
    }
}
