//! Rounded clipping for the floating cards.
//!
//! egui clips every shape to a rectangle, so whatever a card holds (the
//! messages scrolled under a chat's header or composer, a settings page)
//! would show past the card's rounded corners. Flat patches in the gutter's
//! colour hid that only while the gutter was opaque: with Glass letting the
//! desktop through there is nothing to paint over the corners with. Instead,
//! each card says where it is and how it is rounded ([`card`]), and at the
//! end of the frame [`RoundedClip`] cuts the geometry of everything clipped
//! inside that card at its corners' curves, whatever lies behind it.

use egui::epaint::{ClippedShape, Mesh, Primitive, Tessellator, Vertex};
use egui::{Color32, CornerRadius, Pos2, Rect, Shape, ViewportId, vec2};

/// Segments along each corner's quarter circle. The header's veil steps
/// down its top corners through the same points
/// (`conversation::header::CORNER_STEPS`), so what is cut meets the veil
/// edge to edge: with a finer curve, slivers of the messages scrolled under
/// the header showed between the veil's chords and the cut.
pub(crate) const ARC_STEPS: usize = 8;

/// A card on screen this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Card {
    viewport: ViewportId,
    rect: Rect,
    radius: CornerRadius,
}

fn cards_id() -> egui::Id {
    egui::Id::new("rounded-clip-cards")
}

/// Rounds off what is clipped inside `rect` at its `radius` corners this
/// frame.
pub fn card(ctx: &egui::Context, rect: Rect, radius: CornerRadius) {
    if radius == CornerRadius::ZERO || !rect.is_positive() {
        return;
    }
    let viewport = ctx.viewport_id();
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Vec<Card>>(cards_id())
            .push(Card {
                viewport,
                rect,
                radius,
            });
    });
}

/// Cuts each registered card's contents at its rounded corners, in
/// `output_hook`, once every plugin has painted (emoji pictures included)
/// and the font atlas has its final size for the frame.
pub struct RoundedClip;

impl egui::plugin::Plugin for RoundedClip {
    fn debug_name(&self) -> &'static str {
        "zeus-rounded-clip"
    }

    fn output_hook(&mut self, ctx: &egui::Context, output: &mut egui::FullOutput) {
        // Only this viewport's cards: a window drawn inside another's frame
        // leaves the outer one's for its own end.
        let viewport = ctx.viewport_id();
        let cards = ctx.data_mut(|data| {
            let all = data.get_temp_mut_or_default::<Vec<Card>>(cards_id());
            let (mine, rest): (Vec<Card>, Vec<Card>) =
                all.drain(..).partition(|card| card.viewport == viewport);
            *all = rest;
            mine
        });
        if cards.is_empty() {
            return;
        }
        let options = ctx.tessellation_options(|options| *options);
        let font_size = ctx.fonts(|fonts| fonts.font_image_size());
        let mut tessellator = Tessellator::new(output.pixels_per_point, options, font_size, vec![]);
        for clipped in &mut output.shapes {
            let Some(card) = cards
                .iter()
                .find(|card| inside(card.rect, clipped.clip_rect))
            else {
                continue;
            };
            let corners = Corners::new(card);
            if !corners.reaches(clipped) {
                continue;
            }
            let shape = std::mem::replace(&mut clipped.shape, Shape::Noop);
            clipped.shape = corners.cut(&mut tessellator, clipped.clip_rect, shape);
        }
    }
}

/// Whether a shape clipped to `clip` is the card's: its clip lies within it.
fn inside(card: Rect, clip: Rect) -> bool {
    card.expand(0.01).contains_rect(clip) && clip.is_positive()
}

/// One rounded corner: the square it rounds off, and the chords of its curve.
struct Corner {
    square: Rect,
    /// Points along the curve from one side of the card to the other.
    arc: Vec<Pos2>,
}

/// A card's rounded corners, each as the convex curve the geometry is cut on.
struct Corners {
    rect: Rect,
    radius: CornerRadius,
    corners: Vec<Corner>,
}

impl Corners {
    /// The corners of `card`, on the card's own curve, where the flat
    /// patches that covered them ended: what is cut there meets the card's
    /// rounded fills with no sliver showing past them. Fills rounded as the
    /// card is are never cut ([`Self::reaches`]), so their anti-aliased
    /// edges stay whole.
    fn new(card: &Card) -> Self {
        use std::f32::consts::FRAC_PI_2;
        let rect = card.rect;
        let radius = card.radius;
        let mut corners = Vec::new();
        // Each corner: the card's corner, the way into the card, the quarter
        // the curve starts at, and its radius.
        for (corner, inward, quarter, r) in [
            (rect.left_top(), vec2(1.0, 1.0), 2.0, radius.nw),
            (rect.right_top(), vec2(-1.0, 1.0), 3.0, radius.ne),
            (rect.right_bottom(), vec2(-1.0, -1.0), 0.0, radius.se),
            (rect.left_bottom(), vec2(1.0, -1.0), 1.0, radius.sw),
        ] {
            let r = f32::from(r)
                .min(rect.width() / 2.0)
                .min(rect.height() / 2.0);
            if r <= 0.0 {
                continue;
            }
            let centre = corner + inward * r;
            let arc = (0..=ARC_STEPS)
                .map(|step| {
                    let angle = (quarter + step as f32 / ARC_STEPS as f32) * FRAC_PI_2;
                    centre + r * vec2(angle.cos(), angle.sin())
                })
                .collect();
            corners.push(Corner {
                square: Rect::from_two_pos(corner, centre),
                arc,
            });
        }
        Self {
            rect,
            radius,
            corners,
        }
    }

