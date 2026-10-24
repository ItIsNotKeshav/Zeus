//! Halloween surprises: sending a message sometimes makes ghosts and
//! skeletons rain down the window or candy fall over it. Each is its own
//! switch under Settings, Appearance, Halloween.
//!
//! Both are drawn over the window and never take a click. They are made to
//! be safe for people sensitive to flashing: the shapes are small, fade in
//! and out once, never strobe or alternate colours, and neither runs while a
//! screen reader is attached (the setting the other decorative motion
//! follows) or during a call.

use egui::{Color32, Context, Id, LayerId, Mesh, Order, Pos2, Rect, Shape, Vec2, pos2, vec2};

/// The chance, per sent message, that ghosts and skeletons rain down.
pub const GHOST_CHANCE: f64 = 0.10;
/// The chance, per sent message, that candy falls.
pub const CANDY_CHANCE: f64 = 0.10;
/// How long the ghosts and skeletons fall, in seconds.
pub const GHOST_SECONDS: f64 = 2.0;
/// How long the candy falls, in seconds.
pub const CANDY_SECONDS: f64 = 2.0;

/// How many candies fall.
const CANDIES: usize = 36;
/// How many ghosts and skeletons fall.
const SPOOKS: usize = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Ghosts and skeletons.
    Ghost,
    Candy,
}

/// Which surprise, if any, a roll of two dice in `0..1` brings. The ghost
/// wins when both come up, so only one thing happens at a time.
pub fn pick(ghost_on: bool, candy_on: bool, ghost_roll: f64, candy_roll: f64) -> Option<Kind> {
    if ghost_on && ghost_roll < GHOST_CHANCE {
        Some(Kind::Ghost)
    } else if candy_on && candy_roll < CANDY_CHANCE {
        Some(Kind::Candy)
    } else {
        None
    }
}

#[derive(Debug)]
struct Running {
    kind: Kind,
    start: f64,
    seed: u64,
    /// Demo stills: the moment of the animation to hold, as a fraction.
    frozen: Option<f32>,
}

/// The surprise on screen, if any. Keep one per window.
#[derive(Debug, Default)]
pub struct Surprise {
    running: Option<Running>,
}

impl Surprise {
    /// Starts `kind` now, unless one is already running.
    pub fn start(&mut self, ctx: &Context, kind: Kind) {
        if self.running.is_some() {
            return;
        }
        let now = ctx.input(|input| input.time);
        self.running = Some(Running {
            kind,
            start: now,
            seed: (now * 1000.0) as u64 ^ 0x9e37_79b9_7f4a_7c15,
            frozen: None,
        });
        ctx.request_repaint();
    }

    /// Holds `kind` at `fraction` of its time, for demo stills.
    pub fn hold(&mut self, kind: Kind, fraction: f32) {
        self.running = Some(Running {
            kind,
            start: 0.0,
            seed: 7,
            frozen: Some(fraction),
        });
    }

    /// Whether one is on screen.
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// Draws the surprise over everything, and lets it go when it is over.
    pub fn paint(&mut self, ctx: &Context) {
        let Some(running) = &self.running else {
            return;
        };
        let seconds = match running.kind {
            Kind::Ghost => GHOST_SECONDS,
            Kind::Candy => CANDY_SECONDS,
        };
        let now = ctx.input(|input| input.time);
        let t = running
            .frozen
            .unwrap_or(((now - running.start) / seconds) as f32);
        if t >= 1.0 || crate::motion::screen_reader_attached(ctx) {
            self.running = None;
            return;
        }
        let painter =
            ctx.layer_painter(LayerId::new(Order::Tooltip, Id::new("halloween-surprise")));
        let screen = ctx.content_rect();
        match running.kind {
            Kind::Ghost => paint_ghosts_and_skeletons(&painter, screen, t, running.seed),
            Kind::Candy => paint_candies(&painter, screen, t, running.seed),
        }
        crate::motion::request_frame(ctx);
    }
}

fn ease_out(t: f32) -> f32 {
    egui::emath::easing::cubic_out(t.clamp(0.0, 1.0))
}

fn ease_in_out(t: f32) -> f32 {
    egui::emath::easing::cubic_in_out(t.clamp(0.0, 1.0))
}

/// How visible the falling spooks are at `t`: they ease in over the first
/// tenth and out over the last fifth, once.
fn spook_opacity(t: f32) -> f32 {
    ease_out(t / 0.1) * (1.0 - ease_in_out((t - 0.8) / 0.2))
}

