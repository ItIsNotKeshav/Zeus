//! The chat wallpaper: a colour, the default doodles over it, or an image of
//! the reader's own in place of both.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};

use egui::{Color32, ColorImage, Rect, TextureHandle, TextureOptions, Vec2, pos2, vec2};

pub use crate::settings::DoodleStyle;

/// A doodle tile, drawn from Lucide icons (ISC, see
/// `assets/icons/LICENSE.txt`). They are embedded so the wallpaper works
/// offline and does not depend on a third-party request at runtime.
fn svg(style: DoodleStyle) -> &'static [u8] {
    match style {
        DoodleStyle::Classic => include_bytes!("../assets/wallpaper.svg"),
        DoodleStyle::Nature => include_bytes!("../assets/doodles/nature.svg"),
        DoodleStyle::Space => include_bytes!("../assets/doodles/space.svg"),
        DoodleStyle::Food => include_bytes!("../assets/doodles/food.svg"),
        DoodleStyle::Travel => include_bytes!("../assets/doodles/travel.svg"),
        DoodleStyle::Play => include_bytes!("../assets/doodles/play.svg"),
        DoodleStyle::Halloween => include_bytes!("../assets/doodles/halloween.svg"),
    }
}

/// The longest side a wallpaper image keeps, which bounds its texture to about
/// 26 MB even for a square picture while staying sharp on a 1440p display.
pub const MAX_SIDE: u32 = 2560;

/// The extensions a copied wallpaper image may have.
const EXTENSIONS: [&str; 4] = ["jpg", "png", "webp", "gif"];

#[derive(Clone, Default)]
struct Cache(Arc<Mutex<HashMap<DoodleStyle, TextureHandle>>>);

/// The uploaded custom image and which image it was made from.
#[derive(Clone)]
struct ImageTexture {
    generation: u64,
    texture: TextureHandle,
}

/// What the conversation shows behind its bubbles.
#[derive(Clone)]
pub struct Look {
    /// The background colour, with the Theme choice already resolved.
    pub color: Color32,
    /// Whether the doodles are drawn over the colour.
    pub doodles: bool,
    /// Which doodles those are.
    pub style: DoodleStyle,
    /// How strongly they show, as a multiple of the default.
    pub intensity: f32,
    /// The accent they are drawn in, or nothing for the grey ink.
    pub accent: Option<Color32>,
    /// Halloween's night sky, moon and skyline under the doodles.
    pub scene: bool,
    /// A decoded image of the reader's, which wins over colour and doodles.
    pub image: Option<(u64, Arc<ColorImage>)>,
}

/// Paint the wallpaper over the conversation panel, its colour rounded by
/// `radius` so a translucent one leaves a card's corners clear.
pub fn paint(ui: &egui::Ui, look: &Look, radius: f32) {
    paint_rounded(ui, ui.max_rect(), look, radius);
}

/// Paint the wallpaper into a bounded rectangle: the image cover-fitted if
/// there is one, otherwise the colour with the SVG's intrinsic 374 x 666
/// logical-pixel doodle tile repeated in both axes.
pub fn paint_rect(ui: &egui::Ui, rect: Rect, look: &Look) {
    paint_rounded(ui, rect, look, 0.0);
}

fn paint_rounded(ui: &egui::Ui, rect: Rect, look: &Look, radius: f32) {
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, radius, look.color);

    match &look.image {
        Some(image) => {
            let texture = image_texture(ui.ctx(), image);
            let uv = cover_uv(texture.size_vec2(), rect.size());
            painter.image(texture.id(), rect, uv, Color32::WHITE);
            return;
        }
        None => forget_image(ui.ctx()),
    }

    if look.scene {
        paint_sky(&painter, rect, look.color);
    }
    if look.doodles {
        paint_doodles(ui, rect, look, look.style);
    }
    if look.scene {
        paint_skyline(ui.ctx(), &painter, rect, look.color);
    }
}

/// Whether `color` is a dark background, for tinting what is drawn over it.
fn is_dark(color: Color32) -> bool {
    0.2126 * f32::from(color.r()) + 0.7152 * f32::from(color.g()) + 0.0722 * f32::from(color.b())
        < 128.0
}

