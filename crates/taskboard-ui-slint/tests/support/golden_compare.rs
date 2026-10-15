// SPDX-License-Identifier: AGPL-3.0-only

//! The golden-tier comparison (design §10/D7): exact dimensions plus
//! a hand-rolled dHash-64 perceptual hash with a recorded tolerance
//! (Hamming distance ≤ 8 of 64 bits; see the chunk plan §7 for the
//! budget). Pure Rust — no slint types, no IO beyond PNG bytes.

/// An RGBA8 image buffer (row-major, 4 bytes per pixel): the common
/// currency of the driver's snapshots and the decoded golden PNGs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba8Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Rgba8Image {
    #[must_use]
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Self {
        debug_assert_eq!(data.len(), width as usize * height as usize * 4);
        Self {
            width,
            height,
            data,
        }
    }
}

/// Recolors to 8-bit grayscale (Rec. 601 luma, the conventional dHash
/// input) with one sample per *source* pixel; the box-average
/// downscale happens in [`dhash64`].
#[allow(clippy::many_single_char_names)] // (r, g, b, x, y) are the domain's own names
fn gray_at(image: &Rgba8Image, px: u32, py: u32) -> u16 {
    let i = (py as usize * image.width as usize + px as usize) * 4;
    let (red, green, blue) = (
        u32::from(image.data[i]),
        u32::from(image.data[i + 1]),
        u32::from(image.data[i + 2]),
    );
    u16::try_from((red * 299 + green * 587 + blue * 114) / 1000).unwrap_or(u16::MAX)
}

/// The dHash-64: box-average downscale to 9×8 grayscale, then one
/// comparison bit per cell (`left > right`), 64 bits row-major. Pure
/// and total over every image size (a 1-px-wide image yields all
/// zeros — the journeys never feed one).
#[must_use]
pub fn dhash64(image: &Rgba8Image) -> u64 {
    let (w, h) = (image.width, image.height);
    let mut hash = 0u64;
    for row in 0..8u32 {
        for col in 0..8u32 {
            // Box average of the source rect this 9×8 cell covers.
            let avg = |cx: u32| -> u16 {
                let x0 = cx * w / 9;
                let x1 = ((cx + 1) * w / 9).max(x0 + 1);
                let y0 = row * h / 8;
                let y1 = ((row + 1) * h / 8).max(y0 + 1);
                let mut sum = 0u64;
                let mut n = 0u64;
                for y in y0..y1.min(h) {
                    for x in x0..x1.min(w) {
                        sum += u64::from(gray_at(image, x, y));
                        n += 1;
                    }
                }
                u64::checked_div(sum, n)
                    .and_then(|v| u16::try_from(v).ok())
                    .unwrap_or(u16::MAX)
            };
            let bit = avg(col) > avg(col + 1);
            hash |= u64::from(bit) << (row * 8 + col);
        }
    }
    hash
}

/// The Hamming distance between two hashes (the compare metric).
#[must_use]
pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// The recorded tolerance (D7): cross-machine anti-aliasing noise of
/// the software rasterizer measures 0–4 bits; the threshold doubles
/// that headroom. A real regression flips ≥ 20 bits. Re-recording
/// happens in the chunk plan's amendment — never silently here.
pub const TOLERANCE: u32 = 8;

/// The golden verdict for one image pair: dimensions must be exact
/// (structural), the hashes within [`TOLERANCE`].
pub fn matches(golden: &Rgba8Image, candidate: &Rgba8Image) -> Result<(), String> {
    if (golden.width, golden.height) != (candidate.width, candidate.height) {
        return Err(format!(
            "dimensions differ: golden {}×{}, candidate {}×{}",
            golden.width, golden.height, candidate.width, candidate.height
        ));
    }
    let (g, c) = (dhash64(golden), dhash64(candidate));
    let distance = hamming(g, c);
    if distance <= TOLERANCE {
        if distance > 0 {
            tracing::warn!(
                distance,
                TOLERANCE,
                "golden drift within tolerance (visible only in the hash)"
            );
        }
        Ok(())
    } else {
        Err(format!(
            "perceptual hashes differ by {distance} bits (> {TOLERANCE}): \
             golden {g:#018x}, candidate {c:#018x} — if intended, regenerate \
             the goldens (TASKBOARD_REGEN_GOLDENS=1) and review the PNG diff"
        ))
    }
}

/// Encodes `image` as a PNG (the regeneration path's writer side).
///
/// # Errors
///
/// The `png` encoder's typed error, wrapped.
pub fn encode_png(image: &Rgba8Image) -> Result<Vec<u8>, png::EncodingError> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&image.data)?;
    }
    Ok(bytes)
}

/// Decodes a PNG's RGBA8 bytes (the compare path's reader side).
///
/// # Errors
///
/// The `png` decoder's typed error, wrapped; a non-RGBA8 layout is
/// [`DecodeError::UnexpectedLayout`].
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("png decode failed")]
    Png(#[from] png::DecodingError),
    #[error("unexpected png layout")]
    UnexpectedLayout,
}