const BONE: Color32 = Color32::from_rgb(244, 238, 224);
const GHOST_BODY: Color32 = Color32::from_rgb(246, 242, 255);
const INK: Color32 = Color32::from_rgb(36, 20, 46);

fn paint_ghosts_and_skeletons(painter: &egui::Painter, screen: Rect, t: f32, seed: u64) {
    let mut dice = Dice(seed);
    let seconds = GHOST_SECONDS as f32;
    let fade = spook_opacity(t);
    for index in 0..SPOOKS {
        let x = screen.left() + dice.next() * screen.width();
        let size = 26.0 + dice.next() * 26.0;
        let delay = dice.next() * 0.4;
        let sway = (dice.next() - 0.5) * 0.7;
        let phase = dice.next() * std::f32::consts::TAU;
        let progress = ((t - delay / seconds) / (1.0 - delay / seconds - 0.05)).clamp(0.0, 1.0);
        if progress <= 0.0 {
            continue;
        }
        let fall = progress.powf(1.4);
        let y = screen.top() - size * 2.0 + fall * (screen.height() + size * 4.0);
        // A gentle drift and tilt, never a spin.
        let drift = (progress * std::f32::consts::TAU * 1.2 + phase).sin();
        let at = pos2(x + drift * size * 0.5, y);
        let tilt = drift * sway;
        if index % 2 == 0 {
            ghost(painter, at, size, tilt, fade);
        } else {
            skeleton(painter, at, size, tilt, fade);
        }
    }
}

fn faded(colour: Color32, fade: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), (fade * 255.0) as u8)
}

/// Turns `point` by `angle` about `centre`.
fn about(centre: Pos2, point: Pos2, angle: f32) -> Pos2 {
    let (sin, cos) = angle.sin_cos();
    let offset = point - centre;
    centre
        + vec2(
            offset.x * cos - offset.y * sin,
            offset.x * sin + offset.y * cos,
        )
}

/// A little ghost `size` across: a sheet with a round head, a scalloped hem,
/// two eyes and an open mouth.
fn ghost(painter: &egui::Painter, centre: Pos2, size: f32, tilt: f32, fade: f32) {
    let outline: Vec<Pos2> = ghost_outline(centre, size)
        .into_iter()
        .map(|point| about(centre, point, tilt))
        .collect();
    painter.add(fan(&outline, centre, faded(GHOST_BODY, fade)));
    painter.add(Shape::closed_line(
        outline,
        egui::Stroke::new(1.2, faded(INK, fade * 0.7)),
    ));
    let eye = vec2(size * 0.07, size * 0.1);
    for side in [-1.0, 1.0] {
        let at = about(centre, centre + vec2(side * size * 0.18, -size * 0.1), tilt);
        painter.add(fan(&ellipse(at, eye), at, faded(INK, fade)));
    }
    let mouth = about(centre, centre + vec2(0.0, size * 0.14), tilt);
    painter.add(fan(
        &ellipse(mouth, vec2(size * 0.1, size * 0.14)),
        mouth,
        faded(INK, fade),
    ));
}

/// A little skeleton `size` tall: a skull over a spine, ribs, arms and a
/// pelvis, drawn in bone with a dark edge so it reads on any colour.
fn skeleton(painter: &egui::Painter, centre: Pos2, size: f32, tilt: f32, fade: f32) {
    let at = |x: f32, y: f32| about(centre, centre + vec2(x * size, y * size), tilt);
    let bones = |points: Vec<Pos2>| {
        painter.add(Shape::line(
            points.clone(),
            egui::Stroke::new(size * 0.15, faded(INK, fade)),
        ));
        painter.add(Shape::line(
            points,
            egui::Stroke::new(size * 0.075, faded(BONE, fade)),
        ));
    };
    bones(vec![at(0.0, -0.1), at(0.0, 0.34)]);
    for (y, half) in [(0.0, 0.2), (0.1, 0.22), (0.2, 0.18)] {
        bones(vec![at(-half, y + 0.05), at(0.0, y), at(half, y + 0.05)]);
    }
    bones(vec![at(-0.3, 0.3), at(-0.22, 0.04), at(-0.08, -0.06)]);
    bones(vec![at(0.3, 0.3), at(0.22, 0.04), at(0.08, -0.06)]);
    bones(vec![at(-0.14, 0.46), at(0.0, 0.34), at(0.14, 0.46)]);
    // The skull.
    let skull = at(0.0, -0.3);
    let head = ellipse(skull, vec2(size * 0.24, size * 0.22));
    painter.add(fan(&head, skull, faded(BONE, fade)));
    painter.add(Shape::closed_line(
        head,
        egui::Stroke::new(1.2, faded(INK, fade)),
    ));
    for side in [-1.0, 1.0] {
        let eye = at(side * 0.09, -0.31);
        painter.add(fan(
            &ellipse(eye, vec2(size * 0.055, size * 0.07)),
            eye,
            faded(INK, fade),
        ));
    }
    let nose = at(0.0, -0.23);
    painter.add(fan(
        &ellipse(nose, vec2(size * 0.02, size * 0.03)),
        nose,
        faded(INK, fade),
    ));
    painter.add(Shape::line(
        vec![at(-0.1, -0.17), at(0.1, -0.17)],
        egui::Stroke::new(size * 0.035, faded(INK, fade)),
    ));
}

