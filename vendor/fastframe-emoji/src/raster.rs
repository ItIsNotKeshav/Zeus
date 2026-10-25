//! Premultiplied RGBA pictures: decoding a strike's PNG, scaling, and
//! compositing glyph layers into one picture.

/// An emoji drawn as pixels: premultiplied, sRGB, rows top to bottom.
#[derive(Clone, PartialEq, Eq)]
pub struct Picture {
    /// Width and height in pixels.
    pub size: [usize; 2],
    /// Four bytes per pixel, red, green, blue and alpha, with the colour
    /// already multiplied by alpha.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Picture")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl Picture {
    /// A transparent picture.
    pub(crate) fn empty(width: usize, height: usize) -> Self {
        Self {
            size: [width, height],
            rgba: vec![0; width * height * 4],
        }
    }

    /// Whether any pixel has colour (not all grey), for tests.
    #[must_use]
    pub fn has_colour(&self) -> bool {
        self.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] > 0 && (pixel[0] != pixel[1] || pixel[1] != pixel[2]))
    }

    /// The share of pixels that are not fully transparent, for tests.
    #[must_use]
    pub fn coverage(&self) -> f32 {
        let pixels = self.size[0] * self.size[1];
        if pixels == 0 {
            return 0.0;
        }
        let inked = self
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count();
        inked as f32 / pixels as f32
    }

    /// Draws `layer`, scaled to `width` by `height` pixels, with its top
    /// left corner at `x`, `y` (which may be outside), over this picture.
    pub(crate) fn draw(&mut self, layer: &Picture, x: i64, y: i64, width: usize, height: usize) {
        let scaled = layer.scaled(width, height);
        let [own_width, own_height] = self.size;
        for row in 0..height {
            let target_y = y + row as i64;
            if target_y < 0 || target_y >= own_height as i64 {
                continue;
            }
            for column in 0..width {
                let target_x = x + column as i64;
                if target_x < 0 || target_x >= own_width as i64 {
                    continue;
                }
                let from = (row * width + column) * 4;
                let to = (target_y as usize * own_width + target_x as usize) * 4;
                let source = &scaled.rgba[from..from + 4];
                let keep = 255 - u32::from(source[3]);
                for (under, over) in self.rgba[to..to + 4].iter_mut().zip(source) {
                    let blended = u32::from(*over) + (u32::from(*under) * keep + 127) / 255;
                    *under = blended.min(255) as u8;
                }
            }
        }
    }

    /// This picture flipped left to right.
    #[must_use]
    pub(crate) fn mirrored(&self) -> Picture {
        let width = self.size[0];
        let rgba = self
            .rgba
            .as_chunks::<4>()
            .0
            .chunks(width.max(1))
            .flat_map(|row| row.iter().rev().flatten().copied())
            .collect();
        Picture {
            size: self.size,
            rgba,
        }
    }

    /// This picture resampled to `width` by `height` with a tent filter
    /// that widens when shrinking, so every source pixel counts. Working on
    /// premultiplied pixels keeps transparent neighbours from darkening the
    /// edges.
    #[must_use]
    pub(crate) fn scaled(&self, width: usize, height: usize) -> Picture {
        if [width, height] == self.size {
            return self.clone();
        }
        if width == 0 || height == 0 || self.size[0] == 0 || self.size[1] == 0 {
            return Picture::empty(width, height);
        }
        let [source_width, source_height] = self.size;
        let pixels: Vec<[f32; 4]> = self
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| {
                [
                    f32::from(pixel[0]),
                    f32::from(pixel[1]),
                    f32::from(pixel[2]),
                    f32::from(pixel[3]),
                ]
            })
            .collect();
        // Rows first, then columns.
        let horizontal = weights(source_width, width);
        let mut wide = vec![[0.0f32; 4]; width * source_height];
        for row in 0..source_height {
            for (column, taps) in horizontal.iter().enumerate() {
                let mut sum = [0.0f32; 4];
                for (from, weight) in taps {
                    let pixel = pixels[row * source_width + from];
                    for channel in 0..4 {
                        sum[channel] += pixel[channel] * weight;
                    }
                }
                wide[row * width + column] = sum;
            }
        }
        let vertical = weights(source_height, height);
        let mut rgba = Vec::with_capacity(width * height * 4);
        for taps in &vertical {
            for column in 0..width {
                let mut sum = [0.0f32; 4];
                for (from, weight) in taps {
                    let pixel = wide[from * width + column];
                    for channel in 0..4 {
                        sum[channel] += pixel[channel] * weight;
                    }
                }
                let alpha = sum[3].round().clamp(0.0, 255.0);
                for value in &sum[..3] {
                    // Premultiplied colour never exceeds its alpha.
                    rgba.push(value.round().clamp(0.0, alpha) as u8);
                }
                rgba.push(alpha as u8);
            }
        }
        Picture {
            size: [width, height],
            rgba,
        }
    }
}

/// For each target pixel along one axis, the source pixels it samples and
/// their weights, which sum to one.
fn weights(source: usize, target: usize) -> Vec<Vec<(usize, f32)>> {
    let scale = source as f32 / target as f32;
    let support = scale.max(1.0);
    (0..target)
        .map(|index| {
            let centre = (index as f32 + 0.5) * scale;
            let first = (centre - support).floor().max(0.0) as usize;
            let last = ((centre + support).ceil() as usize).min(source);
            let mut taps: Vec<(usize, f32)> = (first..last)
                .map(|from| {
                    let distance = ((from as f32 + 0.5) - centre).abs() / support;
                    (from, (1.0 - distance).max(0.0))
                })
                .filter(|(_, weight)| *weight > 0.0)
                .collect();
            let total: f32 = taps.iter().map(|(_, weight)| weight).sum();
            if total > 0.0 {
                for (_, weight) in &mut taps {
                    *weight /= total;
                }
            } else {
                let nearest = (centre as usize).min(source - 1);
                taps = vec![(nearest, 1.0)];
            }
            taps
        })
        .collect()
}

