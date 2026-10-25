//! Segoe UI Emoji drawn by DirectWrite and Direct2D.
//!
//! Segoe UI Emoji keeps its colours in COLR layers, which Direct2D draws when
//! asked to use colour fonts. On Windows 11 it builds family sequences from
//! several overlapping part glyphs that together take the width of one emoji.
//! From ZapFast pull request #229 by Andrés Rodríguez (pulgueta), framed here
//! in the font's own cell like the bitmap fonts.

#![allow(
    unsafe_code,
    reason = "DirectWrite, Direct2D and WIC are COM interfaces; every call is unsafe"
)]

use std::cell::Cell;
use std::ffi::c_void;
use std::mem::ManuallyDrop;

use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE, D2D1CreateFactory,
    ID2D1Factory,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_GLYPH_RUN,
    DWRITE_GLYPH_RUN_DESCRIPTION, DWRITE_LINE_METRICS, DWRITE_MATRIX, DWRITE_MEASURING_MODE,
    DWRITE_STRIKETHROUGH, DWRITE_UNDERLINE, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory,
    IDWriteFactory, IDWriteFontFace2, IDWriteInlineObject, IDWritePixelSnapping_Impl,
    IDWriteTextLayout, IDWriteTextRenderer, IDWriteTextRenderer_Impl,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
    WICBitmapCacheOnLoad, WICBitmapLockRead, WICRect,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::core::{BOOL, ComObject, IUnknown, Interface, Ref, Result, implement, w};
use windows_numerics::Vector2;

use crate::raster::Picture;

/// The size sequences are measured at, in pixels.
const PROBE: f32 = 100.0;

/// Draws with Segoe UI Emoji on whichever thread asks. Each thread keeps its
/// own factories.
pub(crate) struct DirectWrite;

impl DirectWrite {
    /// The renderer, when this system draws the grinning face in colour.
    pub(crate) fn probe() -> Option<Self> {
        let works =
            FACTORIES.with(|factories| factories.is_some()) && render("\u{1F600}", 32).is_some();
        works.then_some(Self)
    }

    /// Draws `cluster` as one picture `height` pixels tall, or `None` when
    /// DirectWrite does not draw it as one picture.
    #[allow(clippy::unused_self, reason = "the renderer's state is per thread")]
    pub(crate) fn render(&self, cluster: &str, height: u32) -> Option<Picture> {
        render(cluster, height)
    }
}

struct Factories {
    d2d: ID2D1Factory,
    write: IDWriteFactory,
    wic: IWICImagingFactory,
    /// How far one emoji, the grinning face, advances at [`PROBE`].
    emoji_advance: f32,
}

thread_local! {
    /// COM objects stay on the thread that made them. They are never
    /// released: the main thread's locals are dropped after DirectWrite,
    /// Direct2D and WIC have shut down, and releasing them then ends the
    /// process with STATUS_INVALID_PARAMETER (found in ZapFast #229).
    static FACTORIES: ManuallyDrop<Option<Factories>> = ManuallyDrop::new(factories().ok());
}

fn render(cluster: &str, height: u32) -> Option<Picture> {
    // Segoe UI Emoji has no subdivision flags and draws their tag
    // characters as nothing over a plain black flag.
    if cluster
        .chars()
        .any(|character| ('\u{E0020}'..='\u{E007F}').contains(&character))
    {
        return None;
    }
    FACTORIES.with(|factories| draw(factories.as_ref()?, cluster, height).ok()?)
}

fn factories() -> Result<Factories> {
    // SAFETY: the factory constructors take no pointers but the null options.
    unsafe {
        // WIC is a COM class. A thread that already has an apartment keeps
        // it; a new one is single-threaded, like the one winit's OLE drag and
        // drop sets up.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let text: Vec<u16> = "\u{1F600}".encode_utf16().collect();
        let (_, counter) = laid_out(&write, &text, PROBE)?;
        Ok(Factories {
            d2d: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
            write,
            wic: CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?,
            emoji_advance: counter.advance.get(),
        })
    }
}

/// Lays `text` out in Segoe UI Emoji at `size` pixels and counts what the
/// layout draws.
fn laid_out(
    write: &IDWriteFactory,
    text: &[u16],
    size: f32,
) -> Result<(IDWriteTextLayout, ComObject<Counter>)> {
    // SAFETY: every pointer passed points to a live local.
    unsafe {
        let format = write.CreateTextFormat(
            w!("Segoe UI Emoji"),
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            w!("en-us"),
        )?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        let layout = write.CreateTextLayout(text, &format, size * 8.0, size * 8.0)?;
        let counter = ComObject::new(Counter::default());
        let renderer: IDWriteTextRenderer = counter.to_interface();
        layout.Draw(None, &renderer, 0.0, 0.0)?;
        Ok((layout, counter))
    }
}

