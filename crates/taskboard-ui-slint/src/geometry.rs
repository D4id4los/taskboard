// SPDX-License-Identifier: AGPL-3.0-only

//! The device-local main-window geometry (#412): a typed rectangle in
//! physical pixels with a fallible mint ([`WindowGeometry::new`]) at
//! every trust boundary — the config read on restore, the window read
//! on capture — and a pure visibility clamp ([`WindowGeometry::
//! clamp_to`]) for the restore path.
//!
//! **Leniency contract** (the `DeckColor` "strict writes, lenient
//! reads" shape): a degenerate *stored* value (zero/negative/oversized
//! dimensions, out-of-range positions) mints to `None` and the restore
//! hand-over carries `None` — today's default placement. A hostile
//! geometry never blocks UI start (Q15); the value roams nowhere near
//! the domain, the engine state, the storage sidecar, or sync — it is
//! device-local by design and lives in this crate and the app crate's
//! raw config table only.
//!
//! **Platform gap** (honest): on Wayland `set_position` is a
//! compositor no-op and `position()` may report a placeholder — there
//! the restore degrades to compositor-decided placement (the size
//! still restores) and the capture may persist a placeholder position
//! that a later restore also ignores. Benign by construction: the mint
//! and the clamp keep garbage out of the crash class.

/// The persisted main-window geometry in physical pixels: the
/// top-left corner (`x`, `y` — possibly negative: left-of/above the
/// primary monitor is legitimate desktop geography) and the size
/// (`width`, `height` — at least 1×1).
///
/// Private fields (the A1 pattern): the only constructor is the
/// fallible [`WindowGeometry::new`], so everything downstream of a
/// mint carries minted values — no validate-and-forget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

/// The restore clamp's visibility margin: a restore is left untouched
/// when at least this many pixels of the window are visible on the
/// current monitor in both axes; otherwise the position is pulled to
/// the nearest legal spot. Enough to grab and drag the window back.
pub const VISIBILITY_MARGIN_PX: i32 = 80;

impl WindowGeometry {
    /// Mints a geometry from raw physical-pixel values, `None` on
    /// refusal. One constructor serves both trust boundaries (the
    /// config-load mint on restore, the window-read mint on capture):
    ///
    /// - `width < 1` or `height < 1` refuse — a degenerate window is
    ///   not a placement;
    /// - any dimension above `i32::MAX` refuses — documented leniency
    ///   ceiling that keeps the clamp arithmetic in checked-i32 space;
    /// - a position outside `i32` refuses — physical coordinates are
    ///   `i32` on every supported platform, so a wider raw value is a
    ///   corrupted store, not history.
    ///
    /// Off-screen positions deliberately mint (they are legitimate
    /// history — a monitor unplug between sessions); the restore-time
    /// [`WindowGeometry::clamp_to`] handles them where the current
    /// monitor is actually knowable. No clamping happens here.
    #[must_use]
    pub fn new(x: i64, y: i64, width: i64, height: i64) -> Option<Self> {
        if width < 1 || height < 1 {
            return None;
        }
        if width > i64::from(i32::MAX) || height > i64::from(i32::MAX) {
            return None;
        }
        let Ok(x) = i32::try_from(x) else {
            return None;
        };
        let Ok(y) = i32::try_from(y) else {
            return None;
        };
        // The range checks above guarantee the conversions; `try_from`
        // keeps them total without a panicking cast.
        let width = u32::try_from(width).ok()?;
        let height = u32::try_from(height).ok()?;
        Some(Self {
            x,
            y,
            width,
            height,
        })
    }

    /// The stored top-left corner's x, in physical pixels.
    #[must_use]
    pub fn x(self) -> i32 {
        self.x
    }

    /// The stored top-left corner's y, in physical pixels.
    #[must_use]
    pub fn y(self) -> i32 {
        self.y
    }

    /// The stored width, in physical pixels.
    #[must_use]
    pub fn width(self) -> u32 {
        self.width
    }

    /// The stored height, in physical pixels.
    #[must_use]
    pub fn height(self) -> u32 {
        self.height
    }

