// SPDX-License-Identifier: AGPL-3.0-only

//! The print layouts' shared geometry vocabulary (plan §3.2/§4): the
//! point and page-size types, the fold sheet's panel placements, and
//! (in later steps) the display list. All coordinates are points
//! (1 pt = 1/72″), page origin at the top-left, y pointing down — the
//! convention the step-1 spike pinned for krilla, so the renderer
//! passes coordinates through unflipped.

/// A point in page coordinates (top-left origin, y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pt {
    /// Horizontal, rightward from the page's left edge.
    pub x: f32,
    /// Vertical, downward from the page's top edge.
    pub y: f32,
}

impl Pt {
    /// The point at `(x, y)`.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A page size in points. Private fields and no runtime constructor:
/// the curated layout constants below are the only values, all
/// positive and finite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    w_pt: f32,
    h_pt: f32,
}

impl PageSize {
    /// The width in points.
    #[must_use]
    pub const fn w_pt(self) -> f32 {
        self.w_pt
    }

    /// The height in points.
    #[must_use]
    pub const fn h_pt(self) -> f32 {
        self.h_pt
    }
}

/// A4 portrait (210 × 297 mm) — the fold sheet's physical page and the
/// A4 layout's page.
pub const A4: PageSize = PageSize {
    w_pt: 595.276,
    h_pt: 841.890,
};

/// One of the fold sheet's four panels, named by its booklet-page
/// role (reading order of the folded booklet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldPanel {
    /// Booklet page 1 — the front cover. Top-left quadrant, upright.
    Cover,
    /// Booklet page 2. Top-right quadrant, upright.
    Two,
    /// Booklet page 3. Bottom-right quadrant, pre-rotated 180°.
    Three,
    /// Booklet page 4. Bottom-left quadrant, pre-rotated 180°.
    Four,
}

/// The panel→page placement map of report §3 (pinned cover-first): the
/// affine transform each panel's local coordinates undergo onto the A4
/// sheet. Panel content is laid out upright in panel-local coordinates
/// on `[0, w] × [0, h]`; the bottom row (booklet pages 3–4) is
/// pre-rotated 180° so it reads upright after the horizontal fold is
/// flipped over. Turned over, the sheet's bottom row reads mirrored
/// left-to-right (BR left of BL), so page 3 sits bottom-right and page
/// 4 bottom-left — the physical-fold amendment of the §3 table.
/// `Rot180 { tx, ty }` carries the translation (e, f) of the full
/// `[-1 0 0 -1 tx ty]` map.
#[must_use]
pub fn fold_panel_xf(panel: FoldPanel, sheet: PageSize) -> PanelXf {
    let w = sheet.w_pt() / 2.0;
    match panel {
        FoldPanel::Cover => PanelXf::Identity,
        FoldPanel::Two => PanelXf::Translated { dx: w, dy: 0.0 },
        FoldPanel::Three => PanelXf::Rot180 {
            tx: sheet.w_pt(),
            ty: sheet.h_pt(),
        },
        FoldPanel::Four => PanelXf::Rot180 {
            tx: w,
            ty: sheet.h_pt(),
        },
    }
}

/// The one affine placement a panel's display-list ops undergo — one
/// row of the pinned §4 table (`[a b c d e f]` in PDF order):
/// `Identity` = `[1 0 0 1 0 0]`, `Translated { dx, dy }` =
/// `[1 0 0 1 dx dy]`, `Rot180 { tx, ty }` = `[-1 0 0 -1 tx ty]`. The
/// renderer pushes each rotated (and translated) panel's ops with
/// krilla's `push_transform`/`pop` — it never rotates glyphs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanelXf {
    /// Placement as laid out (the cover panel).
    Identity,
    /// A pure translation (the top row's right panel).
    Translated {
        /// The x shift.
        dx: f32,
        /// The y shift.
        dy: f32,
    },
    /// A 180° rotation about the panel origin, then a translation:
    /// sheet = (tx − x, ty − y). Exactly krilla's
    /// `Transform::from_row(-1, 0, 0, -1, tx, ty)` — the bottom row's
    /// pre-rotation so it reads upright after the fold is flipped.
    Rot180 {
        /// The x of the sheet point local (0, 0) lands on.
        tx: f32,
        /// The y of the sheet point local (0, 0) lands on.
        ty: f32,
    },
}

