// SPDX-License-Identifier: AGPL-3.0-only

//! The golden-tier comparison (design §10/D7, upgraded per the golden
//! coverage wave plan §3): exact dimensions, then a **scale-aware
//! box-averaged cell grid** over three Rec. 601 channels — luma (Y)
//! plus the two chroma channels (Cb/Cr) — each cell checked against
//! its own recorded budget.
//!
//! # The grid
//!
//! Cell *physical* size, not a fixed grid count: the grid resolution
//! derives from the image dimensions (`grid_dims`) so a cell is
//! roughly [`CELL_TARGET_PX`] physical pixels on a side (floored at
//! [`MIN_GRID`] columns/rows so tiny images still average). Both
//! compare sides derive the same grid from the exact-dimension
//! precondition, so the sides agree by construction. The earlier fixed
//! 32×24 grid measured ~47×37 px/cell at the 3.0/1500 matrix corner —
//! where the mutation audit measured blindness to ≤16 px badges and
//! 2 px band shifts; the scale-aware sizing keeps those classes
//! visible at every render size.
//!
//! # The channels
//!
//! Luma (`Y = (299R + 587G + 114B)/1000`) with budget
//! [`MAX_CELL_DIFF`]; the two chroma channels as the offset-128
//! integer Rec. 601 forms with budget [`MAX_CHROMA_CELL_DIFF`]. A
//! same-luma hue swap (e.g. a danger↔success tone flip) reads ~0 luma
//! drift but a large chroma-cell diff — the audit's "always invisible"
//! class is visible at every scale.
//!
//! # Budgets (recorded recalibration, wave plan §3.1)
//!
//! Both budgets stay `8/255` levels: the software renderer is
//! byte-deterministic and the render clock frozen, so the measured
//! re-capture noise is exactly 0; the ±1-LSB proptest below pins the
//! noise floor (≤1 luma level, ≤1 chroma level per cell), and the
//! pinned sensitivity tests pin the audit's signal floors. The budgets
//! are evidence-commented constants — never tuned per golden, never
//! widened to admit a failing golden (a failure is a finding to fix).

/// The target physical pixels per cell side of the scale-aware grid.
pub const CELL_TARGET_PX: u32 = 24;
/// The grid floor: even the smallest probe images keep an 8×8 grid.
pub const MIN_GRID: u32 = 8;

/// The per-cell luma budget (of 255 levels): machine anti-aliasing
/// shifts a cell mean by well under one level; a moved or deleted card
/// shifts its cells by tens. Re-recording happens in the chunk plan's
/// amendment — never silently here.
pub const MAX_CELL_DIFF: u16 = 8;
/// The per-cell chroma budget (of 255 levels per channel), calibrated
/// the same way as the luma budget: the renderer's noise floor is 0
/// (byte-deterministic rasterizer, frozen clock), the ±1-LSB proptest
/// holds ≤1 chroma level, and the pinned hue-swap test reads hundreds
/// of levels for the audit's tone-flip class.
pub const MAX_CHROMA_CELL_DIFF: u16 = 8;

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

/// The scale-aware grid resolution for an image size: cells of
/// roughly [`CELL_TARGET_PX`] physical pixels, floored at [`MIN_GRID`]
/// per side, never exceeding one column/row per pixel. Pure and total.
#[must_use]
pub fn grid_dims(width: u32, height: u32) -> (u32, u32) {
    let cols = (width / CELL_TARGET_PX).max(MIN_GRID).min(width.max(1));
    let rows = (height / CELL_TARGET_PX).max(MIN_GRID).min(height.max(1));
    (cols, rows)
}

/// The Rec. 601 luma at one pixel (the BT.601 integer form).
#[must_use]
fn luma_at(image: &Rgba8Image, px: u32, py: u32) -> u32 {
    let (red, green, blue) = rgb_at(image, px, py);
    (red * 299 + green * 587 + blue * 114) / 1000
}

#[must_use]
fn rgb_at(image: &Rgba8Image, px: u32, py: u32) -> (u32, u32, u32) {
    let i = (py as usize * image.width as usize + px as usize) * 4;
    (
        u32::from(image.data[i]),
        u32::from(image.data[i + 1]),
        u32::from(image.data[i + 2]),
    )
}

/// The offset-128 integer Rec. 601 chroma pair at one pixel
/// (`Cb`, `Cr`, each 0–255). Coefficients scaled by 256:
/// `Cb = 128 + (128·B − 85·G − 43·R)/256`,
/// `Cr = 128 + (128·R − 107·G − 21·B)/256`.
#[must_use]
fn chroma_at(image: &Rgba8Image, px: u32, py: u32) -> (u32, u32) {
    let (red, green, blue) = rgb_at(image, px, py);
    let (r, g, b) = (
        i32::try_from(red).unwrap_or(i32::MAX),
        i32::try_from(green).unwrap_or(i32::MAX),
        i32::try_from(blue).unwrap_or(i32::MAX),
    );
    let cb = 128 + (128 * b - 85 * g - 43 * r + 128) / 256;
    let cr = 128 + (128 * r - 107 * g - 21 * b + 128) / 256;
    let clamp = |v: i32| u32::try_from(v.clamp(0, 255)).unwrap_or(255);
    (clamp(cb), clamp(cr))
}