/// Halloween's sky: the background colour warming towards an ember glow at
/// the horizon, and a moon with a soft halo high on the right.
fn paint_sky(painter: &egui::Painter, rect: Rect, color: Color32) {
    let dark = is_dark(color);
    let horizon = if dark {
        color.lerp_to_gamma(Color32::from_rgb(0xff, 0x6a, 0x10), 0.22)
    } else {
        color.lerp_to_gamma(Color32::from_rgb(0xff, 0x9a, 0x3c), 0.45)
    };
    let mid = color.lerp_to_gamma(horizon, 0.18);
    let mut mesh = egui::Mesh::default();
    for (fraction, tone) in [(0.0, color), (0.55, mid), (1.0, horizon)] {
        let y = rect.top() + rect.height() * fraction;
        mesh.colored_vertex(pos2(rect.left(), y), tone);
        mesh.colored_vertex(pos2(rect.right(), y), tone);
    }
    for step in 0..2 {
        let at = step * 2;
        mesh.add_triangle(at, at + 1, at + 2);
        mesh.add_triangle(at + 1, at + 2, at + 3);
    }
    painter.add(egui::Shape::mesh(mesh));

    let radius = (rect.width().min(rect.height()) * 0.055).clamp(16.0, 40.0);
    let center = pos2(
        rect.right() - (rect.width() * 0.2).max(radius * 2.4),
        rect.top() + (rect.height() * 0.17).max(radius * 2.2),
    );
    let (disc, halo) = if dark {
        (
            Color32::from_rgb(0xe9, 0xc9, 0x8f),
            Color32::from_rgb(0xff, 0x9a, 0x3c),
        )
    } else {
        (
            Color32::from_rgb(0xff, 0xd2, 0x8a),
            Color32::from_rgb(0xff, 0xd8, 0x9c),
        )
    };
    for ring in (1..=8).rev() {
        let grow = ring as f32 / 8.0;
        let alpha = (if dark { 5.0 } else { 7.0 } * (1.0 - grow) + 1.0) as u8;
        painter.circle_filled(
            center,
            radius * (1.0 + 2.2 * grow),
            Color32::from_rgba_unmultiplied(halo.r(), halo.g(), halo.b(), alpha),
        );
    }
    painter.circle_filled(center, radius, disc);
    let crater = Color32::from_rgba_unmultiplied(0xc9, 0x8a, 0x3a, if dark { 70 } else { 60 });
    painter.circle_filled(
        center + vec2(-radius * 0.3, -radius * 0.2),
        radius * 0.2,
        crater,
    );
    painter.circle_filled(
        center + vec2(radius * 0.25, radius * 0.3),
        radius * 0.28,
        crater,
    );
    painter.circle_filled(
        center + vec2(radius * 0.35, -radius * 0.35),
        radius * 0.12,
        crater,
    );
}

/// The skyline along the bottom edge: hills, a bare tree, graves, a patch of
/// pumpkins, in near-black at night and umber by day.
fn paint_skyline(ctx: &egui::Context, painter: &egui::Painter, rect: Rect, color: Color32) {
    let Some(texture) = scene_texture(ctx) else {
        return;
    };
    let size = texture.size_vec2();
    let width = rect.width();
    let height = width * size.y / size.x;
    // A tall window crops the sky out of the picture rather than stretching it.
    let shown = height.min(rect.height() * 0.42);
    let uv = Rect::from_min_max(pos2(0.0, 1.0 - shown / height), pos2(1.0, 1.0));
    let area = Rect::from_min_max(
        pos2(rect.left(), rect.bottom() - shown),
        rect.right_bottom(),
    );
    let tint = if is_dark(color) {
        Color32::from_rgba_unmultiplied(0x07, 0x03, 0x06, 0xf2)
    } else {
        Color32::from_rgba_unmultiplied(0x5a, 0x24, 0x08, 0xd8)
    };
    painter.image(texture.id(), area, uv, tint);
}