impl PanelXf {
    /// Maps a panel-local point onto the sheet (the pure twin of the
    /// renderer's pushed transform — every layout property test walks
    /// this).
    #[cfg(test)]
    #[must_use]
    pub fn apply(&self, p: Pt) -> Pt {
        match self {
            Self::Identity => p,
            Self::Translated { dx, dy } => Pt::new(p.x + dx, p.y + dy),
            Self::Rot180 { tx, ty } => Pt::new(tx - p.x, ty - p.y),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const TOL: f32 = 0.001;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < TOL
    }

    /// Each panel's transform maps its local corners exactly onto its
    /// quadrant (the plan §4 check for p3 spelled out: local (0,0) →
    /// the quadrant's far corner, local (w,h) → its near corner).
    #[test]
    fn panel_xfs_map_the_corners_onto_their_quadrants() {
        let sheet = A4;
        let w = sheet.w_pt() / 2.0;
        let h = sheet.h_pt() / 2.0;

        let cases = [
            (FoldPanel::Cover, Pt::new(0.0, 0.0), Pt::new(0.0, 0.0)),
            (FoldPanel::Cover, Pt::new(w, h), Pt::new(w, h)),
            (FoldPanel::Two, Pt::new(0.0, 0.0), Pt::new(w, 0.0)),
            (FoldPanel::Two, Pt::new(w, h), Pt::new(sheet.w_pt(), h)),
            (
                FoldPanel::Three,
                Pt::new(0.0, 0.0),
                Pt::new(sheet.w_pt(), sheet.h_pt()),
            ),
            (FoldPanel::Three, Pt::new(w, h), Pt::new(w, h)),
            (FoldPanel::Four, Pt::new(0.0, 0.0), Pt::new(w, sheet.h_pt())),
            (FoldPanel::Four, Pt::new(w, h), Pt::new(0.0, h)),
        ];
        for (panel, local, expected) in cases {
            let got = fold_panel_xf(panel, sheet).apply(local);
            assert!(
                close(got.x, expected.x) && close(got.y, expected.y),
                "{panel:?} local {local:?} → {got:?}, expected {expected:?}"
            );
        }
    }

    /// The double-application law: rotating 180° twice about the same
    /// anchor is the identity (the inverse of `Rot180 { tx, ty }` is
    /// itself).
    #[test]
    fn rot180_applied_twice_is_the_identity() {
        let xf = PanelXf::Rot180 {
            tx: 297.638,
            ty: 841.890,
        };
        let p = Pt::new(123.4, 56.7);
        let twice = xf.apply(xf.apply(p));
        assert!(close(twice.x, p.x) && close(twice.y, p.y));
    }

    proptest! {
        /// Every panel-local point lands inside its quadrant, and the
        /// four quadrant images tile the sheet: no gap (each quadrant's
        /// image fills it — corner pairing above) and no overlap (the
        /// quadrants are disjoint by construction, and each point stays
        /// in its own).
        #[test]
        fn every_mapped_point_stays_inside_its_quadrant(
            x in 0.0..297.638f32,
            y in 0.0..420.945f32,
        ) {
            let sheet = A4;
            let w = sheet.w_pt() / 2.0;
            let h = sheet.h_pt() / 2.0;
            let local = Pt::new(x, y);
            let expected = [
                (FoldPanel::Cover, 0.0, w, 0.0, h),
                (FoldPanel::Two, w, sheet.w_pt(), 0.0, h),
                (FoldPanel::Three, w, sheet.w_pt(), h, sheet.h_pt()),
                (FoldPanel::Four, 0.0, w, h, sheet.h_pt()),
            ];
            for (panel, x0, x1, y0, y1) in expected {
                let got = fold_panel_xf(panel, sheet).apply(local);
                prop_assert!(got.x >= x0 - TOL && got.x <= x1 + TOL);
                prop_assert!(got.y >= y0 - TOL && got.y <= y1 + TOL);
            }
        }
    }
}