/// Whether the glyphs DirectWrite drew form one picture: exactly one glyph
/// that advances, with any zero-advance glyphs as layers under it, or, for a
/// sequence joined with U+200D, parts that together advance no further than
/// one emoji. Unjoined parts each take a whole emoji's width, and a flag
/// Segoe UI Emoji lacks shows as two letters, which has no U+200D.
fn joined(advancing: u32, advance: f32, zwj: bool, emoji_advance: f32) -> bool {
    advancing == 1 || (zwj && advancing > 1 && advance <= emoji_advance * 1.01)
}

fn draw(factories: &Factories, cluster: &str, height: u32) -> Result<Option<Picture>> {
    let text: Vec<u16> = cluster.encode_utf16().collect();
    let (_, probe) = laid_out(&factories.write, &text, PROBE)?;
    let zwj = cluster.contains('\u{200D}');
    let cell = probe.cell.get();
    if probe.monochrome.get()
        || cell <= 0.0
        || !joined(
            probe.glyphs.get(),
            probe.advance.get(),
            zwj,
            factories.emoji_advance,
        )
    {
        return Ok(None);
    }
    // The size that makes the font's cell, ascent plus descent, `height`.
    let size = height as f32 / cell;
    let width = (probe.advance.get() / PROBE * size).round().max(1.0) as u32;
    let ascent = probe.ascent.get() * size;
    let (layout, _) = laid_out(&factories.write, &text, size)?;
    // SAFETY: every pointer passed points to a live local, and the locked
    // bitmap memory is read only while `lock` is held, within its size.
    unsafe {
        let mut lines = [DWRITE_LINE_METRICS::default()];
        let mut count = 0;
        layout.GetLineMetrics(Some(&mut lines), &mut count)?;
        // The layout's line box may be taller than the cell (line gap);
        // put its baseline on the cell's.
        let top = ascent - lines[0].baseline;

        let bitmap = factories.wic.CreateBitmap(
            width,
            height,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapCacheOnLoad,
        )?;
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            ..Default::default()
        };
        let target = factories
            .d2d
            .CreateWicBitmapRenderTarget(&bitmap, &properties)?;
        let black = D2D1_COLOR_F {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        let brush = target.CreateSolidColorBrush(&black, None)?;
        target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        target.BeginDraw();
        // No colour clears to transparent black.
        target.Clear(None);
        target.DrawTextLayout(
            Vector2 { X: 0.0, Y: top },
            &layout,
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
        );
        target.EndDraw(None, None)?;

        let area = WICRect {
            X: 0,
            Y: 0,
            Width: width as i32,
            Height: height as i32,
        };
        let lock = bitmap.Lock(&area, WICBitmapLockRead.0 as u32)?;
        let stride = lock.GetStride()? as usize;
        let mut size = 0;
        let mut data = std::ptr::null_mut();
        lock.GetDataPointer(&mut size, &mut data)?;
        if data.is_null() {
            return Ok(None);
        }
        let source = std::slice::from_raw_parts(data, size as usize);
        let row = width as usize * 4;
        let mut rgba = Vec::with_capacity(row * height as usize);
        for line in source.chunks(stride).take(height as usize) {
            let Some(line) = line.get(..row) else {
                return Ok(None);
            };
            // Premultiplied BGRA to premultiplied RGBA.
            for pixel in line.as_chunks::<4>().0.iter() {
                rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
            }
        }
        drop(lock);
        if rgba.len() != row * height as usize {
            return Ok(None);
        }
        let picture = Picture {
            size: [width as usize, height as usize],
            rgba,
        };
        Ok((picture.coverage() > 0.0).then_some(picture))
    }
}

/// Counts the glyphs a layout draws that advance and how far they advance,
/// notes a font without colour glyphs (which DirectWrite substitutes for a
/// character Segoe UI Emoji lacks), and reads the font's cell in ems.
#[implement(IDWriteTextRenderer)]
#[derive(Default)]
struct Counter {
    glyphs: Cell<u32>,
    advance: Cell<f32>,
    monochrome: Cell<bool>,
    /// Ascent plus descent, in ems.
    cell: Cell<f32>,
    /// Ascent, in ems.
    ascent: Cell<f32>,
}

