// SPDX-License-Identifier: AGPL-3.0-only

//! The breakpoint mapping (Q16, design §6.3): window-logical-width
//! bands whose thresholds are constants *not* scaled by
//! `Theme.base-scale` — scale and responsiveness compose instead of
//! fighting. Rust owns the mapping (D17); the slint side only reads
//! the pushed [`crate::Breakpoint`] through the `Responsive` global.
//!
//! Pure: no slint types, no IO (the extraction seam — a second
//! consumer lifts this module unchanged).

/// The unscaled width thresholds (logical px). Tunable constants, not
/// config (Q22 fixes the v0.1.0 `[ui]` surface at scale + keymap).
pub const COMPACT_MAX_WIDTH: f32 = 720.0;
pub const EXPANSIVE_MIN_WIDTH: f32 = 1440.0;

/// The breakpoint vocabulary (Q16): `Compact` < [`COMPACT_MAX_WIDTH`],
/// `Regular` up to [`EXPANSIVE_MIN_WIDTH`], `Expansive` beyond. No
/// `Default`: the "no measurement yet" fallback is `Regular` (see
/// [`breakpoint_for_width`]) and lives in the mapping, not in a trait
/// impl that could drift from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breakpoint {
    Compact,
    Regular,
    Expansive,
}

/// Maps a window's logical width onto its breakpoint (D17). Widths
/// `<= 0` are treated as "no measurement yet" and fall back to
/// [`Breakpoint::Regular`] — the null test platform reports 0×0
/// windows, and the initial push happens before real layout.
#[must_use]
pub fn breakpoint_for_width(logical_px: f32) -> Breakpoint {
    if logical_px <= 0.0 {
        return Breakpoint::Regular;
    }
    if logical_px < COMPACT_MAX_WIDTH {
        Breakpoint::Compact
    } else if logical_px < EXPANSIVE_MIN_WIDTH {
        Breakpoint::Regular
    } else {
        Breakpoint::Expansive
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn exact_boundaries_map_per_q16() {
        assert_eq!(
            breakpoint_for_width(0.0),
            Breakpoint::Regular,
            "no measurement"
        );
        assert_eq!(breakpoint_for_width(-1.0), Breakpoint::Regular);
        assert_eq!(breakpoint_for_width(719.5), Breakpoint::Compact);
        assert_eq!(breakpoint_for_width(720.0), Breakpoint::Regular);
        assert_eq!(breakpoint_for_width(1439.5), Breakpoint::Regular);
        assert_eq!(breakpoint_for_width(1440.0), Breakpoint::Expansive);
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(512))]

        #[test]
        fn mapping_is_monotonic_in_width(
            // Real widths only: the `<= 0` "no measurement" fallback is
            // a discontinuity by design and unit-tested separately.
            a in 0.0001f32..3000.0,
            b in 0.0001f32..3000.0,
        ) {
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            let rank = |b: Breakpoint| match b {
                Breakpoint::Compact => 0,
                Breakpoint::Regular => 1,
                Breakpoint::Expansive => 2,
            };
            prop_assert!(rank(breakpoint_for_width(low)) <= rank(breakpoint_for_width(high)));
        }

        #[test]
        fn mapping_is_total_and_bounded(w in prop::num::f32::ANY) {
            let _ = breakpoint_for_width(w); // no panic on any float
        }
    }
}