/// The box-averaged cell means of one per-pixel channel over the
/// scale-aware grid. Pure and total over every image size.
fn cell_means(
    image: &Rgba8Image,
    cols: u32,
    rows: u32,
    channel: impl Fn(&Rgba8Image, u32, u32) -> u32,
) -> Vec<u16> {
    let (w, h) = (image.width, image.height);
    let mut grid = Vec::with_capacity((cols * rows) as usize);
    for row in 0..rows {
        for col in 0..cols {
            let x0 = col * w / cols;
            let x1 = ((col + 1) * w / cols).max(x0 + 1);
            let y0 = row * h / rows;
            let y1 = ((row + 1) * h / rows).max(y0 + 1);
            let mut sum = 0u64;
            let mut n = 0u64;
            for y in y0..y1.min(h) {
                for x in x0..x1.min(w) {
                    sum += u64::from(channel(image, x, y));
                    n += 1;
                }
            }
            grid.push(u16::try_from(sum / n.max(1)).unwrap_or(u16::MAX));
        }
    }
    grid
}

/// The luma cell grid: scale-aware box-averaged Rec. 601 luma means,
/// row-major.
#[must_use]
pub fn cell_grid(image: &Rgba8Image) -> Vec<u16> {
    let (cols, rows) = grid_dims(image.width, image.height);
    cell_means(image, cols, rows, luma_at)
}

/// The chroma cell grids: the scale-aware box-averaged `Cb` and `Cr`
/// means, each row-major, paired with [`cell_grid`]'s luma grid.
#[must_use]
pub fn chroma_grids(image: &Rgba8Image) -> (Vec<u16>, Vec<u16>) {
    let (cols, rows) = grid_dims(image.width, image.height);
    (
        cell_means(image, cols, rows, |img, x, y| chroma_at(img, x, y).0),
        cell_means(image, cols, rows, |img, x, y| chroma_at(img, x, y).1),
    )
}