    /// Whether any of `clipped` may show in a corner outside its curve. A
    /// fill rounded as the card is (its colour, wallpaper or picture)
    /// already ends at the curve, with its own anti-aliased edge; the
    /// outline is cut with the rest, as the patches covered it.
    fn reaches(&self, clipped: &ClippedShape) -> bool {
        if let Shape::Rect(shape) = &clipped.shape
            && shape.rect == self.rect
            && shape.stroke.is_empty()
            && rounds_off(shape.corner_radius, self.radius)
        {
            return false;
        }
        let area = clipped
            .shape
            .visual_bounding_rect()
            .intersect(clipped.clip_rect);
        area.is_positive()
            && self
                .corners
                .iter()
                .any(|corner| corner.square.intersects(area))
    }

    /// `shape` tessellated and cut at the corners it reaches. Paint
    /// callbacks (the frosted glass rounds itself) are kept as they are.
    fn cut(&self, tessellator: &mut Tessellator, clip_rect: Rect, shape: Shape) -> Shape {
        let mut primitives = Vec::new();
        tessellator.tessellate_clipped_shape(ClippedShape { clip_rect, shape }, &mut primitives);
        let shapes: Vec<Shape> = primitives
            .into_iter()
            .map(|primitive| match primitive.primitive {
                Primitive::Mesh(mesh) => Shape::mesh(self.cut_mesh(&mesh)),
                Primitive::Callback(callback) => Shape::Callback(callback),
            })
            .collect();
        Shape::Vec(shapes)
    }

    /// `mesh` without what lies outside the corners' curves: each triangle
    /// that reaches a corner's square is clipped by the chords of that
    /// corner's curve, the rest is kept whole.
    fn cut_mesh(&self, mesh: &Mesh) -> Mesh {
        let mut out = Mesh::with_texture(mesh.texture_id);
        let mut polygon = Vec::with_capacity(8);
        let mut scratch = Vec::with_capacity(8);
        for triangle in mesh.indices.as_chunks::<3>().0 {
            polygon.clear();
            polygon.extend(triangle.iter().map(|&index| mesh.vertices[index as usize]));
            let bounds = Rect::from_points(&[polygon[0].pos, polygon[1].pos, polygon[2].pos]);
            for corner in &self.corners {
                if !corner.square.intersects(bounds) {
                    continue;
                }
                for chord in corner.arc.windows(2) {
                    clip_by_chord(&polygon, chord[0], chord[1], &mut scratch);
                    std::mem::swap(&mut polygon, &mut scratch);
                    if polygon.len() < 3 {
                        break;
                    }
                }
                if polygon.len() < 3 {
                    break;
                }
            }
            if polygon.len() < 3 {
                continue;
            }
            let base = out.vertices.len() as u32;
            out.vertices.extend_from_slice(&polygon);
            for step in 1..polygon.len() as u32 - 1 {
                out.add_triangle(base, base + step, base + step + 1);
            }
        }
        out
    }
}

/// Whether a rectangle rounded by `shape` stays within corners of `card`.
fn rounds_off(shape: CornerRadius, card: CornerRadius) -> bool {
    shape.nw >= card.nw && shape.ne >= card.ne && shape.sw >= card.sw && shape.se >= card.se
}

/// The part of the convex `polygon` on the card's side of the chord from
/// `a` to `b`, which runs round the card clockwise on screen (y down): the
/// card is to its right. Positions, texture coordinates and colours are
/// interpolated where an edge crosses it.
fn clip_by_chord(polygon: &[Vertex], a: Pos2, b: Pos2, out: &mut Vec<Vertex>) {
    out.clear();
    let side = |p: Pos2| (b - a).x * (p.y - a.y) - (b - a).y * (p.x - a.x);
    for (index, &current) in polygon.iter().enumerate() {
        let next = polygon[(index + 1) % polygon.len()];
        let (here, there) = (side(current.pos), side(next.pos));
        if here >= 0.0 {
            out.push(current);
        }
        if (here >= 0.0) != (there >= 0.0) {
            let t = here / (here - there);
            out.push(lerp_vertex(current, next, t));
        }
    }
}

