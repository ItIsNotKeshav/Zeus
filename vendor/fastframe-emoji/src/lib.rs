//! Colour emoji in the platform's own style, inline in egui text.
//!
//! Each emoji cluster (a flag, a skin tone, a family joined with U+200D, a
//! keycap) is laid out as one transparent placeholder glyph and painted over
//! with a picture of it. The picture comes from the platform's emoji font,
//! else from a font the app bundles:
//!
//! - Linux: fontconfig's colour emoji face (Noto Color Emoji and the like).
//! - macOS: Apple Color Emoji, memory-mapped; its `morx` sequences are
//!   joined by harfrust.
//! - Windows: Segoe UI Emoji, drawn by DirectWrite.
//!
//! ```no_run
//! # const NOTO: &[u8] = &[];
//! fastframe_emoji::EmojiSetup::default()
//!     .bundled(NOTO) // the app's own fallback, if it has one
//!     .install();
//! // Find and map the fonts before the first frame needs them.
//! std::thread::spawn(fastframe_emoji::warm_up);
//! ```
//!
//! Then either add [`EmojiPlugin`] to the egui context, which colours the
//! emoji in every text egui draws, or lay text out with [`append`] and paint
//! with [`paint`]. Pictures are drawn on a worker thread, never inside a
//! frame (see [`paint_cluster`]).
//! [`Emoji::render`] draws a picture outside egui.

mod bitmap;
mod paint;
mod plugin;
mod raster;
mod segment;
mod system;
#[cfg(windows)]
mod windows;

use std::sync::OnceLock;

pub use paint::{
    PLACEHOLDER, append, editor_job, paint, paint_cluster, placeholder_rects, prewarm,
};
pub use plugin::EmojiPlugin;
pub use raster::Picture;
pub use segment::{Piece, is_emoji, only_emoji, pieces};

/// Where emoji pictures come from. See the [crate] documentation.
#[derive(Clone, Debug)]
pub struct EmojiSetup {
    system: bool,
    bundled: Option<&'static [u8]>,
    synchronous: bool,
}

impl Default for EmojiSetup {
    /// The platform's emoji font, no bundled fallback, pictures drawn off
    /// the interface thread.
    fn default() -> Self {
        Self {
            system: true,
            bundled: None,
            synchronous: false,
        }
    }
}

impl EmojiSetup {
    /// Whether to draw with the platform's emoji font. On by default; off
    /// draws only the bundled font, as screenshot tests want.
    #[must_use]
    pub fn system(mut self, enabled: bool) -> Self {
        self.system = enabled;
        self
    }

    /// A colour bitmap emoji font (CBDT or sbix, such as Noto Color Emoji)
    /// for what the platform's font lacks: a sequence newer than the
    /// system, the country flags Segoe UI Emoji does not have, or a Linux
    /// desktop without an emoji font.
    #[must_use]
    pub fn bundled(mut self, font: &'static [u8]) -> Self {
        self.bundled = Some(font);
        self
    }

    /// Draws each new picture inside the frame that first shows it, instead
    /// of on the worker thread. For tests and demo screenshots, which must
    /// show every emoji in their first frame; an interactive app keeps it
    /// off so a screen of new emoji never stalls a frame.
    #[must_use]
    pub fn synchronous(mut self, enabled: bool) -> Self {
        self.synchronous = enabled;
        self
    }

    /// Makes this the process's emoji setup. Cheap: the fonts are found on
    /// the first [`warm_up`] or [`get`]. Returns false, changing nothing, if
    /// a setup was already installed.
    pub fn install(self) -> bool {
        SETUP.set(self).is_ok()
    }

    /// Finds and opens the fonts this setup names, now.
    #[must_use]
    pub fn load(&self) -> Emoji {
        let started = std::time::Instant::now();
        let mut sources = Vec::new();
        if self.system {
            #[cfg(windows)]
            if let Some(renderer) = windows::DirectWrite::probe() {
                sources.push(Source::DirectWrite(renderer));
            }
            if let Some(font) = system::bitmap_font() {
                sources.push(Source::Bitmap(Box::new(font)));
            }
        }
        if let Some(bytes) = self.bundled {
            match bitmap::BitmapFont::new(bitmap::Bytes::Static(bytes), 0, "bundled".into()) {
                Ok(font) => sources.push(Source::Bitmap(Box::new(font))),
                Err(reason) => log::warn!("the bundled emoji font is unusable: {reason}"),
            }
        }
        // At info, so a user's ordinary log says where emoji come from.
        log::info!(
            "colour emoji from {} ({:.1} ms)",
            if sources.is_empty() {
                "nowhere".to_owned()
            } else {
                sources
                    .iter()
                    .map(Source::name)
                    .collect::<Vec<_>>()
                    .join(", then ")
            },
            started.elapsed().as_secs_f32() * 1e3
        );
        let mut emoji = Emoji {
            sources,
            synchronous: self.synchronous,
            aspect: 1.0,
        };
        // The cell's shape, from a picture drawn here rather than on the
        // interface thread: one emoji costs a millisecond or two.
        if let Some(face) = emoji.render("\u{1F600}", 72) {
            emoji.aspect = face.size[0] as f32 / face.size[1] as f32;
        }
        emoji
    }
}

