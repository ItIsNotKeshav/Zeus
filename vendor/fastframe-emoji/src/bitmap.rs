//! Colour bitmap emoji fonts: Noto Color Emoji and the like (CBDT), and
//! Apple Color Emoji (sbix).
//!
//! harfrust joins a cluster into glyphs the way the font asks (GSUB
//! ligatures for Noto, the AAT `morx` table for Apple), and skrifa reads
//! each glyph's bitmap. The glyphs are drawn into the font's own cell, one
//! advance wide and ascent plus descent tall, so every font's emoji sit on
//! the text the way that font means them to.

use skrifa::bitmap::{BitmapData, BitmapGlyph, BitmapStrikes, Origin};
use skrifa::instance::{LocationRef, Size};
use skrifa::raw::TableProvider as _;
use skrifa::{FontRef, GlyphId, MetadataProvider as _};

use crate::raster::{self, Picture};

/// A font file's bytes: compiled in, or mapped from disk.
pub(crate) enum Bytes {
    Static(&'static [u8]),
    #[cfg_attr(
        windows,
        allow(
            dead_code,
            reason = "Windows draws its own emoji font through DirectWrite"
        )
    )]
    Mapped(memmap2::Mmap),
}

impl Bytes {
    fn get(&self) -> &[u8] {
        match self {
            Self::Static(bytes) => bytes,
            Self::Mapped(map) => map,
        }
    }
}

/// How far below its data CoreText draws an `sbix` picture, in ems.
///
/// Every Apple Color Emoji glyph's bitmap starts at the baseline and rises
/// one em, and CoreText draws each an eighth of an em lower: measured on
/// macOS 27, every sequence at the 160 ppem strike sits 20 pixels below the
/// position its data give, with the same horizontal position and size. The
/// Apple font is the `sbix` font this path exists for, so its pictures are
/// placed as macOS places them.
const SBIX_DROP: f32 = 0.125;

/// A colour bitmap font, ready to shape and draw.
pub(crate) struct BitmapFont {
    bytes: Bytes,
    index: u32,
    shaper: harfrust::ShaperData,
    /// How far below their data the pictures are drawn, in ems.
    drop: f32,
    /// For the log.
    pub(crate) name: String,
}