    /// The restore-time clamp: guarantees a [`VISIBILITY_MARGIN_PX`]
    /// strip of the window on `screen` (a minted rectangle covering
    /// the current monitor) when the screen can legally provide one,
    /// else the best available placement — while **never altering the
    /// size** (a window wider than the monitor legitimately spanned
    /// two monitors; shrinking it would lose user state, and the WM
    /// enforces the D15 min floors anyway).
    ///
    /// Fully or partially visible (≥ the margin in both axes) ⇒
    /// unchanged — the user's placement is never fought. Otherwise the
    /// position is pulled to the nearest legal spot (saturating `i64`
    /// arithmetic throughout; `i32`-extreme inputs saturate, never
    /// panic). A degenerate screen (`width` or `height` ≤ 0) ⇒
    /// identity — there is nothing to clamp against.
    #[must_use]
    pub fn clamp_to(self, screen: WindowGeometry) -> Self {
        let sw = i64::from(screen.width);
        let sh = i64::from(screen.height);
        if sw <= 0 || sh <= 0 {
            return self;
        }
        let margin = i64::from(VISIBILITY_MARGIN_PX);
        let x = i64::from(self.x);
        let y = i64::from(self.y);
        let ww = i64::from(self.width);
        let wh = i64::from(self.height);
        let sx = i64::from(screen.x);
        let sy = i64::from(screen.y);

        let overlap_x = (x + ww).min(sx + sw) - x.max(sx);
        let overlap_y = (y + wh).min(sy + sh) - y.max(sy);
        if overlap_x >= margin && overlap_y >= margin {
            return self;
        }
        // The nearest spot with a ≥ margin strip visible. An empty
        // interval (the window cannot show the margin on this screen
        // at all) falls back to left/top alignment into the screen —
        // the maximum-overlap placement.
        let clamp_axis = |pos: i64, span: i64, spos: i64, sspan: i64| {
            let lower = spos + margin - span;
            let upper = spos + sspan - margin;
            if lower <= upper {
                pos.clamp(lower, upper)
            } else {
                spos
            }
        };
        let new_x = clamp_axis(x, ww, sx, sw);
        let new_y = clamp_axis(y, wh, sy, sh);
        Self {
            x: saturate_i32(new_x),
            y: saturate_i32(new_y),
            width: self.width,
            height: self.height,
        }
    }
}

