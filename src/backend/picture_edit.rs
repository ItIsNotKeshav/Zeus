//! Crops, turns and draws on a picture on its way out, so an attachment can
//! be trimmed and marked up before it is sent.

use std::path::{Path, PathBuf};

use crate::model::{
    PictureCrop, PictureMark, PictureSource, PictureSticker, PictureStroke, StickerArt,
};

/// Quality of an opaque picture written back. The send path passes a JPEG
/// through untouched, so this is the only lossy step for one.
const QUALITY: u8 = 92;

/// A cropped picture, encoded, with the extension its format goes by.
#[derive(Debug)]
pub struct Edited {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
}

/// A picture's size, read without keeping its pixels.
pub fn inspect(bytes: &[u8]) -> Result<(u32, u32), String> {
    let picture = image::load_from_memory(bytes)
        .map_err(|error| format!("This picture could not be read: {error}"))?;
    Ok((picture.width(), picture.height()))
}

/// The picture turned by whole quarter turns clockwise, drawn on, then
/// cropped. The marks and the crop are measured in the turned picture, which
/// is what the editor showed.
pub fn edit(
    bytes: &[u8],
    crop: PictureCrop,
    turns: u8,
    marks: &[PictureMark],
) -> Result<Edited, String> {
    let picture = image::load_from_memory(bytes)
        .map_err(|error| format!("This picture could not be read: {error}"))?
        .to_rgba8();
    turned_and_cropped(picture, crop, turns, marks)
}

/// The same for a pasted picture, which has no file to read back.
pub fn edit_pasted(
    rgba: &[u8],
    width: u32,
    height: u32,
    crop: PictureCrop,
    turns: u8,
    marks: &[PictureMark],
) -> Result<Edited, String> {
    let picture = image::RgbaImage::from_raw(width.max(1), height.max(1), rgba.to_vec())
        .ok_or_else(|| "This picture could not be read".to_owned())?;
    turned_and_cropped(picture, crop, turns, marks)
}

/// Turns and crops a staged picture, writes the result in `dir` beside the
/// other media, and answers with the file it wrote. The name comes from the
/// picture's own bytes, so the same crop of the same picture is written once.
pub fn write(
    source: &PictureSource,
    width: u32,
    height: u32,
    crop: PictureCrop,
    turns: u8,
    marks: &[PictureMark],
    dir: &Path,
) -> Result<PathBuf, String> {
    let picture = match source {
        PictureSource::File(path) => std::fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| edit(&bytes, crop, turns, marks)),
        PictureSource::Pasted(rgba) => edit_pasted(rgba, width, height, crop, turns, marks),
    }?;
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let name = super::sticker_store::content_hash(&picture.bytes);
    let path = dir.join(format!("{name}.{}", picture.extension));
    std::fs::write(&path, picture.bytes).map_err(|error| error.to_string())?;
    Ok(path)
}

/// Turns the picture, draws the marks on it, keeps the region, and writes it
/// out.
fn turned_and_cropped(
    picture: image::RgbaImage,
    crop: PictureCrop,
    turns: u8,
    marks: &[PictureMark],
) -> Result<Edited, String> {
    let mut picture = match turns % 4 {
        1 => image::imageops::rotate90(&picture),
        2 => image::imageops::rotate180(&picture),
        3 => image::imageops::rotate270(&picture),
        _ => picture,
    };
    for mark in marks {
        match mark {
            PictureMark::Stroke(stroke) => draw_stroke(&mut picture, stroke),
            PictureMark::Sticker(sticker) => draw_sticker(&mut picture, sticker),
        }
    }
    let (width, height) = picture.dimensions();
    let crop = crop.clamped(width, height);
    let kept =
        image::imageops::crop_imm(&picture, crop.x, crop.y, crop.width, crop.height).to_image();
    encode(&kept)
}

/// How opaque a highlighter is: the picture shows through it.
pub const HIGHLIGHTER_ALPHA: f32 = 0.4;