impl BitmapFont {
    /// Opens a face that has colour bitmaps for U+1F600, or says why not.
    pub(crate) fn new(bytes: Bytes, index: u32, name: String) -> Result<Self, &'static str> {
        let font = FontRef::from_index(bytes.get(), index).map_err(|_| "not a font")?;
        let strikes = BitmapStrikes::new(&font);
        if strikes.is_empty() {
            return Err("no bitmap strikes");
        }
        let grinning = font.charmap().map('\u{1F600}').ok_or("no emoji")?;
        let colour = strikes
            .glyph_for_size(Size::new(64.0), grinning)
            .is_some_and(|glyph| !matches!(glyph.data, BitmapData::Mask(_)));
        if !colour {
            return Err("no colour bitmaps");
        }
        let shaper = harfrust::ShaperData::new(&font);
        let drop = if font.sbix().is_ok() { SBIX_DROP } else { 0.0 };
        Ok(Self {
            bytes,
            index,
            shaper,
            drop,
            name,
        })
    }

    fn font(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(self.bytes.get(), self.index).ok()
    }

    /// The glyphs `cluster` shapes to, with their pen positions and offsets
    /// in font units, when they form one picture: every glyph known, and
    /// exactly one that advances. Glyphs that do not advance are layers
    /// drawn with it (Apple draws a couple as a layer under a glyph).
    fn shape(&self, font: &FontRef<'_>, cluster: &str) -> Option<Vec<Placed>> {
        let shaper = self.shaper.shaper(font).build();
        let mut buffer = harfrust::UnicodeBuffer::new();
        buffer.push_str(cluster);
        buffer.guess_segment_properties();
        let shaped = shaper.shape(buffer, harfrust::ShapeOptions::new());
        let mut placed = Vec::new();
        let mut pen = 0i32;
        let mut advancing = 0;
        for (info, position) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
            if info.glyph_id == 0 {
                return None;
            }
            placed.push(Placed {
                glyph: GlyphId::new(info.glyph_id),
                x: pen + position.x_offset,
                y: position.y_offset,
                advance: position.x_advance,
            });
            if position.x_advance != 0 {
                advancing += 1;
            }
            pen += position.x_advance;
        }
        (advancing == 1).then_some(placed)
    }

    /// Draws `cluster` as one picture `height` pixels tall, or `None` when
    /// the font does not join it into one.
    pub(crate) fn render(&self, cluster: &str, height: u32) -> Option<Picture> {
        let font = self.font()?;
        let placed = self.shape(&font, cluster)?;
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        let units = f32::from(metrics.units_per_em);
        let (ascent, descent) = (metrics.ascent / units, metrics.descent / units);
        if units <= 0.0 || ascent - descent <= 0.0 {
            return None;
        }
        let advance: i32 = placed.iter().map(|glyph| glyph.advance).sum();
        // Pixels per em that make the cell `height` tall.
        let scale = height as f32 / (ascent - descent);
        let width = ((advance as f32 / units) * scale).round().max(1.0) as usize;
        let mut canvas = Picture::empty(width, height as usize);
        let strikes = BitmapStrikes::new(&font);
        let mut drawn = false;
        for glyph in &placed {
            let Some((bitmap, mirrored)) = bitmap(&font, &strikes, glyph.glyph, scale) else {
                if glyph.advance != 0 {
                    return None;
                }
                continue;
            };
            let mut picture = match bitmap.data {
                BitmapData::Png(data) => raster::decode_png(data)?,
                BitmapData::Bgra(data) => {
                    raster::from_bgra(data, bitmap.width as usize, bitmap.height as usize)?
                }
                BitmapData::Mask(_) => return None,
            };
            let ppem = bitmap.ppem_y.max(1.0);
            // Where the bitmap sits, in ems, x rightwards from the cell's
            // left edge and y upwards from the baseline.
            let pen = glyph.x as f32 / units;
            let lift = glyph.y as f32 / units - self.drop;
            let (mut left, top) = match bitmap.placement_origin {
                Origin::TopLeft => (
                    pen + bitmap.inner_bearing_x / ppem,
                    lift + bitmap.inner_bearing_y / ppem,
                ),
                Origin::BottomLeft => (
                    pen + bitmap.bearing_x / units + bitmap.inner_bearing_x / ppem,
                    lift + bitmap.bearing_y / units
                        + bitmap.inner_bearing_y / ppem
                        + picture.size[1] as f32 / ppem,
                ),
            };
            if mirrored {
                // Mirrored within the glyph's own advance.
                let right = left + picture.size[0] as f32 / ppem;
                left = 2.0 * pen + glyph.advance as f32 / units - right;
                picture = picture.mirrored();
            }
            let size = [
                (picture.size[0] as f32 / ppem * scale).round().max(1.0) as usize,
                (picture.size[1] as f32 / ppem * scale).round().max(1.0) as usize,
            ];
            let x = (left * scale).round() as i64;
            let y = ((ascent - top) * scale).round() as i64;
            canvas.draw(&picture, x, y, size[0], size[1]);
            drawn = true;
        }
        (drawn && canvas.coverage() > 0.0).then_some(canvas)
    }
}

/// A glyph's bitmap for `scale` pixels per em, and whether to mirror it.
///
/// skrifa reads `png ` data only. Apple Color Emoji draws 108 glyphs, the
/// people who face right (U+27A1 sequences) among them, as `flip` data: the
/// bitmap of the glyph it names, mirrored. `dupe` names a glyph to draw as
/// it is.
fn bitmap<'a>(
    font: &FontRef<'a>,
    strikes: &BitmapStrikes<'a>,
    glyph: GlyphId,
    scale: f32,
) -> Option<(BitmapGlyph<'a>, bool)> {
    if let Some(bitmap) = strikes.glyph_for_size(Size::new(scale), glyph) {
        return Some((bitmap, false));
    }
    let sbix = font.sbix().ok()?;
    let (source, mirrored) = sbix.strikes().iter().flatten().find_map(|strike| {
        let data = strike.glyph_data(glyph).ok()??;
        reference(data.graphic_type().to_be_bytes(), data.data())
    })?;
    let bitmap = strikes.glyph_for_size(Size::new(scale), source)?;
    Some((bitmap, mirrored))
}