static SETUP: OnceLock<EmojiSetup> = OnceLock::new();
static EMOJI: OnceLock<Emoji> = OnceLock::new();

/// The installed setup's fonts, found on first use. Without an
/// [`EmojiSetup::install`] this is the default setup: the platform's font
/// and no bundled one.
pub fn get() -> &'static Emoji {
    EMOJI.get_or_init(|| SETUP.get_or_init(EmojiSetup::default).load())
}

/// Finds and maps the emoji fonts. Call it on a thread of its own at
/// startup, so the first frame with an emoji does not wait for it.
pub fn warm_up() {
    let _ = get();
}

/// Whether any colour emoji font was found. Without one, text is laid out
/// as it is and emoji are left to the text fonts.
pub fn available() -> bool {
    !get().sources.is_empty()
}

/// The fonts emoji pictures come from, in the order they are asked.
pub struct Emoji {
    sources: Vec<Source>,
    synchronous: bool,
    /// The first font's cell, width over height: Noto's is a little wider
    /// than tall, Apple's two thirds of an em narrower than it is tall.
    aspect: f32,
}

impl std::fmt::Debug for Emoji {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Emoji")
            .field(
                "sources",
                &self.sources.iter().map(Source::name).collect::<Vec<_>>(),
            )
            .field("synchronous", &self.synchronous)
            .field("aspect", &self.aspect)
            .finish()
    }
}

enum Source {
    /// Boxed: a font's shaper data is large beside the DirectWrite handle.
    Bitmap(Box<bitmap::BitmapFont>),
    #[cfg(windows)]
    DirectWrite(windows::DirectWrite),
}

impl Source {
    fn name(&self) -> String {
        match self {
            Self::Bitmap(font) => font.name.clone(),
            #[cfg(windows)]
            Self::DirectWrite(_) => "Segoe UI Emoji".to_owned(),
        }
    }

    fn render(&self, cluster: &str, height: u32) -> Option<Picture> {
        match self {
            Self::Bitmap(font) => font.render(cluster, height),
            #[cfg(windows)]
            Self::DirectWrite(renderer) => renderer.render(cluster, height),
        }
    }
}

impl Emoji {
    /// Draws one emoji cluster as a picture `height` pixels tall, framed in
    /// its font's cell (one advance wide, ascent plus descent tall), or
    /// `None` when no font draws it.
    ///
    /// The cluster is tried whole, then without U+FE0F, then with trailing
    /// parts dropped (a joined sequence newer than the fonts shows its first
    /// emoji), and every font gets each form before a part is dropped, so a
    /// bundled font's exact picture beats the system's partial one.
    ///
    /// This does the drawing on the calling thread: a few milliseconds for a
    /// new cluster. Call it off the interface thread.
    #[must_use]
    pub fn render(&self, cluster: &str, height: u32) -> Option<Picture> {
        if height == 0 {
            return None;
        }
        segment::candidates(cluster).iter().find_map(|form| {
            self.sources
                .iter()
                .find_map(|source| source.render(form, height))
        })
    }