/// Draws a line along the stroke's points with round ends and joins. Each
/// pixel takes the most any part of the line covers it, never the sum, so a
/// see-through highlighter that crosses itself stays as light as it started.
fn draw_stroke(picture: &mut image::RgbaImage, stroke: &PictureStroke) {
    let Some(first) = stroke.points.first() else {
        return;
    };
    let radius = (stroke.width / 2.0).max(0.5);
    let (width, height) = picture.dimensions();
    let (mut left, mut top, mut right, mut bottom) = (first[0], first[1], first[0], first[1]);
    for [x, y] in &stroke.points {
        left = left.min(*x);
        top = top.min(*y);
        right = right.max(*x);
        bottom = bottom.max(*y);
    }
    let reach = radius + 1.0;
    let x0 = (left - reach).floor().max(0.0) as u32;
    let y0 = (top - reach).floor().max(0.0) as u32;
    let x1 = ((right + reach).ceil().max(0.0) as u32).min(width);
    let y1 = ((bottom + reach).ceil().max(0.0) as u32).min(height);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let span = (x1 - x0) as usize;
    let mut cover = vec![0.0_f32; span * (y1 - y0) as usize];
    // A lone point is a dot: a segment from it to itself.
    let segments: Vec<([f32; 2], [f32; 2])> = if stroke.points.len() == 1 {
        vec![(*first, *first)]
    } else {
        stroke
            .points
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .collect()
    };
    for (a, b) in segments {
        let sx0 = (a[0].min(b[0]) - reach).floor().max(x0 as f32) as u32;
        let sy0 = (a[1].min(b[1]) - reach).floor().max(y0 as f32) as u32;
        let sx1 = ((a[0].max(b[0]) + reach).ceil().max(0.0) as u32).min(x1);
        let sy1 = ((a[1].max(b[1]) + reach).ceil().max(0.0) as u32).min(y1);
        for y in sy0..sy1 {
            for x in sx0..sx1 {
                let distance = segment_distance([x as f32 + 0.5, y as f32 + 0.5], a, b);
                // A pixel-wide soft edge, so the line is not jagged.
                let covered = (radius + 0.5 - distance).clamp(0.0, 1.0);
                let at = (y - y0) as usize * span + (x - x0) as usize;
                cover[at] = cover[at].max(covered);
            }
        }
    }
    let opacity = if stroke.highlighter {
        HIGHLIGHTER_ALPHA
    } else {
        1.0
    };
    for y in y0..y1 {
        for x in x0..x1 {
            let alpha = cover[(y - y0) as usize * span + (x - x0) as usize] * opacity;
            if alpha > 0.0 {
                blend(picture.get_pixel_mut(x, y), stroke.colour, alpha);
            }
        }
    }
}

