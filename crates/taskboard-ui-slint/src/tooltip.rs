// SPDX-License-Identifier: AGPL-3.0-only

//! The tooltip placement math (round-1 D10): pure, single-source,
//! proptestable — the old markup formula degenerated exactly because
//! it was untestable. All values are logical pixels in window
//! coordinates; no slint types (the §5.5 extraction seam).

/// One placement problem: the anchor rect, the tooltip's measured
/// size, the window rect, the status bar's top edge, and the margin
/// and gap tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TooltipInput {
    /// The anchor rect (x, y, w, h) in window coordinates.
    pub anchor: (f32, f32, f32, f32),
    /// The tooltip's measured (w, h).
    pub tip: (f32, f32),
    /// The window (w, h).
    pub window: (f32, f32),
    /// The window-coordinates top of the status bar.
    pub status_top: f32,
    pub margin: f32,
    pub gap: f32,
}

/// Returns the tooltip's top-left so the rect stays inside the window
/// whenever it can fit at all: horizontally centred on the anchor and
/// clamped to the margins; below the anchor, flipped above when the
/// below placement would cross the status bar's top edge (or the
/// window bottom) *and* room exists above, else clamped inside the
/// window's vertical margins. A tip that cannot fit an axis (tip ≥
/// window) saturates at the margin on that axis — the point stays
/// inside; the rect cannot (nothing can).
#[must_use]
pub fn place(input: &TooltipInput) -> (f32, f32) {
    let TooltipInput {
        anchor: (ax, ay, aw, ah),
        tip: (tw, th),
        window: (ww, wh),
        status_top,
        margin,
        gap,
    } = *input;
    let x = (ax + aw / 2.0 - tw / 2.0).clamp(margin, (ww - tw - margin).max(margin));
    let below = ay + ah + gap;
    // The flip's floor is also bounded by the window's bottom margin:
    // with no (or a shallow) status bar, `status_top` alone would let
    // the flipped tip land inside the bottom margin strip — the
    // proptest's counterexample region (tooltip.txt).
    let floor = status_top.min(wh).min(wh - margin).max(margin);
    let crosses = below + th > floor;
    let above_y = ay - gap - th;
    let y = if crosses && above_y >= margin && above_y + th <= floor {
        above_y
    } else {
        below.clamp(margin, (wh - th - margin).max(margin))
    };
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(anchor: (f32, f32, f32, f32), tip: (f32, f32)) -> TooltipInput {
        TooltipInput {
            anchor,
            tip,
            window: (1000.0, 900.0),
            status_top: 820.0,
            margin: 8.0,
            gap: 4.0,
        }
    }

    /// The toolbar-anchor case: near the top, room below — placed
    /// below the anchor, horizontally centred.
    #[test]
    fn places_below_a_top_anchor() {
        let (x, y) = place(&input((100.0, 10.0, 32.0, 32.0), (120.0, 40.0)));
        assert_eq!(x, 56.0, "centred on the anchor");
        assert_eq!(y, 46.0, "anchor bottom + gap");
    }

    /// Flipped above when the below placement would cross the status
    /// bar and room exists above.
    #[test]
    fn flips_above_when_crossing_the_status_bar() {
        // Anchor just above the status bar: below would cross 820.
        let (x, y) = place(&input((100.0, 760.0, 32.0, 40.0), (120.0, 60.0)));
        assert_eq!(x, 56.0);
        assert_eq!(y, 760.0 - 4.0 - 60.0, "above the anchor, gap apart");
    }

    /// No room above: the below placement is clamped inside the
    /// window's vertical margins (its bottom may cross the status bar
    /// — nothing better exists — but never the window edge).
    #[test]
    fn clamps_when_no_room_above() {
        // Anchor at the very top: above has no room, so below wins
        // and clamps to the window's bottom margin.
        let (_, y) = place(&input((100.0, 0.0, 32.0, 32.0), (120.0, 800.0)));
        // The below placement (anchor bottom + gap) already fits the
        // window's vertical margins, so it is kept verbatim.
        assert_eq!(y, 36.0);
        // A tip too tall for the window saturates at the margin.
        let (_, y) = place(&input((100.0, 0.0, 32.0, 32.0), (120.0, 1200.0)));
        assert_eq!(y, 8.0, "saturated at the margin, never negative");
    }

    /// Horizontal clamping at the edges: an anchor at the left edge
    /// keeps the tip inside the margin.
    #[test]
    fn clamps_horizontally_at_the_edges() {
        let (x, _) = place(&input((0.0, 100.0, 32.0, 32.0), (200.0, 40.0)));
        assert_eq!(x, 8.0);
        let (x, _) = place(&input((968.0, 100.0, 32.0, 32.0), (200.0, 40.0)));
        assert_eq!(x, 1000.0 - 200.0 - 8.0);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    // The D10 invariant over adversarial inputs: whenever the tip can
    // fit the window (allowing the margins), the placed rect lies
    // inside the window rect; always, the placed top-left stays at or
    // past the margin on every axis (an unfittable tip saturates at
    // the margin — nothing can fit, and the point never goes negative
    // or off-window).
    proptest! {
        #[test]
        fn placed_rect_stays_inside_the_window(
            ax in 0.0f32..2000.0,
            ay in 0.0f32..2000.0,
            aw in 1.0f32..200.0,
            ah in 1.0f32..200.0,
            tw in 1.0f32..2400.0,
            th in 1.0f32..2400.0,
            ww in 100.0f32..2000.0,
            wh in 100.0f32..2000.0,
            status_h in 0.0f32..300.0,
        ) {
            let input = TooltipInput {
                anchor: (ax, ay, aw, ah),
                tip: (tw, th),
                window: (ww, wh),
                status_top: wh - status_h,
                margin: 8.0,
                gap: 4.0,
            };
            let (x, y) = place(&input);
            let fits_w = tw <= ww - 16.0;
            let fits_h = th <= wh - 16.0;
            prop_assert!(x >= 8.0 && x <= ww - 8.0, "x {x} in [8, {}]", ww - 8.0);
            prop_assert!(y >= 8.0 && y <= wh - 8.0, "y {y} in [8, {}]", wh - 8.0);
            // A half-ULP tolerance: the clamp target is `ww - tw - 8`,
            // and re-adding `tw` may round past it.
            let eps = 0.01f32;
            if fits_w {
                prop_assert!(x + tw <= ww - 8.0 + eps, "right edge inside");
            }
            if fits_h {
                prop_assert!(y + th <= wh - 8.0 + eps, "bottom edge inside");
            }
        }
    }
}
