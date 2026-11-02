// SPDX-License-Identifier: AGPL-3.0-only

//! The icon colorize probe suite (r4 chunk 3, plan §3): the R2 gate's
//! runtime evidence that `Image.colorize` tints a vendored monochrome
//! SVG, on the software renderer. Every cell must contain glyph
//! pixels — a blank cell means decode or colorize failed, and the
//! migration does not proceed. A committed probe golden
//! (`tests/goldens/icon-probe-1.0-396.png`) pins byte-stable
//! determinism and is the review artefact. Pixel assertions are
//! floors only ("glyph present", "full-tint cores exist at 3.0") —
//! never ceilings.

mod support;

use support::driver::{DriverOptions, SlintDriver};
use support::golden_compare::{self, Rgba8Image};

include!(concat!(env!("OUT_DIR"), "/icon_probe.rs"));

use slint::ComponentHandle as _;

// The probe's cell-grid constants — mirrored by `tests/icon_probe.slint`
// (keep the two in step).
const CELL: u32 = 44;
const PITCH: u32 = 48;
const ORIGIN: u32 = 8;
const WINDOW_WIDTH: u32 = 392;
const WINDOW_HEIGHT: u32 = 372;

// The probe's expected tints and backgrounds — the theme palette
// values `tests/icon_probe.slint` names (the tone map's outputs in
// `Tone` order, the hover-lift pair on both surfaces, and the sizes
// row's neutral tint).
const TINTS_ROW_A: [[u8; 3]; 8] = [
    [0xa6, 0xab, 0xbd], // neutral → text-secondary
    [0xe8, 0xea, 0xf0], // info → text-primary
    [0x6f, 0xd9, 0x8b], // success
    [0xe8, 0xc2, 0x68], // warning
    [0xe8, 0x75, 0x6f], // danger
    [0xff, 0x9e, 0x64], // overdue
    [0x7a, 0xa2, 0xf7], // sync-active
    [0xa6, 0xab, 0xbd], // offline → text-secondary
];
const BACKGROUND: [u8; 3] = [0x14, 0x16, 0x1c];
const SURFACE_RAISED: [u8; 3] = [0x26, 0x2a, 0x35];

fn run_probe(job: impl FnOnce(&IconProbeWindow, &Rgba8Image) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver: &mut SlintDriver| {
            let window = IconProbeWindow::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            let buffer = window
                .window()
                .take_snapshot()
                .expect("the software renderer captures the probe (renderer-presence contract)");
            let snapshot =
                Rgba8Image::new(buffer.width(), buffer.height(), buffer.as_bytes().to_vec());
            job(&window, &snapshot);
        },
    );
}

fn cell_rect(column: u32, row_y: u32) -> (u32, u32, u32, u32) {
    (ORIGIN + column * PITCH, row_y, CELL, CELL)
}

/// Glyph-presence floor: pixels in the cell strictly closer to the
/// tint than to the cell background (the antialiased stroke's
/// tint-weighted pixels). A decode/colorize failure renders zero.
fn glyph_pixels(image: &Rgba8Image, rect: (u32, u32, u32, u32), tint: [u8; 3], bg: [u8; 3]) -> u32 {
    let (cx, cy, cw, ch) = rect;
    let (x0, y0) = (cx.min(image.width), cy.min(image.height));
    let (x1, y1) = ((cx + cw).min(image.width), (cy + ch).min(image.height));
    let dist2 = |i: usize, c: [u8; 3]| -> u32 {
        let (dr, dg, db) = (
            i32::from(image.data[i]) - i32::from(c[0]),
            i32::from(image.data[i + 1]) - i32::from(c[1]),
            i32::from(image.data[i + 2]) - i32::from(c[2]),
        );
        #[allow(clippy::cast_sign_loss)] // squared channel deltas are non-negative
        ((dr * dr + dg * dg + db * db) as u32).min(u32::MAX / 4)
    };
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y as usize * image.width as usize + x as usize) * 4;
            if dist2(i, tint) < dist2(i, bg) {
                count += 1;
            }
        }
    }
    count
}

/// Full-tint cores: pixels exactly matching the tint — evidence the
/// stroke has full-coverage interior pixels at the cell's scale.
fn exact_tint_pixels(image: &Rgba8Image, rect: (u32, u32, u32, u32), tint: [u8; 3]) -> u32 {
    let (cx, cy, cw, ch) = rect;
    let (x0, y0) = (cx.min(image.width), cy.min(image.height));
    let (x1, y1) = ((cx + cw).min(image.width), (cy + ch).min(image.height));
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y as usize * image.width as usize + x as usize) * 4;
            if image.data[i] == tint[0]
                && image.data[i + 1] == tint[1]
                && image.data[i + 2] == tint[2]
            {
                count += 1;
            }
        }
    }
    count
}

fn probe_golden_path() -> std::path::PathBuf {
    std::path::PathBuf::from(format!(
        "{}/tests/goldens/icon-probe-1.0-396.png",
        env!("CARGO_MANIFEST_DIR")
    ))
}

/// P1 — the renderer-presence contract and the tone ladder: every
/// tone-map tint renders glyph pixels on `Theme.background`. A blank
/// cell means SVG decode or colorize failed — the migration gate.
#[test]
fn p1_every_tone_of_the_ladder_renders_glyph_pixels() {
    run_probe(|_window, snapshot| {
        assert_eq!(
            (snapshot.width, snapshot.height),
            (WINDOW_WIDTH, WINDOW_HEIGHT),
            "renderer-presence contract: the software renderer captures \
             at the exact window size"
        );
        for (column, tint) in TINTS_ROW_A.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)] // the ladder's column index
            let column = column as u32;
            let rect = cell_rect(column, 8);
            let count = glyph_pixels(snapshot, rect, *tint, BACKGROUND);
            assert!(
                count > 0,
                "tone column {column}: no glyph pixels of tint {tint:?} — \
                 decode or colorize failed"
            );
        }
    });
}