/// How far `point` is from the segment `a` to `b`.
fn segment_distance(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 {
        (((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (nx, ny) = (a[0] + t * dx - point[0], a[1] + t * dy - point[1]);
    (nx * nx + ny * ny).sqrt()
}

/// Lays straight `colour` over the pixel at `alpha`.
fn blend(pixel: &mut image::Rgba<u8>, colour: [u8; 3], alpha: f32) {
    let under = f32::from(pixel[3]) / 255.0;
    let out = alpha + under * (1.0 - alpha);
    if out <= 0.0 {
        return;
    }
    for channel in 0..3 {
        let over = f32::from(colour[channel]) * alpha;
        let below = f32::from(pixel[channel]) * under * (1.0 - alpha);
        pixel[channel] = ((over + below) / out).round().clamp(0.0, 255.0) as u8;
    }
    pixel[3] = (out * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// Lays the sticker's picture, upright and fitted to its square, over the
/// picture. One that cannot be drawn (an emoji no font has, a sticker file
/// gone) is left out rather than failing the whole edit.
fn draw_sticker(picture: &mut image::RgbaImage, sticker: &PictureSticker) {
    let side = sticker.size.round().max(1.0) as u32;
    let art = match &sticker.art {
        StickerArt::Emoji(cluster) => emoji_picture(cluster, side),
        StickerArt::File(path) => image::open(path).ok().map(|art| art.to_rgba8()),
    };
    let Some(art) = art else {
        return;
    };
    let (wide, tall) = art.dimensions();
    let scale = side as f32 / wide.max(tall).max(1) as f32;
    let (fit_w, fit_h) = (
        ((wide as f32 * scale).round() as u32).max(1),
        ((tall as f32 * scale).round() as u32).max(1),
    );
    let art = if (fit_w, fit_h) == (wide, tall) {
        art
    } else {
        image::imageops::resize(&art, fit_w, fit_h, image::imageops::FilterType::Triangle)
    };
    let left = (sticker.centre[0] - fit_w as f32 / 2.0).round() as i64;
    let top = (sticker.centre[1] - fit_h as f32 / 2.0).round() as i64;
    image::imageops::overlay(picture, &art, left, top);
}

/// An emoji drawn by the emoji font, as straight RGBA `side` pixels tall.
fn emoji_picture(cluster: &str, side: u32) -> Option<image::RgbaImage> {
    let drawn = fastframe_emoji::get().render(cluster, side)?;
    let [width, height] = drawn.size;
    // The font's pixels are premultiplied; the picture's are not.
    let mut rgba = drawn.rgba;
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha > 0 && alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    image::RgbaImage::from_raw(width as u32, height as u32, rgba)
}

/// Writes the picture as a PNG when any of it is see-through, so the crop
/// keeps its transparency, and as a JPEG otherwise.
fn encode(picture: &image::RgbaImage) -> Result<Edited, String> {
    use image::ImageEncoder;
    let (width, height) = picture.dimensions();
    let mut bytes = Vec::new();
    if picture.pixels().any(|pixel| pixel[3] < u8::MAX) {
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                picture.as_raw(),
                width,
                height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|error| format!("Could not write the picture: {error}"))?;
        return Ok(Edited {
            bytes,
            extension: "png",
        });
    }
    let opaque = image::DynamicImage::ImageRgba8(picture.clone()).to_rgb8();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, QUALITY)
        .write_image(
            opaque.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|error| format!("Could not write the picture: {error}"))?;
    Ok(Edited {
        bytes,
        extension: "jpg",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A picture whose top-left pixel is red, so a turn can be told apart
    /// from the other direction.
    fn png(width: u32, height: u32) -> Vec<u8> {
        use image::ImageEncoder;
        let picture = image::RgbaImage::from_fn(width, height, |x, y| {
            if x == 0 && y == 0 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            }
        });
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(&picture, width, height, image::ExtendedColorType::Rgba8)
            .expect("encodes");
        bytes
    }

    fn decoded(edited: &Edited) -> image::RgbaImage {
        image::load_from_memory(&edited.bytes)
            .expect("decodes")
            .to_rgba8()
    }

    #[test]
    fn a_picture_reports_its_size() {
        assert_eq!(inspect(&png(800, 600)).expect("reads"), (800, 600));
        assert!(inspect(b"not a picture").is_err());
    }

    #[test]
    fn a_crop_keeps_only_the_region_it_names() {
        let bytes = edit(
            &png(800, 600),
            PictureCrop {
                x: 100,
                y: 50,
                width: 200,
                height: 120,
            },
            0,
            &[],
        )
        .expect("edits");
        assert_eq!(decoded(&bytes).dimensions(), (200, 120));
    }

    #[test]
    fn a_quarter_turn_clockwise_moves_the_top_left_to_the_top_right() {
        let bytes = edit(&png(4, 2), PictureCrop::full(2, 4), 1, &[]).expect("edits");
        let picture = decoded(&bytes);
        assert_eq!(picture.dimensions(), (2, 4));
        // The red corner started top-left of a wide picture, so turning
        // clockwise carries it to the top-right of a tall one. The write is
        // lossy, so the channels are compared rather than their exact values.
        let top_right = picture.get_pixel(1, 0);
        assert!(
            top_right[0] > 200 && top_right[2] < 60,
            "red at the top right, got {top_right:?}"
        );
        let top_left = picture.get_pixel(0, 0);
        assert!(
            top_left[2] > 200 && top_left[0] < 60,
            "blue at the top left, got {top_left:?}"
        );
    }

    #[test]
    fn turning_and_cropping_measure_the_crop_after_the_turn() {
        // The same region the editor showed, in the turned picture.
        let bytes = edit(
            &png(4, 2),
            PictureCrop {
                x: 1,
                y: 0,
                width: 1,
                height: 2,
            },
            1,
            &[],
        )
        .expect("edits");
        let picture = decoded(&bytes);
        assert_eq!(picture.dimensions(), (1, 2));
        let corner = picture.get_pixel(0, 0);
        assert!(
            corner[0] > 200 && corner[2] < 60,
            "the red corner, got {corner:?}"
        );
    }

    #[test]
    fn see_through_pixels_stay_see_through() {
        use image::ImageEncoder;
        let mut source = Vec::new();
        image::codecs::png::PngEncoder::new(&mut source)
            .write_image(
                image::RgbaImage::from_fn(8, 8, |x, _| {
                    image::Rgba([10, 20, 30, if x < 4 { 0 } else { 128 }])
                })
                .as_raw(),
                8,
                8,
                image::ExtendedColorType::Rgba8,
            )
            .expect("encodes");
        let edited = edit(&source, PictureCrop::full(8, 8), 0, &[]).expect("edits");
        assert_eq!(edited.extension, "png");
        let picture = decoded(&edited);
        assert_eq!(picture.get_pixel(2, 2)[3], 0);
        assert_eq!(*picture.get_pixel(6, 2), image::Rgba([10, 20, 30, 128]));
    }

    #[test]
    fn an_opaque_picture_is_written_as_a_jpeg() {
        let edited = edit(&png(8, 8), PictureCrop::full(8, 8), 0, &[]).expect("edits");
        assert_eq!(edited.extension, "jpg");
    }

    #[test]
    fn a_pasted_picture_is_turned_and_cropped_like_a_file() {
        // The same red corner as the encoded fixture, as straight RGBA.
        let (width, height) = (4u32, 2u32);
        let mut rgba = [0u8, 0, 255, 255].repeat((width * height) as usize);
        rgba[..4].copy_from_slice(&[255, 0, 0, 255]);
        let bytes =
            edit_pasted(&rgba, width, height, PictureCrop::full(2, 4), 1, &[]).expect("edits");
        let picture = decoded(&bytes);
        assert_eq!(picture.dimensions(), (2, 4));
        let top_right = picture.get_pixel(1, 0);
        assert!(
            top_right[0] > 200 && top_right[2] < 60,
            "red at the top right, got {top_right:?}"
        );
    }

    #[test]
    fn a_pasted_picture_that_is_not_rgba_is_refused() {
        assert!(edit_pasted(&[0, 0, 0], 4, 4, PictureCrop::full(4, 4), 0, &[]).is_err());
    }

    #[test]
    fn writing_a_crop_puts_it_under_its_own_bytes() {
        let root = tempfile::tempdir().expect("makes the directory");
        let dir = root.path().to_path_buf();
        let source = dir.join("source.png");
        std::fs::write(&source, png(800, 600)).expect("writes the source");
        let crop = PictureCrop {
            x: 100,
            y: 50,
            width: 200,
            height: 120,
        };
        let written = write(
            &PictureSource::File(source.clone()),
            800,
            600,
            crop,
            0,
            &[],
            &dir,
        )
        .expect("writes the crop");
        assert_eq!(
            written.extension().and_then(|kind| kind.to_str()),
            Some("jpg")
        );
        assert_eq!(read(&written).dimensions(), (200, 120));
        // The same crop of the same picture lands on the same name, so a
        // second look at it writes nothing new.
        let again = write(&PictureSource::File(source), 800, 600, crop, 0, &[], &dir)
            .expect("writes it again");
        assert_eq!(again, written);
        // A pasted picture goes the same way, from the pixels it was given.
        let pasted = std::sync::Arc::new([0u8, 0, 255, 255].repeat(800 * 600));
        let pasted = write(&PictureSource::Pasted(pasted), 800, 600, crop, 0, &[], &dir)
            .expect("writes the pasted crop");
        assert_eq!(read(&pasted).dimensions(), (200, 120));
    }

    fn read(path: &Path) -> image::RgbaImage {
        image::open(path).expect("decodes").to_rgba8()
    }

    /// A white picture as PNG bytes.
    fn white(width: u32, height: u32) -> Vec<u8> {
        use image::ImageEncoder;
        let picture = image::RgbaImage::from_pixel(width, height, image::Rgba([255; 4]));
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(&picture, width, height, image::ExtendedColorType::Rgba8)
            .expect("encodes");
        bytes
    }

    fn line(points: Vec<[f32; 2]>, highlighter: bool) -> PictureMark {
        PictureMark::Stroke(PictureStroke {
            points,
            colour: [0, 0, 0],
            width: 6.0,
            highlighter,
        })
    }

    #[test]
    fn a_line_lands_where_it_was_drawn_on_a_turned_and_cropped_picture() {
        // Drawn on the turned picture (20 across, 40 tall), across row 30,
        // then cropped to its bottom half.
        let edited = edit(
            &white(40, 20),
            PictureCrop {
                x: 0,
                y: 20,
                width: 20,
                height: 20,
            },
            1,
            &[line(vec![[0.0, 30.0], [20.0, 30.0]], false)],
        )
        .expect("edits");
        let picture = decoded(&edited);
        assert_eq!(picture.dimensions(), (20, 20));
        // Ten rows into the crop is where the line ran.
        assert!(picture.get_pixel(10, 10)[0] < 40, "dark on the line");
        assert!(picture.get_pixel(10, 2)[0] > 220, "white away from it");
    }

    #[test]
    fn a_highlighter_crossing_itself_does_not_darken() {
        let edited = edit(
            &white(40, 40),
            PictureCrop::full(40, 40),
            0,
            &[line(
                vec![[5.0, 20.0], [35.0, 20.0], [35.0, 21.0], [5.0, 20.5]],
                true,
            )],
        )
        .expect("edits");
        let picture = decoded(&edited);
        let once = 255.0 * (1.0 - HIGHLIGHTER_ALPHA);
        let shade = f32::from(picture.get_pixel(20, 20)[0]);
        assert!(
            (shade - once).abs() < 6.0,
            "one coat of highlighter, got {shade} for {once}"
        );
    }

    #[test]
    fn a_sticker_file_is_laid_over_the_picture() {
        use image::ImageEncoder;
        let root = tempfile::tempdir().expect("makes the directory");
        let path = root.path().join("sticker.png");
        let red = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]));
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(&red, 8, 8, image::ExtendedColorType::Rgba8)
            .expect("encodes");
        std::fs::write(&path, bytes).expect("writes the sticker");
        let edited = edit(
            &white(40, 40),
            PictureCrop::full(40, 40),
            0,
            &[PictureMark::Sticker(PictureSticker {
                art: StickerArt::File(path),
                centre: [20.0, 20.0],
                size: 16.0,
            })],
        )
        .expect("edits");
        let picture = decoded(&edited);
        let middle = picture.get_pixel(20, 20);
        assert!(middle[0] > 200 && middle[1] < 60, "red under the sticker");
        assert!(picture.get_pixel(2, 2)[1] > 220, "white beyond it");
    }

    #[test]
    fn a_sticker_that_cannot_be_drawn_leaves_the_picture_alone() {
        let edited = edit(
            &white(10, 10),
            PictureCrop::full(10, 10),
            0,
            &[PictureMark::Sticker(PictureSticker {
                art: StickerArt::File("/gone/sticker.webp".into()),
                centre: [5.0, 5.0],
                size: 6.0,
            })],
        );
        assert!(edited.is_ok());
    }

    #[test]
    fn writing_a_crop_of_a_missing_picture_says_so() {
        let root = tempfile::tempdir().expect("makes the directory");
        let dir = root.path().to_path_buf();
        let error = write(
            &PictureSource::File(dir.join("gone.png")),
            4,
            4,
            PictureCrop::full(4, 4),
            0,
            &[],
            &dir,
        )
        .expect_err("refuses");
        assert!(!error.is_empty());
    }
}
