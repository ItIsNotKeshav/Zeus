//! A change of chat theme revealed from the card that chose it outwards.
//!
//! As fastframe-theme's `Transition` does for the middle of the window, it
//! keeps a picture of the old colours: [`Reveal::begin`] asks for a
//! screenshot while the window still shows them, [`Reveal::holding`] keeps
//! them until the picture arrives, and [`Reveal::paint`] lays the picture
//! over the new colours with a circle cut out of it, growing from the card
//! until the old colours are gone.

use std::sync::Arc;

use egui::{Color32, Context, Id, LayerId, Mesh, Order, Pos2, Rect, TextureHandle, Vec2};

/// How long the old colours are held for a screenshot that may never come
/// (a hidden window, or a renderer without screenshots), in seconds.
const WAIT: f64 = 0.25;
/// How long the circle takes to pass the farthest corner, in seconds.
const DURATION: f64 = crate::theme::MOTION_SCREEN as f64;
/// How wide the soft edge of the circle is, in points.
const FEATHER: f32 = 36.0;
/// How many sides the circle has.
const SEGMENTS: usize = 96;

/// Marks the screenshots this reveal asked for.
#[derive(Debug)]
struct Marker;

#[derive(Default)]
enum State {
    #[default]
    Idle,
    /// A theme card was picked here; the next change of colours opens
    /// from it.
    Armed { origin: Pos2 },
    /// A screenshot was asked for at this time.
    Capturing { origin: Pos2, since: f64 },
    /// The old colours are being revealed away since `start`.
    Revealing {
        origin: Pos2,
        picture: TextureHandle,
        start: f64,
    },
}

/// The reveal of a chat theme from where it was picked. Keep one per window.
#[derive(Default)]
pub struct Reveal {
    state: State,
}

impl std::fmt::Debug for Reveal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = match self.state {
            State::Idle => "idle",
            State::Armed { .. } => "armed",
            State::Capturing { .. } => "capturing",
            State::Revealing { .. } => "revealing",
        };
        formatter
            .debug_struct("Reveal")
            .field("state", &state)
            .finish()
    }
}

impl Reveal {
    /// Opens the next change of colours from `origin`, in screen points.
    pub fn arm(&mut self, origin: Pos2) {
        if matches!(self.state, State::Idle | State::Armed { .. }) {
            self.state = State::Armed { origin };
        }
    }

    /// Whether a change of colours should open from a picked card: one was
    /// armed, or its picture is on its way.
    pub fn wanted(&self) -> bool {
        matches!(self.state, State::Armed { .. } | State::Capturing { .. })
    }

    /// Asks for a picture of the window as this frame draws it, with the old
    /// colours. While a picture is already on its way it does nothing.
    pub fn begin(&mut self, ctx: &Context) {
        let State::Armed { origin } = self.state else {
            return;
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
            Marker,
        )));
        self.state = State::Capturing {
            origin,
            since: ctx.input(|input| input.time),
        };
        ctx.request_repaint();
    }

    /// Whether the app should keep drawing the old colours this frame: the
    /// picture was asked for and has not arrived yet.
    pub fn holding(&mut self, ctx: &Context) -> bool {
        let State::Capturing { origin, since } = self.state else {
            return false;
        };
        let picture = ctx.input(|input| {
            input.raw.events.iter().find_map(|event| match event {
                egui::Event::Screenshot {
                    user_data, image, ..
                } if user_data
                    .data
                    .as_ref()
                    .is_some_and(|data| data.downcast_ref::<Marker>().is_some()) =>
                {
                    Some(Arc::clone(image))
                }
                _ => None,
            })
        });
        let now = ctx.input(|input| input.time);
        if let Some(image) = picture {
            let picture = ctx.load_texture(
                "zeus-theme-reveal",
                Arc::unwrap_or_clone(image),
                egui::TextureOptions::LINEAR,
            );
            self.state = State::Revealing {
                origin,
                picture,
                start: now,
            };
            ctx.request_repaint();
            return false;
        }
        if now - since > WAIT {
            self.state = State::Idle;
            return false;
        }
        ctx.request_repaint();
        true
    }

    /// Lays the old picture over everything drawn this frame, with the
    /// circle grown further each frame. Call it last.
    pub fn paint(&mut self, ctx: &Context) {
        let State::Revealing {
            origin,
            picture,
            start,
        } = &self.state
        else {
            return;
        };
        let elapsed = ctx.input(|input| input.time) - start;
        if elapsed >= DURATION {
            self.state = State::Idle;
            return;
        }
        let t = (elapsed / DURATION) as f32;
        let progress = crate::theme::ease_out(t);
        let painter = ctx.layer_painter(LayerId::new(Order::Debug, Id::new("zeus-theme-reveal")));
        painter.add(circle(ctx.viewport_rect(), *origin, picture.id(), progress));
        ctx.request_repaint();
    }
}