/// P2 — the neutral hover-lift pair reads on both surfaces: the
/// tone map's light/dark extremes carry glyph pixels on
/// `Theme.background` and on `Theme.surface-raised`.
#[test]
fn p2_the_hover_lift_pair_reads_on_both_surfaces() {
    run_probe(|_window, snapshot| {
        let tints = [[0xa6, 0xab, 0xbd], [0xe8, 0xea, 0xf0]];
        for (column, bg) in [BACKGROUND, SURFACE_RAISED].iter().enumerate() {
            for (offset, tint) in tints.iter().enumerate() {
                #[allow(clippy::cast_possible_truncation)] // the pair grid's cell index
                let rect = cell_rect((column * 2 + offset) as u32, 60);
                let count = glyph_pixels(snapshot, rect, *tint, *bg);
                assert!(
                    count > 0,
                    "tint {tint:?} on background {bg:?}: no glyph pixels"
                );
            }
        }
    });
}

/// Ink-presence floor: pixels visibly off the cell background (the
/// shared counter). At the 0.5× extreme the sub-pixel strokes blend
/// far toward the background — the cell still carries glyph ink, and
/// the count is a floor, never a ceiling.
fn ink_pixels(image: &Rgba8Image, rect: (u32, u32, u32, u32), bg: [u8; 3]) -> u32 {
    let (cx, cy, cw, ch) = rect;
    support::pixel_probe::count_off_background(image, cx, cy, cx + cw, cy + ch, bg)
}

/// P3 — the crispness extremes: glyphs render ink at 0.5× and 1.0×
/// the icon edge (the kiosk panel's sub-pixel strokes still leave
/// ink), and the 3.0× cell has full-tint stroke cores — the
/// rasterizer's crispness evidence at the scale extreme.
#[test]
fn p3_glyphs_render_at_the_crispness_extremes() {
    run_probe(|_window, snapshot| {
        let tint = TINTS_ROW_A[0];
        for column in 0..2 {
            let rect = cell_rect(column, 112);
            assert!(
                ink_pixels(snapshot, rect, BACKGROUND) > 0,
                "size column {column}: no glyph ink at the small extreme"
            );
        }
        let large = cell_rect(2, 112);
        let cores = exact_tint_pixels(snapshot, large, tint);
        assert!(
            cores > 0,
            "the 3.0× cell has no full-tint pixels — the stroke never \
             reaches full coverage"
        );
    });
}

/// P4 — the inlined post-swap button box: the padded 32 px square
/// carries the glyph in its interior — the geometry the migration
/// lands (plan §2.3).
#[test]
fn p4_the_button_box_interior_carries_the_glyph() {
    run_probe(|_window, snapshot| {
        // The button box sits at (8, 172), 32 px; the padded interior
        // is the icon's 12 px square at its centre.
        let rect = (8, 172, 32, 32);
        let count = glyph_pixels(snapshot, rect, TINTS_ROW_A[0], BACKGROUND);
        assert!(count > 0, "the inlined button box renders no glyph pixels");
    });
}

/// P6 — the kinds wall: every vendored asset decodes and colorizes —
/// one cell per file (the plan §1 mapping-table order, 8 cells per
/// row), each carrying glyph pixels. A blank cell means that asset
/// fails to decode or tint.
#[test]
fn p6_every_vendored_asset_renders_glyph_pixels() {
    run_probe(|_window, snapshot| {
        let wall_rows: [u32; 3] = [224, 272, 320];
        let counts_per_row = [8usize, 8, 3];
        let mut index = 0usize;
        for (row, y) in wall_rows.iter().enumerate() {
            for column in 0..counts_per_row[row] {
                #[allow(clippy::cast_possible_truncation)] // the wall grid
                let rect = cell_rect(column as u32, *y);
                let count = ink_pixels(snapshot, rect, BACKGROUND);
                assert!(
                    count > 0,
                    "kind-wall cell {index} (row {row}, column {column}): \
                     no glyph ink — the asset fails to decode or tint"
                );
                index += 1;
            }
        }
        assert_eq!(index, 19, "the wall covers the whole vendored set");
    });
}

/// P5 — the committed probe golden: byte-stable determinism of the
/// whole probe composition, and the review artefact. Regeneration is
/// the explicit failing step (the goldens' protocol).
#[test]
fn p5_the_probe_golden_holds() {
    let regen = std::env::var_os("TASKBOARD_REGEN_GOLDENS").is_some_and(|v| v == "1");
    run_probe(move |_window, snapshot| {
        let path = probe_golden_path();
        if regen {
            let bytes = golden_compare::encode_png(snapshot).expect("encodes");
            std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
            std::fs::write(&path, bytes).expect("writes");
            return;
        }
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|error| panic!("golden {} readable: {error}", path.display()));
        let golden: Rgba8Image =
            golden_compare::decode_png(&bytes).expect("the committed golden decodes as RGBA8");
        golden_compare::matches(&golden, snapshot)
            .unwrap_or_else(|error| panic!("golden {} drifted: {error}", path.display()));
    });
    assert!(
        !regen,
        "probe golden regenerated — review the PNG diff before landing"
    );
}