/// The ghost's outline, `width` across: a round head, straight sides and
/// three scallops along the hem, listed so that a fan from its middle
/// covers it exactly once.
fn ghost_outline(centre: Pos2, width: f32) -> Vec<Pos2> {
    let radius = width / 2.0;
    let top = centre.y - radius * 0.35;
    let bottom = centre.y + radius * 0.95;
    let mut points = Vec::new();
    const HEAD: usize = 24;
    for step in 0..=HEAD {
        let angle = std::f32::consts::PI * (1.0 + step as f32 / HEAD as f32);
        points.push(pos2(
            centre.x + radius * angle.cos(),
            top + radius * 0.95 * angle.sin() + radius * 0.35,
        ));
    }
    points.push(pos2(centre.x + radius, bottom));
    // Three scallops, right to left; each bulges down.
    let bump = radius * 2.0 / 3.0;
    const ARC: usize = 8;
    for index in 0..3 {
        let right = centre.x + radius - bump * index as f32;
        let middle = right - bump / 2.0;
        for step in 0..=ARC {
            let angle = std::f32::consts::PI * step as f32 / ARC as f32;
            points.push(pos2(
                middle + bump / 2.0 * angle.cos(),
                bottom + radius * 0.16 * angle.sin(),
            ));
        }
    }
    points.push(pos2(centre.x - radius, bottom));
    points
}

fn ellipse(centre: Pos2, radii: Vec2) -> Vec<Pos2> {
    const SIDES: usize = 24;
    (0..SIDES)
        .map(|step| {
            let angle = std::f32::consts::TAU * step as f32 / SIDES as f32;
            pos2(
                centre.x + radii.x * angle.cos(),
                centre.y + radii.y * angle.sin(),
            )
        })
        .collect()
}

/// A single flat-coloured mesh of `outline` as a fan from `centre`, so that
/// translucent colour is laid once, never twice where shapes would overlap.
fn fan(outline: &[Pos2], centre: Pos2, colour: Color32) -> Shape {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(centre, colour);
    for point in outline {
        mesh.colored_vertex(*point, colour);
    }
    let count = outline.len() as u32;
    for index in 1..count {
        mesh.add_triangle(0, index, index + 1);
    }
    mesh.add_triangle(0, count, 1);
    Shape::mesh(mesh)
}

/// A small deterministic generator, so a candy's place needs no state.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }
}

const WRAPPERS: [Color32; 5] = [
    Color32::from_rgb(255, 122, 26),
    Color32::from_rgb(150, 80, 220),
    Color32::from_rgb(110, 200, 70),
    Color32::from_rgb(240, 80, 150),
    Color32::from_rgb(255, 205, 50),
];