/// The glyph an `sbix` `flip` or `dupe` record draws, and whether mirrored.
fn reference(graphic_type: [u8; 4], data: &[u8]) -> Option<(GlyphId, bool)> {
    let mirrored = match &graphic_type {
        b"flip" => true,
        b"dupe" => false,
        _ => return None,
    };
    let id = u16::from_be_bytes([*data.first()?, *data.get(1)?]);
    Some((GlyphId::new(id.into()), mirrored))
}

/// A shaped glyph, in font units.
struct Placed {
    glyph: GlyphId,
    x: i32,
    y: i32,
    advance: i32,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A subset of Noto Color Emoji 2.051 (SIL Open Font License 1.1):
    /// 😀, 👍🏽, 👨‍👩‍👧, 🇩🇪, #️⃣ and ❤️ with their parts.
    pub(crate) const NOTO_SUBSET: &[u8] =
        include_bytes!("../tests/fixtures/NotoColorEmoji-subset.ttf");

    pub(crate) fn noto() -> BitmapFont {
        BitmapFont::new(Bytes::Static(NOTO_SUBSET), 0, "test".into()).expect("the subset loads")
    }

    #[test]
    fn a_single_emoji_is_drawn_in_colour_at_the_height_asked() {
        let picture = noto().render("😀", 72).expect("grinning face");
        assert_eq!(picture.size[1], 72);
        // Noto's cell is 1.245 em wide and 1.17 em tall.
        assert!((75..=78).contains(&picture.size[0]), "{:?}", picture.size);
        assert!(picture.has_colour());
        assert!(picture.coverage() > 0.4, "{}", picture.coverage());
    }

    #[test]
    fn sequences_are_joined_into_one_picture() {
        let font = noto();
        let face = font.render("😀", 72).expect("face");
        for sequence in ["👍🏽", "👨‍👩‍👧", "🇩🇪", "#️⃣", "❤️"] {
            let picture = font
                .render(sequence, 72)
                .unwrap_or_else(|| panic!("{sequence} is one picture"));
            assert_eq!(picture.size, face.size, "{sequence} keeps the cell");
            assert!(picture.coverage() > 0.2, "{sequence}");
            // Noto draws families as grey silhouettes, so compare with the
            // first part rather than look for colour.
            // ❤️ is the heart itself, asked for as a picture.
            let first: String = sequence.chars().take(1).collect();
            if sequence != "❤️" {
                assert_ne!(Some(picture), font.render(&first, 72), "{sequence}");
            }
        }
        let thumb = font.render("👍", 72);
        let toned = font.render("👍🏽", 72);
        assert!(thumb.is_some());
        assert_ne!(thumb, toned, "the skin tone is drawn");
    }

    #[test]
    fn a_sequence_the_font_cannot_join_is_refused() {
        let font = noto();
        assert!(font.render("😀\u{200D}😀", 72).is_none());
        assert!(font.render("😀😀", 72).is_none(), "two pictures");
        assert!(font.render("a", 72).is_none(), "not in the font");
        // A flag the subset lacks is two letters, not one picture.
        assert!(font.render("🇮🇹", 72).is_none());
    }

    #[test]
    fn fonts_without_colour_bitmaps_are_refused() {
        let reason = BitmapFont::new(Bytes::Static(b"nope"), 0, String::new()).err();
        assert_eq!(reason, Some("not a font"));
    }

    #[test]
    fn flip_and_dupe_records_name_another_glyph() {
        assert_eq!(
            reference(*b"flip", &[3, 68]),
            Some((GlyphId::new(836), true))
        );
        assert_eq!(
            reference(*b"dupe", &[0, 7, 9]),
            Some((GlyphId::new(7), false))
        );
        assert_eq!(reference(*b"flip", &[3]), None, "too short");
        assert_eq!(reference(*b"png ", &[0, 7]), None);
        assert_eq!(
            noto().drop,
            0.0,
            "CBDT pictures sit where their data put them"
        );
    }

    #[test]
    fn pictures_scale_with_the_height() {
        let font = noto();
        let small = font.render("😀", 24).expect("small");
        let large = font
            .render("😀", 160)
            .expect("large, drawn up from the strike");
        assert_eq!(small.size[1], 24);
        assert_eq!(large.size[1], 160);
        assert!(small.coverage() > 0.4 && large.coverage() > 0.4);
    }
}