/// The skyline's mask, rendered once per window.
fn scene_texture(ctx: &egui::Context) -> Option<TextureHandle> {
    let id = egui::Id::new("halloween-scene");
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return Some(texture);
    }
    let tree = resvg::usvg::Tree::from_data(
        include_bytes!("../assets/halloween-scene.svg"),
        &resvg::usvg::Options::default(),
    )
    .ok()?;
    let (width, height) = (tree.size().width() as u32, tree.size().height() as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect::<Vec<u8>>();
    let image = ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &rgba);
    let texture = ctx.load_texture("halloween-scene", image, TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    Some(texture)
}

/// The doodles of `style` over `rect`, inked as `look` says for its
/// background, which the caller has already painted.
pub fn paint_doodles(ui: &egui::Ui, rect: Rect, look: &Look, style: DoodleStyle) {
    let painter = ui.painter().with_clip_rect(rect);
    let Some(texture) = texture(ui.ctx(), style) else {
        return;
    };

    let tile = texture.size_vec2();
    if tile.x <= 0.0 || tile.y <= 0.0 {
        return;
    }

    let line = ink(look);
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let origin = pos2(0.0, 0.0);
    let first_x = origin.x + (rect.left() - origin.x).div_euclid(tile.x) * tile.x;
    let first_y = origin.y + (rect.top() - origin.y).div_euclid(tile.y) * tile.y;

    let mut y = first_y;
    while y < rect.bottom() {
        let mut x = first_x;
        while x < rect.right() {
            let tile_rect = Rect::from_min_size(pos2(x, y), tile);
            if tile_rect.intersects(rect) {
                painter.image(texture.id(), tile_rect, uv, line);
            }
            x += tile.x;
        }
        y += tile.y;
    }
}

/// The doodles' tint for `look`: the grey ink of [`doodle_ink`], or the accent
/// at the same opacity, with the opacity scaled by the intensity.
fn ink(look: &Look) -> Color32 {
    let grey = doodle_ink(look.color);
    let intensity = if look.intensity.is_finite() {
        look.intensity.clamp(
            *crate::settings::DOODLE_INTENSITY.start(),
            *crate::settings::DOODLE_INTENSITY.end(),
        )
    } else {
        1.0
    };
    let alpha = (f32::from(grey.a()) * intensity).round().min(255.0) as u8;
    let [r, g, b] = match look.accent {
        Some(accent) => [accent.r(), accent.g(), accent.b()],
        None => [grey.r(), grey.g(), grey.b()],
    };
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// The doodles' tint over `background`: whichever of near-black or white
/// stands further from it, as opaque as it takes to differ from the
/// background by a fixed step. Any theme colour then shows the doodles about
/// as faintly as WhatsApp's own colours do, mid-tones included.
pub fn doodle_ink(background: Color32) -> Color32 {
    const DARK_INK: u8 = 30;
    const LIGHT_INK: u8 = 255;
    let luma = 0.2126 * f32::from(background.r())
        + 0.7152 * f32::from(background.g())
        + 0.0722 * f32::from(background.b());
    let (ink, step) = if luma - f32::from(DARK_INK) > f32::from(LIGHT_INK) - luma {
        (DARK_INK, 23.2)
    } else {
        (LIGHT_INK, 38.0)
    };
    let distance = (luma - f32::from(ink)).abs().max(1.0);
    let alpha = (step / distance).clamp(0.1, 0.4);
    Color32::from_rgba_unmultiplied(ink, ink, ink, (alpha * 255.0).round() as u8)
}

/// The part of an image of `image` size that covers `area` while keeping its
/// aspect ratio: the whole of its shorter dimension, centred on the longer.
pub fn cover_uv(image: Vec2, area: Vec2) -> Rect {
    let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    if image.x <= 0.0 || image.y <= 0.0 || area.x <= 0.0 || area.y <= 0.0 {
        return full;
    }
    let image_aspect = image.x / image.y;
    let area_aspect = area.x / area.y;
    if image_aspect > area_aspect {
        let visible = area_aspect / image_aspect;
        let left = (1.0 - visible) / 2.0;
        Rect::from_min_max(pos2(left, 0.0), pos2(left + visible, 1.0))
    } else {
        let visible = image_aspect / area_aspect;
        let top = (1.0 - visible) / 2.0;
        Rect::from_min_max(pos2(0.0, top), pos2(1.0, top + visible))
    }
}

fn image_texture_id() -> egui::Id {
    egui::Id::new("chat-wallpaper-image")
}

/// The custom image's texture, uploaded once per image and window.
fn image_texture(
    ctx: &egui::Context,
    (generation, image): &(u64, Arc<ColorImage>),
) -> TextureHandle {
    let cached = ctx.data(|data| data.get_temp::<ImageTexture>(image_texture_id()));
    if let Some(cached) = cached.filter(|cached| cached.generation == *generation) {
        return cached.texture;
    }
    let options = TextureOptions {
        mipmap_mode: Some(egui::TextureFilter::Linear),
        ..TextureOptions::LINEAR
    };
    let texture = ctx.load_texture("chat-wallpaper-image", Arc::clone(image), options);
    ctx.data_mut(|data| {
        data.insert_temp(
            image_texture_id(),
            ImageTexture {
                generation: *generation,
                texture: texture.clone(),
            },
        );
    });
    texture
}

/// Drops the custom image's texture, which frees it once nothing draws it.
pub fn forget_image(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<ImageTexture>(image_texture_id()));
}