pub fn decode_png(bytes: &[u8]) -> Result<Rgba8Image, DecodeError> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info()?;
    let Some(size) = reader.output_buffer_size() else {
        return Err(DecodeError::UnexpectedLayout);
    };
    let mut buffer = vec![0; size];
    let info = reader.next_frame(&mut buffer)?;
    let (Ok(width), Ok(height)) = (usize::try_from(info.width), usize::try_from(info.height))
    else {
        return Err(DecodeError::UnexpectedLayout);
    };
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(DecodeError::UnexpectedLayout);
    }
    buffer.truncate(width * height * 4);
    Ok(Rgba8Image::new(info.width, info.height, buffer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Rgba8Image {
        Rgba8Image::new(width, height, rgba.repeat(width as usize * height as usize))
    }

    /// T3 — PNG round-trip: encode → decode is the identity buffer.
    #[test]
    fn png_round_trip_preserves_the_buffer() {
        let mut gradient = Vec::new();
        for y in 0..40u32 {
            for x in 0..60u32 {
                gradient.extend_from_slice(&[
                    u8::try_from(x * 4 % 256).unwrap_or(0),
                    u8::try_from(y * 6 % 256).unwrap_or(0),
                    7,
                    255,
                ]);
            }
        }
        let image = Rgba8Image::new(60, 40, gradient);
        let decoded = decode_png(&encode_png(&image).expect("encodes")).expect("decodes");
        assert_eq!(decoded, image);
    }

    // T3 — the hash is stable under ±1 LSB per-channel noise on
    // every pixel (proptest). The fixture is a strong horizontal ramp
    // with heavy dither — adjacent cell averages differ by dozens of
    // gray levels, like a real UI capture — so the comparison bits
    // never sit on a knife edge.
    proptest! {
        #[test]
        fn dhash_is_stable_under_one_lsb_noise(seed in any::<u64>()) {
            let mut state = seed | 1;
            let mut next = move || {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                u8::try_from(state >> 33).unwrap_or(0)
            };
            let mut data = Vec::with_capacity(64 * 48 * 4);
            for x in 0..64u32 {
                for _ in 0..48u32 {
                    let dither = i16::from(next() % 41) - 20;
                    let v = u8::try_from(i16::clamp(i16::try_from(x * 3).unwrap_or(i16::MAX) + dither, 0, i16::from(u8::MAX))).unwrap_or(0);
                    data.extend_from_slice(&[v, v, v, 255]);
                }
            }
            let image = Rgba8Image::new(64, 48, data);
            // +1 LSB on every colour channel; the alpha byte is spared.
            let noisy = Rgba8Image::new(
                64,
                48,
                image
                    .data
                    .iter()
                    .enumerate()
                    .map(|(i, &v)| if i % 4 == 3 { v } else { v.saturating_add(1) })
                    .collect(),
            );
            prop_assert_eq!(hamming(dhash64(&image), dhash64(&noisy)), 0);
        }
    }

    /// T3 — the hash registers a one-row shift of a textured fixture
    /// (a flat rectangle's vertical shift is invisible to dHash by
    /// construction — horizontal comparisons only) and reads a
    /// structural change (a block moved across the view) as far above
    /// the tolerance — the calibration case of chunk plan §7.
    #[test]
    fn dhash_registers_shifts_and_far_exceeds_tolerance_on_structure() {
        let paint = |data: &mut [u8], row_offset: u32, block: Option<u32>| {
            for y in 0..64u32 {
                for x in 0..64u32 {
                    let i = ((y * 64 + x) * 4) as usize;
                    let value = match block {
                        // A filled block whose left edge sits at `b`.
                        Some(b) if x >= b && x < b + 24 => 255,
                        // A diagonal texture so a one-row shift moves
                        // content the horizontal comparisons sample.
                        _ => u8::try_from((x * 3 + (y + row_offset) * 5) % 256).unwrap_or(0),
                    };
                    data[i..i + 3].fill(value);
                    data[i + 3] = 255;
                }
            }
        };
        let mut unshifted = vec![0u8; 64 * 64 * 4];
        let mut shifted = vec![0u8; 64 * 64 * 4];
        paint(&mut unshifted, 0, Some(8));
        paint(&mut shifted, 1, Some(8));
        let shift_distance = hamming(
            dhash64(&Rgba8Image::new(64, 64, unshifted.clone())),
            dhash64(&Rgba8Image::new(64, 64, shifted)),
        );
        assert!(shift_distance > 0, "a textured one-row shift must register");
        let mut across = vec![0u8; 64 * 64 * 4];
        paint(&mut across, 0, Some(40));
        let distance = hamming(
            dhash64(&Rgba8Image::new(64, 64, unshifted)),
            dhash64(&Rgba8Image::new(64, 64, across)),
        );
        assert!(
            distance > TOLERANCE,
            "a block moved across the view is a real regression: {distance}"
        );
    }

    /// T3 — identical images match; the compare path also enforces
    /// the exact-dimensions precondition.
    #[test]
    fn compare_enforces_dimensions_and_equality() {
        let a = solid(30, 20, [10, 20, 30, 255]);
        assert!(matches(&a, &a).is_ok());
        let b = solid(31, 20, [10, 20, 30, 255]);
        assert!(matches(&a, &b).is_err());
    }

    /// T3 — solid images (no gradient) hash to zero and match.
    #[test]
    fn solid_images_hash_to_zero() {
        assert_eq!(dhash64(&solid(16, 16, [128, 128, 128, 255])), 0);
    }
}