fn paint_candies(painter: &egui::Painter, screen: Rect, t: f32, seed: u64) {
    let mut dice = Dice(seed);
    let seconds = CANDY_SECONDS as f32;
    let fade = 1.0 - ease_in_out((t - 0.85) / 0.15);
    for _ in 0..CANDIES {
        let x = screen.left() + dice.next() * screen.width();
        let size = 11.0 + dice.next() * 12.0;
        let delay = dice.next() * 0.45;
        let spin = (dice.next() - 0.5) * 8.0;
        let wrapper = WRAPPERS[(dice.next() * WRAPPERS.len() as f32) as usize % WRAPPERS.len()];
        let progress = ((t - delay / seconds) / (1.0 - delay / seconds - 0.05)).clamp(0.0, 1.0);
        if progress <= 0.0 {
            continue;
        }
        // Gravity: slow off the top, quicker toward the bottom.
        let fall = progress.powf(1.6);
        let y = screen.top() - size * 2.0 + fall * (screen.height() + size * 4.0);
        candy(
            painter,
            pos2(x, y),
            size,
            spin * progress * seconds,
            wrapper,
            fade,
        );
    }
}

/// A wrapped sweet: a round body with a twist of wrapper at each end.
fn candy(
    painter: &egui::Painter,
    centre: Pos2,
    size: f32,
    angle: f32,
    wrapper: Color32,
    fade: f32,
) {
    let alpha = (fade * 255.0) as u8;
    let tint = |colour: Color32| {
        Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), alpha)
    };
    let (sin, cos) = angle.sin_cos();
    let turn = |offset: Vec2| {
        centre
            + vec2(
                offset.x * cos - offset.y * sin,
                offset.x * sin + offset.y * cos,
            )
    };
    for side in [-1.0_f32, 1.0] {
        let near = turn(vec2(side * size * 0.6, 0.0));
        let high = turn(vec2(side * size * 1.7, -size * 0.7));
        let low = turn(vec2(side * size * 1.7, size * 0.7));
        painter.add(Shape::convex_polygon(
            vec![near, high, low],
            tint(wrapper),
            egui::Stroke::NONE,
        ));
    }
    painter.circle_filled(centre, size, tint(wrapper));
    painter.circle_filled(
        centre + vec2(-size * 0.3, -size * 0.3),
        size * 0.3,
        Color32::from_white_alpha((fade * 150.0) as u8),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chances_are_one_in_ten() {
        assert!((GHOST_CHANCE - 0.1).abs() < f64::EPSILON);
        assert!((CANDY_CHANCE - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn rolls_pick_one_surprise_and_respect_the_switches() {
        assert_eq!(pick(true, true, 0.05, 0.05), Some(Kind::Ghost));
        assert_eq!(pick(true, true, 0.5, 0.05), Some(Kind::Candy));
        assert_eq!(pick(true, true, 0.5, 0.5), None);
        assert_eq!(pick(false, true, 0.05, 0.05), Some(Kind::Candy));
        assert_eq!(pick(true, false, 0.05, 0.05), Some(Kind::Ghost));
        assert_eq!(pick(false, false, 0.0, 0.0), None);
    }

    #[test]
    fn the_spooks_fade_in_and_out_once_without_flicker() {
        let samples: Vec<f32> = (0..=100)
            .map(|step| spook_opacity(step as f32 / 100.0))
            .collect();
        assert!(samples[0] < 0.01 && samples[100] < 0.01);
        let peak = samples
            .iter()
            .position(|value| *value >= 0.99)
            .expect("fully visible at some point");
        // Rises to the peak, then only falls: a single swell, never a flash.
        assert!(samples[..=peak].windows(2).all(|pair| pair[1] >= pair[0]));
        let last_full = samples.iter().rposition(|value| *value >= 0.99).unwrap();
        assert!(
            samples[last_full..]
                .windows(2)
                .all(|pair| pair[1] <= pair[0])
        );
    }

    #[test]
    fn a_surprise_runs_its_time_and_stops() {
        let ctx = Context::default();
        let mut surprise = Surprise::default();
        for (kind, seconds) in [(Kind::Ghost, GHOST_SECONDS), (Kind::Candy, CANDY_SECONDS)] {
            let frame = |time: f64, surprise: &mut Surprise| {
                let _ = ctx.run_ui(
                    egui::RawInput {
                        time: Some(time),
                        ..Default::default()
                    },
                    |_| surprise.paint(&ctx),
                );
            };
            frame(100.0, &mut surprise);
            let _ = ctx.run_ui(
                egui::RawInput {
                    time: Some(100.0),
                    ..Default::default()
                },
                |_| surprise.start(&ctx, kind),
            );
            assert!(surprise.running());
            frame(100.0 + seconds / 2.0, &mut surprise);
            assert!(surprise.running());
            frame(100.0 + seconds + 0.1, &mut surprise);
            assert!(!surprise.running());
        }
    }
}