/// The textures a window holds for the custom image and the doodle tile, for
/// telling them apart in painted shapes.
#[cfg(test)]
pub(crate) fn texture_ids(ctx: &egui::Context) -> (Option<egui::TextureId>, egui::TextureId) {
    let image = ctx.data(|data| data.get_temp::<ImageTexture>(image_texture_id()));
    let doodles = texture(ctx, DoodleStyle::Classic).expect("the doodle tile renders");
    (image.map(|image| image.texture.id()), doodles.id())
}

/// The tile of `style`, rendered once per window.
pub fn texture(ctx: &egui::Context, style: DoodleStyle) -> Option<TextureHandle> {
    let cache = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Cache>(egui::Id::new("chat-wallpaper"))
            .clone()
    });
    let mut cached = cache
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(texture) = cached.get(&style) {
        return Some(texture.clone());
    }

    let image = rasterize(style)?;
    let texture = ctx.load_texture(
        format!("chat-wallpaper-{style:?}"),
        image,
        TextureOptions::LINEAR,
    );
    cached.insert(style, texture.clone());
    Some(texture)
}

fn rasterize(style: DoodleStyle) -> Option<ColorImage> {
    // The source uses currentColor. Render a white mask once, then use the
    // painter tint to adapt the line colour and opacity to the active theme.
    let source = String::from_utf8_lossy(svg(style)).replace("currentColor", "#ffffff");
    let tree =
        resvg::usvg::Tree::from_data(source.as_bytes(), &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let width = size.width().round() as u32;
    let height = size.height().round() as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect::<Vec<u8>>();
    Some(ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        &rgba,
    ))
}

/// The wallpaper image named in the settings, decoded off the interface
/// thread. A file that is missing or cannot be decoded leaves the colour in
/// place and is not tried again until the setting changes.
#[derive(Default)]
pub struct CustomImage {
    /// The file last asked for.
    path: Option<PathBuf>,
    /// Bumped for every new file, so a window's texture of an earlier one is
    /// replaced even when the copy kept its name.
    generation: u64,
    image: Option<Arc<ColorImage>>,
    pending: Option<mpsc::Receiver<Result<ColorImage, String>>>,
}