    /// Whether no font was found.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// The width of the first font's pictures over their height: what an
    /// emoji's placeholder is scaled to, so it takes the room the picture
    /// does. 1 when no font draws the grinning face.
    #[must_use]
    pub fn aspect(&self) -> f32 {
        self.aspect
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundled_only() -> Emoji {
        EmojiSetup::default()
            .system(false)
            .bundled(bitmap::tests::NOTO_SUBSET)
            .load()
    }

    #[test]
    fn the_bundled_font_draws_what_it_has() {
        let emoji = bundled_only();
        assert!(!emoji.is_empty());
        for sequence in ["😀", "👍🏽", "👨‍👩‍👧", "🇩🇪", "#️⃣", "❤️"]
        {
            assert!(emoji.render(sequence, 48).is_some(), "{sequence}");
        }
        assert!(emoji.render("😀", 0).is_none());
    }

    #[test]
    fn an_unjoinable_sequence_shows_its_first_part() {
        let emoji = bundled_only();
        let joined = emoji.render("😀\u{200D}😀", 48).expect("a picture");
        assert_eq!(Some(joined), emoji.render("😀", 48));
        // A family member the subset lacks drops to the family's first part.
        let family = emoji.render("👨\u{200D}👩\u{200D}👧\u{200D}👦", 48);
        assert_eq!(family, emoji.render("👨\u{200D}👩\u{200D}👧", 48));
    }

    #[test]
    fn the_cell_shape_comes_from_the_first_font() {
        // Noto's cell is 1.245 em wide and 1.17 em tall.
        let aspect = bundled_only().aspect();
        assert!((1.05..1.08).contains(&aspect), "{aspect}");
        let nothing = EmojiSetup::default().system(false).load();
        assert_eq!(nothing.aspect(), 1.0);
    }

    #[test]
    fn nothing_is_found_when_nothing_is_asked_for() {
        let emoji = EmojiSetup::default().system(false).load();
        assert!(emoji.is_empty());
        assert!(emoji.render("😀", 48).is_none());
    }

    #[test]
    fn a_broken_bundled_font_is_skipped() {
        let emoji = EmojiSetup::default()
            .system(false)
            .bundled(b"not a font")
            .load();
        assert!(emoji.is_empty());
    }

    /// Whatever emoji font this machine has: run with `--ignored` to see
    /// that the system path draws flags, skin tones, families and keycaps.
    #[test]
    #[ignore = "reads this machine's fonts"]
    fn the_system_font_joins_sequences() {
        let emoji = EmojiSetup::default().load();
        assert!(!emoji.is_empty(), "no system emoji font here");
        let face = emoji.render("😀", 72).expect("grinning face");
        assert!(face.has_colour());
        for sequence in ["👍🏽", "👨‍👩‍👧", "🇩🇪", "#️⃣", "🏳️‍🌈", "👩‍❤️‍👨", "🫱🏼‍🫲🏿", "🏃‍➡️"]
        {
            // Segoe UI Emoji has no country flags: apps bundle them.
            if cfg!(windows) && sequence == "🇩🇪" {
                continue;
            }
            let picture = emoji
                .render(sequence, 72)
                .unwrap_or_else(|| panic!("{sequence}"));
            let first: String = sequence.chars().take(1).collect();
            assert_ne!(
                Some(&picture),
                emoji.render(&first, 72).as_ref(),
                "{sequence} fell back to its first part"
            );
        }
        println!("cell aspect {:.3}", emoji.aspect());
        // What one new picture costs the worker.
        let started = std::time::Instant::now();
        let faces: Vec<char> = ('\u{1F600}'..='\u{1F64F}').collect();
        for face in &faces {
            let _ = emoji.render(&face.to_string(), 72);
        }
        println!(
            "{:.2} ms per picture",
            started.elapsed().as_secs_f32() * 1e3 / faces.len() as f32
        );
    }

    /// Apple Color Emoji as macOS draws it: on macOS, with
    /// `cargo test -p fastframe-emoji -- --ignored`.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "reads this machine's fonts"]
    fn apple_emoji_sit_where_core_text_puts_them() {
        let emoji = EmojiSetup::default().load();
        // 210 pixels is Apple's cell (1.3125 em) at its 160 ppem strike,
        // so no scaling blurs the edges. CoreText draws the thumb's ink
        // from row 20 to 179 and the flag's from 49 to 151.
        let ink_rows = |cluster: &str| {
            let picture = emoji.render(cluster, 210).expect(cluster);
            let rows: Vec<usize> = picture
                .rgba
                .as_chunks::<4>()
                .0
                .chunks(picture.size[0])
                .enumerate()
                .filter(|(_, row)| row.iter().any(|pixel| pixel[3] > 64))
                .map(|(index, _)| index)
                .collect();
            (picture.size, rows[0], rows[rows.len() - 1] + 1)
        };
        assert_eq!(ink_rows("👍"), ([160, 210], 20, 179));
        assert_eq!(ink_rows("🇩🇪"), ([160, 210], 49, 151));
        assert!((emoji.aspect() - 160.0 / 210.0).abs() < 0.01);
        // The runner facing right is Apple's `flip` record: the runner
        // facing left, mirrored.
        let left = emoji.render("🏃", 210).expect("runner");
        let right = emoji
            .render("🏃\u{200D}➡\u{FE0F}", 210)
            .expect("runner facing right");
        assert_eq!(right, left.mirrored());
    }
}