impl IDWriteTextRenderer_Impl for Counter_Impl {
    fn DrawGlyphRun(
        &self,
        _context: *const c_void,
        _x: f32,
        _y: f32,
        _mode: DWRITE_MEASURING_MODE,
        run: *const DWRITE_GLYPH_RUN,
        _description: *const DWRITE_GLYPH_RUN_DESCRIPTION,
        _effect: Ref<'_, IUnknown>,
    ) -> Result<()> {
        // SAFETY: DirectWrite passes a valid glyph run for this call.
        let Some(run) = (unsafe { run.as_ref() }) else {
            return Ok(());
        };
        let (advancing, advance) = if run.glyphAdvances.is_null() {
            (run.glyphCount, f32::INFINITY)
        } else {
            // SAFETY: DirectWrite passes one advance for each glyph of the run.
            let advances =
                unsafe { std::slice::from_raw_parts(run.glyphAdvances, run.glyphCount as usize) };
            (
                advances.iter().filter(|advance| **advance != 0.0).count() as u32,
                advances.iter().sum(),
            )
        };
        self.glyphs.set(self.glyphs.get() + advancing);
        self.advance.set(self.advance.get() + advance);
        let face = (*run.fontFace).as_ref();
        if let Some(face) = face {
            let mut metrics = DWRITE_FONT_METRICS::default();
            // SAFETY: the face is live for this call and fills the struct.
            unsafe { face.GetMetrics(&mut metrics) };
            let units = f32::from(metrics.designUnitsPerEm);
            if units > 0.0 {
                let ascent = f32::from(metrics.ascent) / units;
                let cell = ascent + f32::from(metrics.descent) / units;
                if cell > self.cell.get() {
                    self.cell.set(cell);
                    self.ascent.set(ascent);
                }
            }
        }
        let colour = face
            .and_then(|face| face.cast::<IDWriteFontFace2>().ok())
            // SAFETY: the face is a live font face for this call.
            .is_some_and(|face| unsafe { face.IsColorFont() }.as_bool());
        if !colour {
            self.monochrome.set(true);
        }
        Ok(())
    }

    fn DrawUnderline(
        &self,
        _context: *const c_void,
        _x: f32,
        _y: f32,
        _underline: *const DWRITE_UNDERLINE,
        _effect: Ref<'_, IUnknown>,
    ) -> Result<()> {
        Ok(())
    }

    fn DrawStrikethrough(
        &self,
        _context: *const c_void,
        _x: f32,
        _y: f32,
        _strikethrough: *const DWRITE_STRIKETHROUGH,
        _effect: Ref<'_, IUnknown>,
    ) -> Result<()> {
        Ok(())
    }

    fn DrawInlineObject(
        &self,
        _context: *const c_void,
        _x: f32,
        _y: f32,
        _object: Ref<'_, IDWriteInlineObject>,
        _sideways: BOOL,
        _right_to_left: BOOL,
        _effect: Ref<'_, IUnknown>,
    ) -> Result<()> {
        Ok(())
    }
}

impl IDWritePixelSnapping_Impl for Counter_Impl {
    fn IsPixelSnappingDisabled(&self, _context: *const c_void) -> Result<BOOL> {
        Ok(true.into())
    }

    fn GetCurrentTransform(
        &self,
        _context: *const c_void,
        transform: *mut DWRITE_MATRIX,
    ) -> Result<()> {
        // SAFETY: DirectWrite passes a valid matrix to fill in.
        if let Some(transform) = unsafe { transform.as_mut() } {
            *transform = DWRITE_MATRIX {
                m11: 1.0,
                m22: 1.0,
                ..Default::default()
            };
        }
        Ok(())
    }

    fn GetPixelsPerDip(&self, _context: *const c_void) -> Result<f32> {
        Ok(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Advances Segoe UI Emoji reported on Windows Server 2025 at 144 pixels.
    const EMOJI: f32 = 197.71875;

    #[test]
    fn a_family_built_from_parts_is_one_picture() {
        assert!(joined(2, 98.78906 + 81.63281, true, EMOJI));
        assert!(joined(1, EMOJI, true, EMOJI));
    }

    #[test]
    fn side_by_side_parts_are_not_one_picture() {
        assert!(!joined(2, EMOJI * 2.0, true, EMOJI));
        assert!(!joined(2, 79.171875 + 60.046875, false, EMOJI));
        assert!(!joined(0, 0.0, false, EMOJI));
    }

    #[test]
    fn segoe_draws_colour_emoji_in_its_cell() {
        let renderer = DirectWrite::probe().expect("Segoe UI Emoji draws in colour");
        let face = renderer.render("\u{1F600}", 72).expect("grinning face");
        assert_eq!(face.size[1], 72);
        assert!(face.has_colour());
        for sequence in ["👨‍👩‍👧‍👦", "👍🏽"] {
            assert!(
                renderer.render(sequence, 72).is_some(),
                "{sequence} is one picture"
            );
        }
        assert!(renderer.render("😀\u{200D}😀", 72).is_none());
        let scotland = "\u{1F3F4}\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}";
        assert!(renderer.render(scotland, 72).is_none());
    }
}