impl CustomImage {
    /// Follows the setting: decodes a newly named file on a thread, drops the
    /// image when the setting is cleared, and collects a finished decode.
    pub fn sync(&mut self, wanted: Option<&Path>, waker: &crate::backend::Waker) {
        if self.path.as_deref() != wanted {
            self.path = wanted.map(Path::to_path_buf);
            self.generation += 1;
            self.image = None;
            self.pending = None;
            if let Some(path) = self.path.clone() {
                let (sender, receiver) = mpsc::channel();
                let waker = waker.clone();
                let spawned = std::thread::Builder::new()
                    .name("wallpaper-image".into())
                    .spawn(move || {
                        let _ = sender.send(decode(&path));
                        waker.wake();
                    });
                match spawned {
                    Ok(_) => self.pending = Some(receiver),
                    Err(error) => log::debug!("could not start decoding the wallpaper: {error}"),
                }
            }
        }
        let Some(pending) = &self.pending else {
            return;
        };
        match pending.try_recv() {
            Ok(Ok(image)) => {
                self.image = Some(Arc::new(image));
                self.pending = None;
            }
            Ok(Err(error)) => {
                let name = self
                    .path
                    .as_deref()
                    .and_then(Path::file_name)
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                log::debug!("wallpaper image {name} is not shown: {error}");
                self.pending = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
        }
    }

    /// Decodes the file again at the next [`Self::sync`], for a new image
    /// copied over the old one's name.
    pub fn reload(&mut self) {
        self.path = None;
    }

    /// The decoded image, if one is ready, with its generation.
    pub fn ready(&self) -> Option<(u64, Arc<ColorImage>)> {
        self.image
            .as_ref()
            .map(|image| (self.generation, Arc::clone(image)))
    }

    /// Shows `image` for `path` at once, for demos and layout tests.
    #[cfg(any(test, feature = "demo"))]
    pub fn show_now(&mut self, path: &Path, image: ColorImage) {
        self.path = Some(path.to_path_buf());
        self.generation += 1;
        self.image = Some(Arc::new(image));
        self.pending = None;
    }
}

/// Reads an image for display, no larger than [`MAX_SIDE`].
pub fn decode(path: &Path) -> Result<ColorImage, String> {
    let decoded = image::ImageReader::open(path)
        .and_then(image::ImageReader::with_guessed_format)
        .map_err(|error| error.to_string())?
        .decode()
        .map_err(|error| error.to_string())?;
    let rgba = bounded(decoded).to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

fn bounded(image: image::DynamicImage) -> image::DynamicImage {
    if image.width().max(image.height()) > MAX_SIDE {
        image.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        image
    }
}

/// Copies a chosen image to ZapFast's own `wallpaper.<ext>`, so the original
/// may move or go. An image larger than [`MAX_SIDE`] is scaled down first and
/// stored as JPEG, or PNG when it has transparency; a smaller one is copied as
/// it is. Earlier copies are removed once the new one is in place.
pub fn import(source: &Path, state: &Path) -> Result<PathBuf, String> {
    let bytes = std::fs::read(source).map_err(|error| error.to_string())?;
    let format = image::guess_format(&bytes).map_err(|_| "not an image".to_owned())?;
    let decoded =
        image::load_from_memory_with_format(&bytes, format).map_err(|error| error.to_string())?;
    let (contents, extension) = if decoded.width().max(decoded.height()) > MAX_SIDE {
        encode(&bounded(decoded))?
    } else {
        let extension = match format {
            image::ImageFormat::Jpeg => "jpg",
            image::ImageFormat::Png => "png",
            image::ImageFormat::WebP => "webp",
            image::ImageFormat::Gif => "gif",
            _ => return Err("unsupported image format".to_owned()),
        };
        (bytes, extension)
    };
    let target = state.join(format!("wallpaper.{extension}"));
    fastframe_file::write(&target, contents).map_err(|error| error.to_string())?;
    for other in EXTENSIONS.into_iter().filter(|other| *other != extension) {
        let _ = std::fs::remove_file(state.join(format!("wallpaper.{other}")));
    }
    Ok(target)
}

fn encode(image: &image::DynamicImage) -> Result<(Vec<u8>, &'static str), String> {
    let mut contents = Vec::new();
    let extension = if image.color().has_alpha() {
        image
            .write_to(
                &mut std::io::Cursor::new(&mut contents),
                image::ImageFormat::Png,
            )
            .map_err(|error| error.to_string())?;
        "png"
    } else {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut contents, 90)
            .encode_image(&image.to_rgb8())
            .map_err(|error| error.to_string())?;
        "jpg"
    };
    Ok((contents, extension))
}

/// Deletes ZapFast's copy of the wallpaper image, whatever its extension.
pub fn remove(state: &Path) {
    for extension in EXTENSIONS {
        match std::fs::remove_file(state.join(format!("wallpaper.{extension}"))) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => log::debug!("could not delete the wallpaper image: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppDirs;
    use egui::vec2;

    #[test]
    fn every_doodle_tile_keeps_its_intrinsic_dimensions() {
        for style in DoodleStyle::ALL {
            let image = rasterize(style).expect("doodle SVG renders");
            assert_eq!(image.size, [374, 666], "{style:?}");
            assert!(image.pixels.iter().any(|pixel| pixel.a() != 0), "{style:?}");
        }
    }

    #[test]
    fn cover_fit_fills_the_area_and_crops_the_centre() {
        let close = |a: Rect, b: Rect| {
            assert!(
                (a.min - b.min).length() < 1e-5 && (a.max - b.max).length() < 1e-5,
                "{a:?} != {b:?}"
            );
        };
        let full = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        close(cover_uv(vec2(1600.0, 900.0), vec2(800.0, 450.0)), full);
        // A wide image in a square keeps its middle square.
        close(
            cover_uv(vec2(2000.0, 1000.0), vec2(500.0, 500.0)),
            Rect::from_min_max(pos2(0.25, 0.0), pos2(0.75, 1.0)),
        );
        // A tall image in a wide area keeps its middle band.
        close(
            cover_uv(vec2(1000.0, 2000.0), vec2(800.0, 400.0)),
            Rect::from_min_max(pos2(0.0, 0.375), pos2(1.0, 0.625)),
        );
        // The shown part always has the area's aspect ratio.
        let uv = cover_uv(vec2(1234.0, 567.0), vec2(300.0, 700.0));
        let shown = vec2(uv.width() * 1234.0, uv.height() * 567.0);
        assert!((shown.x / shown.y - 300.0 / 700.0).abs() < 1e-4);
        close(cover_uv(vec2(0.0, 10.0), vec2(10.0, 10.0)), full);
        close(cover_uv(vec2(10.0, 10.0), vec2(0.0, 0.0)), full);
    }

    #[test]
    fn doodles_show_on_every_background() {
        for level in 0..=255u8 {
            for background in [
                Color32::from_rgb(level, level, level),
                Color32::from_rgb(level, 0, 0),
                Color32::from_rgb(0, level, level / 2),
            ] {
                let ink = doodle_ink(background).to_srgba_unmultiplied();
                let alpha = f32::from(ink[3]) / 255.0;
                let over = |channel: u8, ink: u8| {
                    f32::from(channel) * (1.0 - alpha) + f32::from(ink) * alpha
                };
                let luma = |r: f32, g: f32, b: f32| 0.2126 * r + 0.7152 * g + 0.0722 * b;
                let before = luma(
                    f32::from(background.r()),
                    f32::from(background.g()),
                    f32::from(background.b()),
                );
                let after = luma(
                    over(background.r(), ink[0]),
                    over(background.g(), ink[1]),
                    over(background.b(), ink[2]),
                );
                assert!(
                    (after - before).abs() >= 20.0,
                    "doodles fade into {background:?}: {before} to {after}"
                );
            }
        }
        // WhatsApp's own light and dark defaults keep their familiar look.
        assert_eq!(ink_alpha(doodle_ink(Color32::from_rgb(245, 241, 235))), 28);
        assert_eq!(ink_alpha(doodle_ink(Color32::from_rgb(22, 23, 23))), 42);
    }

    fn ink_alpha(ink: Color32) -> u8 {
        ink.to_srgba_unmultiplied()[3]
    }

    fn look(intensity: f32, accent: Option<Color32>) -> Look {
        Look {
            color: Color32::from_rgb(22, 23, 23),
            doodles: true,
            scene: false,
            style: DoodleStyle::Classic,
            intensity,
            accent,
            image: None,
        }
    }

    #[test]
    fn intensity_scales_the_doodles_and_stays_in_range() {
        let default = ink_alpha(ink(&look(1.0, None)));
        assert_eq!(
            default,
            ink_alpha(doodle_ink(Color32::from_rgb(22, 23, 23)))
        );
        assert!(ink_alpha(ink(&look(2.0, None))) > default);
        assert!(ink_alpha(ink(&look(0.5, None))) < default);
        // Out of range or not a number falls back to the slider's ends or 1.
        let range = &crate::settings::DOODLE_INTENSITY;
        assert_eq!(
            ink_alpha(ink(&look(100.0, None))),
            ink_alpha(ink(&look(*range.end(), None)))
        );
        assert_eq!(
            ink_alpha(ink(&look(-3.0, None))),
            ink_alpha(ink(&look(*range.start(), None)))
        );
        assert_eq!(ink_alpha(ink(&look(f32::NAN, None))), default);
    }

    #[test]
    fn accent_doodles_take_the_accent_at_the_same_opacity() {
        let accent = Color32::from_rgb(0x20, 0x90, 0xf0);
        let grey = ink(&look(1.5, None));
        let tinted = ink(&look(1.5, Some(accent)));
        assert_eq!(ink_alpha(tinted), ink_alpha(grey));
        // The ink is stored premultiplied at a low alpha, so each channel
        // comes back within a few steps of the accent.
        let rgb = tinted.to_srgba_unmultiplied();
        for (channel, want) in rgb[..3].iter().zip([0x20u8, 0x90, 0xf0]) {
            assert!(channel.abs_diff(want) <= 3, "{rgb:?}");
        }
        assert_ne!(
            tinted.to_srgba_unmultiplied()[..3],
            grey.to_srgba_unmultiplied()[..3]
        );
    }

    fn write_image(path: &Path, width: u32, height: u32) {
        let image = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        image.save(path).unwrap();
    }

    #[test]
    fn a_chosen_image_is_copied_scaled_and_removed() {
        let root = tempfile::tempdir().unwrap();
        let dirs = AppDirs::under(&root.path().join("app"));
        let chosen = root.path().join("holiday.png");
        write_image(&chosen, 64, 32);

        let small = import(&chosen, &dirs.state).unwrap();
        assert_eq!(small, dirs.wallpaper_file("png"));
        assert_eq!(
            std::fs::read(&small).unwrap(),
            std::fs::read(&chosen).unwrap()
        );
        assert_eq!(decode(&small).unwrap().size, [64, 32]);
        assert!(chosen.exists(), "the original stays where it was");

        // A large photo is scaled to the bound, keeps its shape, and
        // replaces the earlier copy.
        let large = root.path().join("panorama.jpg");
        write_image(&large, 4000, 1000);
        let copy = import(&large, &dirs.state).unwrap();
        assert_eq!(copy, dirs.wallpaper_file("jpg"));
        assert!(!small.exists(), "the earlier copy is gone");
        let (width, height) = image::image_dimensions(&copy).unwrap();
        assert_eq!((width, height), (MAX_SIDE, 640));
        assert_eq!(decode(&copy).unwrap().size, [2560, 640]);

        // Something that is not an image is refused and the copy survives.
        let text = root.path().join("notes.png");
        std::fs::write(&text, "not a picture").unwrap();
        assert!(import(&text, &dirs.state).is_err());
        assert!(copy.exists());

        remove(&dirs.state);
        assert!(!copy.exists());
        assert!(
            std::fs::read_dir(&dirs.state).unwrap().next().is_none(),
            "nothing is left behind"
        );
        remove(&dirs.state);
    }

    #[test]
    fn a_missing_or_damaged_image_falls_back_to_the_colour() {
        let root = tempfile::tempdir().unwrap();
        let damaged = root.path().join("wallpaper.png");
        std::fs::write(&damaged, "damaged").unwrap();
        let waker = crate::backend::Waker::default();
        for path in [damaged, root.path().join("missing.jpg")] {
            let mut custom = CustomImage::default();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            custom.sync(Some(&path), &waker);
            while custom.pending.is_some() && std::time::Instant::now() < deadline {
                std::thread::yield_now();
                custom.sync(Some(&path), &waker);
            }
            assert!(custom.pending.is_none());
            assert!(custom.ready().is_none());
        }
    }

    #[test]
    fn the_image_follows_the_setting() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("wallpaper.png");
        write_image(&path, 20, 10);
        let waker = crate::backend::Waker::default();
        let mut custom = CustomImage::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        custom.sync(Some(&path), &waker);
        while custom.ready().is_none() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
            custom.sync(Some(&path), &waker);
        }
        let (generation, image) = custom.ready().expect("decoded");
        assert_eq!(image.size, [20, 10]);
        custom.reload();
        custom.sync(Some(&path), &waker);
        assert!(custom.ready().is_none_or(|(next, _)| next > generation));
        custom.sync(None, &waker);
        assert!(custom.ready().is_none());
    }
}