/// Saturating `i64` → `i32` for the clamp's result position: extreme
/// inputs clamp to the range's edge instead of panicking or wrapping.
fn saturate_i32(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(if value < 0 { i32::MIN } else { i32::MAX })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: WindowGeometry = WindowGeometry {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };

    fn rect(x: i64, y: i64, w: i64, h: i64) -> Option<WindowGeometry> {
        WindowGeometry::new(x, y, w, h)
    }

    // -- T1: the mint --------------------------------------------------

    /// Sane rects mint exactly — negative positions and a (0,0) origin
    /// are legitimate desktop geography, and the mint clamps nothing.
    #[test]
    fn the_mint_accepts_sane_rects_verbatim() {
        let g = rect(-1920, -500, 1366, 768).expect("negative positions are legal");
        assert_eq!(
            (g.x(), g.y(), g.width(), g.height()),
            (-1920, -500, 1366, 768)
        );
        let origin = rect(0, 0, 1, 1).expect("a 1×1 window is legal");
        assert_eq!(
            (origin.x(), origin.y(), origin.width(), origin.height()),
            (0, 0, 1, 1)
        );
    }

    /// Degenerate dimensions and out-of-range values refuse: the
    /// lenient-read contract's raw material is `None`, never a
    /// silently "fixed" rect.
    #[test]
    fn the_mint_refuses_degenerate_and_oversized_values() {
        assert_eq!(rect(0, 0, 0, 100), None, "width 0");
        assert_eq!(rect(0, 0, 100, 0), None, "height 0");
        assert_eq!(rect(0, 0, -5, 100), None, "negative width");
        assert_eq!(rect(0, 0, 100, -5), None, "negative height");
        assert_eq!(rect(0, 0, 1 << 40, 100), None, "absurd width");
        assert_eq!(rect(0, 0, 100, 1 << 40), None, "absurd height");
        assert_eq!(
            rect(i64::from(i32::MAX) + 1, 0, 100, 100),
            None,
            "x overflow"
        );
        assert_eq!(
            rect(0, i64::from(i32::MIN) - 1, 100, 100),
            None,
            "y underflow"
        );
        // The documented ceilings themselves mint: i32-range positions
        // and i32::MAX dimensions are in range.
        assert!(
            rect(
                i64::from(i32::MAX),
                i64::from(i32::MIN),
                i64::from(i32::MAX),
                100
            )
            .is_some()
        );
    }

    // -- T2: the clamp --------------------------------------------------

    /// Fully on-screen ⇒ identity; a partial visibility of at least
    /// the margin in both axes is user geography and never "fixed".
    #[test]
    fn the_clamp_keeps_visible_placements_untouched() {
        let fully = rect(100, 100, 800, 600).expect("mint");
        assert_eq!(fully.clamp_to(SCREEN), fully);
        // 10 px on the left, well over the margin vertically: partial
        // visibility is the user's span-across-monitors history.
        let partial = rect(-10, 100, 800, 600).expect("mint");
        assert_eq!(partial.clamp_to(SCREEN), partial);
    }

    /// A fully off-screen rect (any side) is pulled to the nearest
    /// legal spot — exactly the margin strip visible on the screen.
    #[test]
    fn the_clamp_pulls_fully_offscreen_rects_to_the_margin_strip() {
        let m = VISIBILITY_MARGIN_PX;
        // Off the right edge: the nearest legal x is the interval's
        // upper end (sx + sw - margin); the window's right edge then
        // reaches past the screen, with exactly the margin overlapping.
        let right = rect(4000, 100, 800, 600).expect("mint");
        let fixed = right.clamp_to(SCREEN);
        assert_eq!(fixed.x(), 1920 - m);
        let overlap = i64::min(
            i64::from(fixed.x()) + i64::from(fixed.width()),
            i64::from(SCREEN.width()),
        ) - i64::from(fixed.x()).max(0);
        assert_eq!(overlap, i64::from(m), "exactly the margin strip is visible");
        assert_eq!(fixed.y(), 100, "the untouched axis stays untouched");
        // Off the bottom: the nearest legal y is sy + sh - margin.
        let bottom = rect(100, 5000, 800, 600).expect("mint");
        assert_eq!(bottom.clamp_to(SCREEN).y(), 1080 - m);
        // Off the left: the nearest legal x is the interval's lower
        // end (sx + margin - width), negative into the off-screen
        // region so exactly the margin strip shows.
        let left = rect(-4000, 100, 800, 600).expect("mint");
        assert_eq!(left.clamp_to(SCREEN).x(), m - 800);
        // Off the top.
        let top = rect(100, -4000, 800, 600).expect("mint");
        assert_eq!(top.clamp_to(SCREEN).y(), m - 600);
    }

    /// A sub-margin overlap (10 px) is not enough and is pulled in;
    /// exactly the margin overlap is enough and stays.
    #[test]
    fn the_clamp_boundary_is_exactly_the_margin() {
        let sub = rect(-790, 100, 800, 600).expect("mint"); // 10 px visible
        let fixed = sub.clamp_to(SCREEN);
        assert_ne!(fixed, sub, "a 10 px strip is pulled in");
        let edge = rect(i64::from(i32::MAX) - 100, 0, 100, 100).expect("mint");
        let _ = edge.clamp_to(SCREEN); // saturates, never panics
        let exact = rect(-720, 100, 800, 600).expect("mint"); // exactly 80 px visible
        assert_eq!(exact.clamp_to(SCREEN), exact);
    }

    /// A window larger than the screen clamps its position so a
    /// margin strip is visible and its size stays verbatim.
    #[test]
    fn the_clamp_never_alters_the_size() {
        let huge = rect(100, 100, 4000, 3000).expect("mint");
        let fixed = huge.clamp_to(SCREEN);
        assert_eq!(
            (fixed.width(), fixed.height()),
            (4000, 3000),
            "size untouched"
        );
        assert!(
            i64::from(fixed.x()) + i64::from(fixed.width()) >= 1920,
            "the huge window still spans past the screen edge"
        );
        // But a fully-off-screen huge window is pulled to the strip:
        // the position lands on the interval's upper end and exactly
        // the margin overlaps the screen on both axes.
        let off = rect(10000, 10000, 4000, 3000).expect("mint");
        let fixed = off.clamp_to(SCREEN);
        let m = VISIBILITY_MARGIN_PX;
        assert_eq!(fixed.x(), 1920 - m);
        assert_eq!(fixed.y(), 1080 - m);
        assert_eq!(
            1920 - i64::from(fixed.x()),
            i64::from(m),
            "exactly the margin strip is visible horizontally"
        );
    }

    /// A degenerate (0×0) screen clamps to identity, and `i32::MAX`-
    /// scale inputs saturate instead of panicking.
    #[test]
    fn the_clamp_is_identity_on_a_degenerate_screen_and_saturates_at_the_extremes() {
        let degenerate = rect(100, 100, 800, 600).expect("mint");
        let zero = WindowGeometry {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        assert_eq!(degenerate.clamp_to(zero), degenerate);

        let extreme = rect(i64::from(i32::MAX), i64::from(i32::MAX), 100, 100).expect("mint");
        let screen = rect(0, 0, 1920, 1080).expect("mint");
        let fixed = extreme.clamp_to(screen);
        assert_eq!((fixed.width(), fixed.height()), (100, 100));
        assert!(
            i64::from(fixed.x()) + i64::from(fixed.width())
                >= 1920 - i64::from(VISIBILITY_MARGIN_PX),
            "even the extreme saturates into the margin strip"
        );
    }
}