/// Decodes a strike's PNG to a premultiplied picture.
pub(crate) fn decode_png(data: &[u8]) -> Option<Picture> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8() | png::Transformations::ALPHA,
    );
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let (width, height) = (info.width as usize, info.height as usize);
    let samples = &buffer[..info.buffer_size()];
    let mut rgba = Vec::with_capacity(width * height * 4);
    let mut push = |red: u8, green: u8, blue: u8, alpha: u8| {
        let multiply = |value: u8| ((u32::from(value) * u32::from(alpha) + 127) / 255) as u8;
        rgba.extend_from_slice(&[multiply(red), multiply(green), multiply(blue), alpha]);
    };
    match info.color_type {
        png::ColorType::Rgba => samples
            .as_chunks::<4>()
            .0
            .iter()
            .for_each(|pixel| push(pixel[0], pixel[1], pixel[2], pixel[3])),
        png::ColorType::Rgb => samples
            .as_chunks::<3>()
            .0
            .iter()
            .for_each(|pixel| push(pixel[0], pixel[1], pixel[2], 255)),
        png::ColorType::GrayscaleAlpha => samples
            .as_chunks::<2>()
            .0
            .iter()
            .for_each(|pixel| push(pixel[0], pixel[0], pixel[0], pixel[1])),
        png::ColorType::Grayscale => samples
            .iter()
            .for_each(|value| push(*value, *value, *value, 255)),
        png::ColorType::Indexed => return None,
    }
    (rgba.len() == width * height * 4).then_some(Picture {
        size: [width, height],
        rgba,
    })
}

/// A premultiplied BGRA strike bitmap as a picture.
pub(crate) fn from_bgra(data: &[u8], width: usize, height: usize) -> Option<Picture> {
    let rgba: Vec<u8> = data
        .get(..width * height * 4)?
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]])
        .collect();
    Some(Picture {
        size: [width, height],
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(width: usize, height: usize, pixel: [u8; 4]) -> Picture {
        Picture {
            size: [width, height],
            rgba: pixel.repeat(width * height),
        }
    }

    #[test]
    fn scaling_keeps_edge_colours() {
        let side = 144;
        let mut canvas = Picture::empty(side, side);
        for row in 0..side {
            for column in 0..side / 2 {
                let at = (row * side + column) * 4;
                canvas.rgba[at..at + 4].copy_from_slice(&[128, 0, 0, 128]);
            }
        }
        let small = canvas.scaled(72, 72);
        for pixel in small
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
        {
            let red = u32::from(pixel[0]) * 255 / u32::from(pixel[3]);
            assert!(red >= 250 && pixel[1] == 0 && pixel[2] == 0, "{pixel:?}");
        }
    }

    #[test]
    fn a_flat_colour_stays_flat_at_any_size() {
        let blue = filled(10, 7, [0, 0, 200, 200]);
        for (width, height) in [(3, 2), (10, 7), (31, 29)] {
            let scaled = blue.scaled(width, height);
            assert_eq!(scaled.size, [width, height]);
            assert!(
                scaled
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| *pixel == [0, 0, 200, 200])
            );
        }
    }

    #[test]
    fn layers_are_drawn_over_each_other_and_clipped() {
        let mut canvas = filled(4, 4, [0, 0, 255, 255]);
        canvas.draw(&filled(2, 2, [255, 0, 0, 255]), -1, 3, 2, 2);
        // Only the top right pixel of the layer lands, at the bottom left.
        assert_eq!(&canvas.rgba[12 * 4..12 * 4 + 4], &[255, 0, 0, 255]);
        assert_eq!(&canvas.rgba[13 * 4..13 * 4 + 4], &[0, 0, 255, 255]);
        let mut clear = Picture::empty(2, 2);
        clear.draw(&filled(1, 1, [0, 64, 0, 128]), 0, 0, 1, 1);
        assert_eq!(&clear.rgba[..4], &[0, 64, 0, 128]);
    }

    #[test]
    fn mirroring_reverses_each_row() {
        let picture = Picture {
            size: [3, 2],
            rgba: (0..24).collect(),
        };
        let mirrored = picture.mirrored();
        assert_eq!(mirrored.size, [3, 2]);
        assert_eq!(
            mirrored.rgba,
            [
                8, 9, 10, 11, 4, 5, 6, 7, 0, 1, 2, 3, 20, 21, 22, 23, 16, 17, 18, 19, 12, 13, 14,
                15
            ]
        );
        assert_eq!(mirrored.mirrored(), picture);
    }

    #[test]
    fn bgra_is_reordered() {
        let picture = from_bgra(&[1, 2, 3, 4], 1, 1).expect("one pixel");
        assert_eq!(picture.rgba, vec![3, 2, 1, 4]);
        assert!(from_bgra(&[1, 2, 3], 1, 1).is_none());
    }

    #[test]
    fn garbage_is_not_a_png() {
        assert!(decode_png(b"not a png").is_none());
    }
}
