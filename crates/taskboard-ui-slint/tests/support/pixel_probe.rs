// SPDX-License-Identifier: AGPL-3.0-only

//! Pixel-ink floors for the icon probes (r4 chunk 3): the shared
//! "visibly off the background" counter both icon-bearing suites
//! build on. A max-channel delta of ≥ 12 clears the antialiasing
//! band — a decoded-but-untinted (black) glyph never does.

use crate::support::golden_compare::Rgba8Image;

/// The count of pixels in the clamped region visibly off `bg` (a
/// max-channel delta of ≥ 12). A floor, never a ceiling: at the 0.5×
/// extreme the sub-pixel strokes blend toward the background, the
/// region still carries glyph ink.
pub fn count_off_background(
    image: &Rgba8Image,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    bg: [u8; 3],
) -> u32 {
    let (x0, y0) = (x0.min(image.width), y0.min(image.height));
    let (x1, y1) = (x1.min(image.width), y1.min(image.height));
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y as usize * image.width as usize + x as usize) * 4;
            let delta = (i32::from(image.data[i]) - i32::from(bg[0]))
                .abs()
                .max((i32::from(image.data[i + 1]) - i32::from(bg[1])).abs())
                .max((i32::from(image.data[i + 2]) - i32::from(bg[2])).abs());
            if delta >= 12 {
                count += 1;
            }
        }
    }
    count
}