fn lerp_vertex(a: Vertex, b: Vertex, t: f32) -> Vertex {
    let channel = |x: u8, y: u8| egui::lerp(f32::from(x)..=f32::from(y), t).round() as u8;
    Vertex {
        pos: a.pos + (b.pos - a.pos) * t,
        uv: a.uv + (b.uv - a.uv) * t,
        color: Color32::from_rgba_premultiplied(
            channel(a.color.r(), b.color.r()),
            channel(a.color.g(), b.color.g()),
            channel(a.color.b(), b.color.b()),
            channel(a.color.a(), b.color.a()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn card() -> Card {
        Card {
            viewport: ViewportId::ROOT,
            rect: Rect::from_min_size(pos2(100.0, 50.0), vec2(400.0, 300.0)),
            radius: CornerRadius::same(14),
        }
    }

    /// Whether `p` is within the card's rounded outline, give or take `slack`.
    fn within(card: &Card, p: Pos2, slack: f32) -> bool {
        let r = 14.0;
        let inner = card.rect.shrink(r);
        card.rect.expand(slack).contains(p) && p.distance(inner.clamp(p)) <= r + slack
    }

    /// A square filling the card is cut at every corner's curve, and what is
    /// left still covers the rest of the card.
    #[test]
    fn a_square_fill_is_rounded_off() {
        let card = card();
        let corners = Corners::new(&card);
        let fill = ClippedShape {
            clip_rect: card.rect,
            shape: Shape::rect_filled(card.rect, 0.0, Color32::RED),
        };
        assert!(corners.reaches(&fill));
        let mut tessellator = Tessellator::new(1.0, Default::default(), [1, 1], vec![]);
        let Shape::Vec(shapes) = corners.cut(&mut tessellator, fill.clip_rect, fill.shape) else {
            panic!("a list of meshes");
        };
        let mut area = 0.0;
        for shape in shapes {
            let Shape::Mesh(mesh) = shape else {
                panic!("a mesh");
            };
            for vertex in &mesh.vertices {
                assert!(
                    within(&card, vertex.pos, 1.0),
                    "{:?} past a corner",
                    vertex.pos
                );
            }
            for triangle in mesh.indices.as_chunks::<3>().0 {
                let [a, b, c] = [0, 1, 2].map(|i| mesh.vertices[triangle[i] as usize].pos);
                area += ((b - a).x * (c - a).y - (b - a).y * (c - a).x).abs() / 2.0;
            }
        }
        // The rounded rectangle's area, a corner's square less its quarter
        // circle taken off four times.
        let rounded = 400.0 * 300.0 - 4.0 * (14.0f32.powi(2) - std::f32::consts::PI * 49.0);
        assert!((area - rounded).abs() < 60.0, "{area} against {rounded}");
    }

    /// Shapes away from the corners, others' shapes and the card's own
    /// rounded fill are left alone.
    #[test]
    fn only_what_reaches_a_corner_is_cut() {
        let card = card();
        let corners = Corners::new(&card);
        let middle = Rect::from_center_size(card.rect.center(), vec2(50.0, 50.0));
        assert!(!corners.reaches(&ClippedShape {
            clip_rect: card.rect,
            shape: Shape::rect_filled(middle, 0.0, Color32::RED),
        }));
        assert!(!corners.reaches(&ClippedShape {
            clip_rect: card.rect,
            shape: Shape::rect_filled(card.rect, 14.0, Color32::RED),
        }));
        assert!(corners.reaches(&ClippedShape {
            clip_rect: card.rect,
            shape: Shape::rect_stroke(
                card.rect,
                14.0,
                (1.0, Color32::RED),
                egui::StrokeKind::Inside
            ),
        }));
        assert!(!inside(card.rect, card.rect.expand(20.0)), "a popup's clip");
        assert!(inside(card.rect, card.rect.shrink(30.0)));
    }

    /// Colours and texture coordinates are interpolated where an edge is cut.
    #[test]
    fn a_cut_edge_interpolates_its_vertex() {
        let a = Vertex {
            pos: pos2(0.0, 0.0),
            uv: pos2(0.0, 0.0),
            color: Color32::from_rgba_premultiplied(0, 0, 0, 0),
        };
        let b = Vertex {
            pos: pos2(10.0, 0.0),
            uv: pos2(1.0, 0.5),
            color: Color32::from_rgba_premultiplied(200, 100, 50, 250),
        };
        let mid = lerp_vertex(a, b, 0.5);
        assert_eq!(mid.pos, pos2(5.0, 0.0));
        assert_eq!(mid.uv, pos2(0.5, 0.25));
        assert_eq!(
            mid.color,
            Color32::from_rgba_premultiplied(100, 50, 25, 125)
        );
    }
}