/// The largest per-cell difference between two grids.
#[must_use]
pub fn max_cell_diff(a: &[u16], b: &[u16]) -> u16 {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

/// The golden verdict for one image pair: dimensions must be exact
/// (structural), every luma cell within [`MAX_CELL_DIFF`], every
/// chroma cell within [`MAX_CHROMA_CELL_DIFF`].
pub fn matches(golden: &Rgba8Image, candidate: &Rgba8Image) -> Result<(), String> {
    if (golden.width, golden.height) != (candidate.width, candidate.height) {
        return Err(format!(
            "dimensions differ: golden {}×{}, candidate {}×{}",
            golden.width, golden.height, candidate.width, candidate.height
        ));
    }
    let diff = max_cell_diff(&cell_grid(golden), &cell_grid(candidate));
    if diff > MAX_CELL_DIFF {
        return Err(format!(
            "luma cells differ by {diff} (> {MAX_CELL_DIFF}) — if intended, \
             regenerate the goldens (TASKBOARD_REGEN_GOLDENS=1) and review the PNG diff"
        ));
    }
    let golden_chroma = chroma_grids(golden);
    let candidate_chroma = chroma_grids(candidate);
    let chroma_diff = max_cell_diff(&golden_chroma.0, &candidate_chroma.0)
        .max(max_cell_diff(&golden_chroma.1, &candidate_chroma.1));
    if chroma_diff > MAX_CHROMA_CELL_DIFF {
        return Err(format!(
            "chroma cells differ by {chroma_diff} (> {MAX_CHROMA_CELL_DIFF}) — \
             a hue-level change is visible only here; if intended, regenerate \
             the goldens (TASKBOARD_REGEN_GOLDENS=1) and review the PNG diff"
        ));
    }
    if diff > 0 || chroma_diff > 0 {
        tracing::warn!(
            luma_diff = diff,
            chroma_diff,
            MAX_CELL_DIFF,
            MAX_CHROMA_CELL_DIFF,
            "golden drift within tolerance (visible only in the cell grid)"
        );
    }
    Ok(())
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

    /// T3 — the scale-aware grid stays fine at the matrix's largest
    /// corner: a fixed grid there was ~47×37 px/cell (the audit's
    /// blindness window); the derived grid resolves well below 20 px.
    #[test]
    fn grid_dims_scale_aware_at_the_matrix_corners() {
        // 3.0 scale, expansive breakpoint: 62×37 cells, ~24 px/cell.
        assert_eq!(grid_dims(1500, 900), (62, 37));
        // The compact breakpoint keeps a usable grid.
        assert_eq!(grid_dims(600, 900), (25, 37));
        // The grid floors on tiny probe images.
        assert_eq!(grid_dims(16, 16), (8, 8));
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

    // T3 — the chroma grids share the noise floor: ±1 LSB per channel
    // shifts a chroma cell by at most one level (the integer Rec. 601
    // coefficients sum to ±256, so the shift rounds to ≤1).
    proptest! {
        #[test]
        fn chroma_grids_are_stable_under_one_lsb_noise(seed in any::<u64>()) {
            let mut state = seed | 1;
            let mut next = move || {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                u8::try_from(state >> 33).unwrap_or(0)
            };
            let data: Vec<u8> = (0..64 * 48 * 4).map(|_| next()).collect();
            let image = Rgba8Image::new(64, 48, data);
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
            let (cb_a, cr_a) = chroma_grids(&image);
            let (cb_b, cr_b) = chroma_grids(&noisy);
            prop_assert!(max_cell_diff(&cb_a, &cb_b) <= 1);
            prop_assert!(max_cell_diff(&cr_a, &cr_b) <= 1);
        }
    }

    // T3 — the audit's blind spot #1: a ≤16 px badge appearing at the
    // largest render size exceeds the luma budget under the scale-aware
    // grid (the fixed 32×24 grid averaged it away at this corner).
    #[test]
    fn sub_cell_badge_at_the_largest_render_exceeds_the_budget() {
        let paint = |badge: bool| {
            let mut data = Vec::with_capacity(1500 * 900 * 4);
            for y in 0..900u32 {
                for x in 0..1500u32 {
                    let on = badge && (40..56).contains(&x) && (40..56).contains(&y);
                    let v = if on { 200 } else { 40 };
                    data.extend_from_slice(&[v, v, v, 255]);
                }
            }
            Rgba8Image::new(1500, 900, data)
        };
        let base = paint(false);
        let with_badge = paint(true);
        let diff = max_cell_diff(&cell_grid(&base), &cell_grid(&with_badge));
        assert!(
            diff > MAX_CELL_DIFF,
            "a 16px badge at the largest render is a real regression: {diff}"
        );
        assert!(matches(&base, &with_badge).is_err());
    }

    // T3 — the audit's blind spot #2: a same-luma red↔blue hue swap
    // reads ~0 luma drift but exceeds the chroma budget at every grid.
    // Both solids land within a luma level of each other by
    // construction (134 vs 133), so the luma channel alone is blind.
    #[test]
    fn same_luma_hue_swap_exceeds_the_chroma_budget() {
        let warm = solid(300, 200, [255, 100, 0, 255]); // luma 134
        let cool = solid(300, 200, [0, 178, 255, 255]); // luma 133
        let luma = max_cell_diff(&cell_grid(&warm), &cell_grid(&cool));
        assert!(
            luma <= MAX_CELL_DIFF,
            "fixture drift: the pair must be same-luma for this test to pin chroma ({luma})"
        );
        let warm_chroma = chroma_grids(&warm);
        let cool_chroma = chroma_grids(&cool);
        let chroma = max_cell_diff(&warm_chroma.0, &cool_chroma.0)
            .max(max_cell_diff(&warm_chroma.1, &cool_chroma.1));
        assert!(
            chroma > MAX_CHROMA_CELL_DIFF,
            "a same-luma hue swap is a real regression the chroma channels must read: {chroma}"
        );
        assert!(matches(&warm, &cool).is_err());
    }

    // T3 — the audit's blind spot #3: a 2px band shift at high
    // resolution exceeds the luma budget under the scale-aware grid
    // (the fixed grid's ~47px cells averaged the shift to ≲ one
    // budget-level at this corner).
    #[test]
    fn two_px_band_shift_at_high_resolution_exceeds_the_budget() {
        let paint = |offset: u32| {
            let mut data = Vec::with_capacity(1500 * 900 * 4);
            for _y in 0..900u32 {
                for x in 0..1500u32 {
                    let on = x >= 100 + offset && x < 140 + offset;
                    let v = if on { 255 } else { 0 };
                    data.extend_from_slice(&[v, v, v, 255]);
                }
            }
            Rgba8Image::new(1500, 900, data)
        };
        let base = paint(0);
        let shifted = paint(2);
        let diff = max_cell_diff(&cell_grid(&base), &cell_grid(&shifted));
        assert!(
            diff > MAX_CELL_DIFF,
            "a 2px band shift at high resolution is a real regression: {diff}"
        );
        assert!(matches(&base, &shifted).is_err());
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

    /// T3 — flat images hash to their constant channels: the luma cell
    /// reads the constant luma and both chroma cells read the constant
    /// (offset-128) chroma.
    #[test]
    fn solid_images_give_constant_cells() {
        let image = solid(16, 16, [128, 128, 128, 255]);
        assert!(cell_grid(&image).iter().all(|&cell| cell == 128));
        let (cb, cr) = chroma_grids(&image);
        // Neutral gray: Cb = Cr = 128 (the Rec. 601 neutral point).
        assert!(cb.iter().all(|&cell| cell == 128));
        assert!(cr.iter().all(|&cell| cell == 128));
    }
}