/// The old picture covering `screen`, with a circle cleared around `origin`
/// that has grown `progress` (0 to 1) of the way past the farthest corner.
fn circle(screen: Rect, origin: Pos2, texture: egui::TextureId, progress: f32) -> Mesh {
    let corner = [
        screen.left_top(),
        screen.right_top(),
        screen.left_bottom(),
        screen.right_bottom(),
    ]
    .into_iter()
    .map(|corner| origin.distance(corner))
    .fold(0.0, f32::max);
    let hole = progress * (corner + FEATHER);
    let radii = [hole, hole + FEATHER, corner + FEATHER * 2.0];
    let alphas = [0.0, 1.0, 1.0];
    let size = screen.size().max(Vec2::splat(1.0));
    let mut mesh = Mesh::with_texture(texture);
    for (radius, alpha) in radii.into_iter().zip(alphas) {
        let tint = Color32::WHITE.gamma_multiply(alpha);
        for segment in 0..SEGMENTS {
            let angle = segment as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            let position = origin + Vec2::angled(angle) * radius;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: position,
                uv: Pos2::new(
                    (position.x - screen.min.x) / size.x,
                    (position.y - screen.min.y) / size.y,
                ),
                color: tint,
            });
        }
    }
    let ring = SEGMENTS as u32;
    for band in 0..2u32 {
        for segment in 0..ring {
            let next = (segment + 1) % ring;
            let (inner, inner_next) = (band * ring + segment, band * ring + next);
            let (outer, outer_next) = (inner + ring, inner_next + ring);
            mesh.add_triangle(inner, outer, outer_next);
            mesh.add_triangle(inner, outer_next, inner_next);
        }
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen() -> Rect {
        Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))
    }

    #[test]
    fn the_circle_opens_from_the_card_and_clears_the_far_corner() {
        let origin = Pos2::new(700.0, 500.0);
        let start = circle(screen(), origin, egui::TextureId::default(), 0.0);
        // At the start only the feather around the card is clear.
        for vertex in &start.vertices {
            if vertex.color.a() == 0 {
                assert!(vertex.pos.distance(origin) < 0.01);
            }
        }
        let end = circle(screen(), origin, egui::TextureId::default(), 1.0);
        let far = origin.distance(Pos2::ZERO);
        for vertex in &end.vertices {
            if vertex.color.a() > 0 {
                assert!(vertex.pos.distance(origin) > far);
            }
        }
    }

    #[test]
    fn the_picture_lines_up_with_the_screen() {
        let mesh = circle(
            screen(),
            Pos2::new(100.0, 80.0),
            egui::TextureId::default(),
            0.5,
        );
        for vertex in &mesh.vertices {
            let expected = Pos2::new(vertex.pos.x / 800.0, vertex.pos.y / 600.0);
            assert!((vertex.uv - expected).length() < 1e-4);
        }
    }

    #[test]
    fn only_an_armed_reveal_asks_for_a_picture() {
        let ctx = Context::default();
        let mut reveal = Reveal::default();
        reveal.begin(&ctx);
        assert!(!reveal.wanted());
        reveal.arm(Pos2::new(10.0, 10.0));
        assert!(reveal.wanted());
        reveal.begin(&ctx);
        assert!(matches!(reveal.state, State::Capturing { .. }));
    }
}
