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

/// The comparison grid (D7 amendment): box-averaged Rec. 601 luma
/// cells. The grid resolution was calibrated before landing (chunk
/// plan §7): dHash-64 at UI resolutions measured **0–2 bits** for the
/// plan's own regression classes (a card moved across columns, a card
/// deleted) — a 9×8 average is blind to small blocks. A 32×24 cell
/// grid reads the same perturbations far above the thresholds below
/// while anti-aliasing noise stays sub-LSB per cell.
pub const CELL_GRID_W: u32 = 32;
pub const CELL_GRID_H: u32 = 24;
/// The per-cell, per-channel budget (of 255 levels): machine anti-aliasing
/// shifts a cell mean by well under one level; a moved or deleted card
/// shifts its cells by tens. Re-recording happens in the chunk plan's
/// amendment — never silently here.
pub const MAX_CELL_DIFF: u16 = 8;
///
/// Calibration evidence (this chunk, before landing; recorded in the
/// PR description): re-mount noise 0; a card moved across columns
/// 12–20 at every breakpoint width; a missed scale step (0.5 vs 1.0)
/// 47. A deleted *task* (a content change, not a rendering one)
/// measures 0 — content is the component/journey tiers' territory,
/// not this tier's.
#[allow(clippy::many_single_char_names)] // (r, g, b, x, y) are the domain's own names
fn gray_at(image: &Rgba8Image, px: u32, py: u32) -> u32 {
    let i = (py as usize * image.width as usize + px as usize) * 4;
    let (red, green, blue) = (
        u32::from(image.data[i]),
        u32::from(image.data[i + 1]),
        u32::from(image.data[i + 2]),
    );
    (red * 299 + green * 587 + blue * 114) / 1000
}

/// The cell grid: `CELL_GRID_W × CELL_GRID_H` box-averaged luma
/// means, row-major. Pure and total over every image size.
#[must_use]
pub fn cell_grid(image: &Rgba8Image) -> Vec<u16> {
    let (w, h) = (image.width, image.height);
    let mut grid = Vec::with_capacity((CELL_GRID_W * CELL_GRID_H) as usize);
    for row in 0..CELL_GRID_H {
        for col in 0..CELL_GRID_W {
            let x0 = col * w / CELL_GRID_W;
            let x1 = ((col + 1) * w / CELL_GRID_W).max(x0 + 1);
            let y0 = row * h / CELL_GRID_H;
            let y1 = ((row + 1) * h / CELL_GRID_H).max(y0 + 1);
            let mut sum = 0u64;
            let mut n = 0u64;
            for y in y0..y1.min(h) {
                for x in x0..x1.min(w) {
                    sum += u64::from(gray_at(image, x, y));
                    n += 1;
                }
            }
            grid.push(u16::try_from(sum / n.max(1)).unwrap_or(u16::MAX));
        }
    }
    grid
}

/// The largest per-cell luma difference between two grids.
#[must_use]
pub fn max_cell_diff(a: &[u16], b: &[u16]) -> u16 {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

/// The golden verdict for one image pair: dimensions must be exact
/// (structural), every luma cell within [`MAX_CELL_DIFF`].
pub fn matches(golden: &Rgba8Image, candidate: &Rgba8Image) -> Result<(), String> {
    if (golden.width, golden.height) != (candidate.width, candidate.height) {
        return Err(format!(
            "dimensions differ: golden {}×{}, candidate {}×{}",
            golden.width, golden.height, candidate.width, candidate.height
        ));
    }
    let diff = max_cell_diff(&cell_grid(golden), &cell_grid(candidate));
    if diff <= MAX_CELL_DIFF {
        if diff > 0 {
            tracing::warn!(
                diff,
                MAX_CELL_DIFF,
                "golden drift within tolerance (visible only in the cell grid)"
            );
        }
        Ok(())
    } else {
        Err(format!(
            "luma cells differ by {diff} (> {MAX_CELL_DIFF}) — if intended, \
             regenerate the goldens (TASKBOARD_REGEN_GOLDENS=1) and review the PNG diff"
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

    // T3 — the grid is stable under ±1 LSB per-channel noise on
    // every pixel (proptest): a cell mean shifts by less than one
    // luma level, far inside the budget.
    proptest! {
        #[test]
        fn cell_grid_is_stable_under_one_lsb_noise(seed in any::<u64>()) {
            let mut state = seed | 1;
            let mut next = move || {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                u8::try_from(state >> 33).unwrap_or(0)
            };
            let data: Vec<u8> = (0..64 * 48 * 4).map(|_| next()).collect();
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
            prop_assert!(max_cell_diff(&cell_grid(&image), &cell_grid(&noisy)) <= 1);
        }
    }

    /// T3 — the grid is sensitive to the plan's regression classes: a
    /// filled block moved across the view (a card migrating columns)
    /// and a block deleted both read far above the budget.
    #[test]
    fn cell_grid_reads_structure_changes_as_real() {
        let paint = |data: &mut Vec<u8>, block: Option<(u32, u32)>| {
            data.clear();
            for y in 0..64u32 {
                for x in 0..64u32 {
                    let on = block
                        .is_some_and(|(bx, by)| x >= bx && x < bx + 20 && y >= by && y < by + 16);
                    let v = if on { 255 } else { 0 };
                    data.extend_from_slice(&[v, v, v, 255]);
                }
            }
        };
        let mut base_data = Vec::new();
        let mut moved_data = Vec::new();
        let mut removed_data = Vec::new();
        paint(&mut base_data, Some((4, 8)));
        paint(&mut moved_data, Some((40, 8)));
        paint(&mut removed_data, None);
        let base = Rgba8Image::new(64, 64, base_data);
        let moved = Rgba8Image::new(64, 64, moved_data);
        let removed = Rgba8Image::new(64, 64, removed_data);
        for (name, candidate) in [("moved", &moved), ("removed", &removed)] {
            let diff = max_cell_diff(&cell_grid(&base), &cell_grid(candidate));
            assert!(
                diff > MAX_CELL_DIFF,
                "a {name} block is a real regression: {diff}"
            );
            assert!(matches(&base, candidate).is_err());
        }
    }

    /// T3 — identical images match; the compare path also enforces
    /// the exact-dimensions precondition.
    #[test]
    fn compare_enforces_dimensions_and_equality() {
        let a = solid(30, 20, [10, 20, 30, 255]);
        assert_eq!(matches(&a, &a), Ok(()));
        let b = solid(31, 20, [10, 20, 30, 255]);
        assert!(matches(&a, &b).is_err());
    }

    /// T3 — flat images hash to their constant luma and match.
    #[test]
    fn solid_images_give_constant_cells() {
        let grid = cell_grid(&solid(16, 16, [128, 128, 128, 255]));
        assert!(grid.iter().all(|&cell| cell == 128));
    }
}
