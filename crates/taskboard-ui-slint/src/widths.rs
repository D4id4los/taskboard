// SPDX-License-Identifier: AGPL-3.0-only

//! The stack-column width math (round-1 D17): pure, single-source,
//! proptestable — the `tooltip.rs` precedent. All values are rendered
//! slint pixels (winit-logical, `Theme.base-scale` included — the
//! space `window-w`, the wrapper width, and drag deltas already
//! share). Conversion to stored logical px happens only at the
//! persistence boundary; no slint types here (the §5.5 extraction
//! seam).

/// The lower bound of the design's 96–120 band: card content stays
/// readable and the count badge fits.
pub const STACK_WIDTH_MIN_PX: u32 = 96;

/// The keyboard seed for a never-resized column — near the typical
/// auto distribution.
pub const STACK_WIDTH_DEFAULT_PX: u32 = 280;

/// The fixed keyboard step (stack.wider / stack.narrower).
pub const STACK_WIDTH_STEP_PX: u32 = 32;

/// Saturating clamp of a requested rendered width into
/// `[min, max]` (`max < min` — a degenerate window — resolves to
/// `min`; the ceiling function never returns below the floor).
#[must_use]
pub fn clamp_width(requested: i64, min: u32, max: u32) -> u32 {
    let min = i64::from(min);
    let max = i64::from(max).max(min);
    u32::try_from(requested.clamp(min, max)).unwrap_or(u32::MAX)
}

/// The remaining-viewport share bound: the widest a dragged/stepped
/// column may get — the window minus the other columns' effective
/// minimums minus the row's fixed furniture, floored at
/// [`STACK_WIDTH_MIN_PX`] (a too-small window cannot starve the
/// column below the readable bound).
///
/// `others_min_total` is Σ over the *other* live columns of their
/// effective minimum (the user width if set, else
/// [`STACK_WIDTH_MIN_PX`]); `chrome_px` is the row's fixed furniture
/// (the inter-column spacing gaps plus the add-column affordance).
#[must_use]
pub fn width_ceiling(window_w: u32, others_min_total: u32, chrome_px: u32) -> u32 {
    let room = i64::from(window_w) - i64::from(others_min_total) - i64::from(chrome_px);
    u32::try_from(room).unwrap_or(0).max(STACK_WIDTH_MIN_PX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bounds and saturation at both ends (edge units over the
    /// proptest invariants).
    #[test]
    fn clamp_width_saturates_at_both_bounds() {
        assert_eq!(clamp_width(150, STACK_WIDTH_MIN_PX, 400), 150);
        assert_eq!(clamp_width(10, STACK_WIDTH_MIN_PX, 400), STACK_WIDTH_MIN_PX);
        assert_eq!(clamp_width(9_999, STACK_WIDTH_MIN_PX, 400), 400);
        assert_eq!(
            clamp_width(-50, STACK_WIDTH_MIN_PX, 400),
            STACK_WIDTH_MIN_PX
        );
    }

    /// A degenerate window (`max < min`) resolves to the floor: the
    /// readable bound holds even when nothing fits.
    #[test]
    fn clamp_width_treats_a_degenerate_range_as_the_floor() {
        assert_eq!(clamp_width(150, 300, 100), 300);
    }

    /// Subtracts the other columns' minimums and the furniture; the
    /// result never dips below the readable floor.
    #[test]
    fn width_ceiling_subtracts_others_and_chrome() {
        assert_eq!(
            width_ceiling(1000, 2 * STACK_WIDTH_MIN_PX, 80),
            1000 - 192 - 80
        );
        assert_eq!(
            width_ceiling(100, 2 * STACK_WIDTH_MIN_PX, 80),
            STACK_WIDTH_MIN_PX,
            "a starved window still floors at the readable bound"
        );
    }

    #[cfg(test)]
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// Monotone in the request and always inside `[min, max]`
            /// (the clamp is the load-bearing invariant — the drag path
            /// feeds it raw pointer deltas).
            #[test]
            fn clamp_width_is_monotone_and_bounded(
                a in -1000i64..5000,
                b in -1000i64..5000,
                min in 1u32..1000,
                max in 1u32..2000,
            ) {
                let (lo, hi) = (a.min(b), a.max(b));
                let (clamped_lo, clamped_hi) = (clamp_width(lo, min, max), clamp_width(hi, min, max));
                prop_assert!(clamped_lo <= clamped_hi, "monotone: {clamped_lo} <= {clamped_hi}");
                let bound_lo = min.min(max);
                let bound_hi = min.max(max);
                prop_assert!(clamped_lo >= bound_lo && clamped_hi <= bound_hi);
            }

            /// The ceiling is at least the floor whenever the window
            /// can fit one column and the furniture (garbage-input
            /// floors are fine).
            #[test]
            fn width_ceiling_never_drops_below_the_floor(
                window_w in 0u32..6000,
                others in 0u32..3000,
                chrome in 0u32..500,
            ) {
                prop_assume!(window_w >= STACK_WIDTH_MIN_PX + chrome);
                prop_assert_eq!(width_ceiling(window_w, others, chrome) >= STACK_WIDTH_MIN_PX, true);
            }
        }
    }
}
