//! Halloween surprises: sending a message sometimes makes a ghost jump out
//! or candy fall over the window. Each is its own switch under Settings,
//! Appearance, Halloween.
//!
//! Both are drawn over the window and never take a click. They are made to
//! be safe for people sensitive to flashing: the ghost fades in and out once
//! over a dimmed window, with no strobing or alternating colours, and
//! neither runs while a screen reader is attached (the setting the other
//! decorative motion follows) or during a call.

use egui::{Color32, Context, Id, LayerId, Mesh, Order, Pos2, Rect, Shape, Vec2, pos2, vec2};

/// The chance, per sent message, that the ghost jumps out.
pub const GHOST_CHANCE: f64 = 0.10;
/// The chance, per sent message, that candy falls.
pub const CANDY_CHANCE: f64 = 0.10;
/// How long the ghost stays, in seconds.
pub const GHOST_SECONDS: f64 = 1.0;
/// How long the candy falls, in seconds.
pub const CANDY_SECONDS: f64 = 2.0;

/// How many candies fall.
const CANDIES: usize = 36;
/// How much of the window the ghost's dimming reaches at most.
const SCRIM: f32 = 0.55;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
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
            Kind::Ghost => paint_ghost(&painter, screen, t),
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

/// Eases out past 1 and settles back: the jump of the ghost.
fn ease_out_back(t: f32) -> f32 {
    const OVERSHOOT: f32 = 1.4;
    let t = t.clamp(0.0, 1.0) - 1.0;
    1.0 + (OVERSHOOT + 1.0) * t * t * t + OVERSHOOT * t * t
}

/// How visible the ghost is at `t` of its second: it eases in over the first
/// fifth and out over the last third, once.
fn ghost_opacity(t: f32) -> f32 {
    ease_out(t / 0.2) * (1.0 - ease_in_out((t - 0.65) / 0.35))
}

fn paint_ghost(painter: &egui::Painter, screen: Rect, t: f32) {
    let opacity = ghost_opacity(t);
    let scale = (0.35 + 0.65 * ease_out_back(t / 0.3)) * (1.0 + 0.05 * t);
    painter.rect_filled(
        screen,
        0.0,
        Color32::from_black_alpha((SCRIM * opacity * 255.0) as u8),
    );
    let width = screen.width().min(screen.height() * 0.8) * 0.8 * scale;
    let sway = (t * std::f32::consts::TAU * 1.5).sin() * width * 0.02;
    let centre = screen.center() + vec2(sway, -width * 0.05);
    let alpha = |value: f32| (value * opacity * 255.0) as u8;
    let body = Color32::from_rgba_unmultiplied(246, 242, 255, alpha(1.0));
    let glow = Color32::from_rgba_unmultiplied(255, 140, 40, alpha(0.22));
    let ink = Color32::from_rgba_unmultiplied(36, 20, 46, alpha(1.0));
    // A soft orange halo, then the ghost.
    let halo = ghost_outline(centre, width * 1.06);
    painter.add(fan(&halo, centre, glow));
    let outline = ghost_outline(centre, width);
    painter.add(fan(&outline, centre, body));
    // Eyes and the open mouth of a "boo".
    let eye = vec2(width * 0.085, width * 0.12);
    for side in [-1.0, 1.0] {
        let at = centre + vec2(side * width * 0.2, -width * 0.12);
        painter.add(fan(&ellipse(at, eye), at, ink));
    }
    let mouth = centre + vec2(0.0, width * 0.13);
    painter.add(fan(
        &ellipse(mouth, vec2(width * 0.12, width * 0.17)),
        mouth,
        ink,
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
    fn the_ghost_fades_in_and_out_once_without_flicker() {
        let samples: Vec<f32> = (0..=100)
            .map(|step| ghost_opacity(step as f32 / 100.0))
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
