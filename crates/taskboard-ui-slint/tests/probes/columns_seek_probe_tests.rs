// SPDX-License-Identifier: AGPL-3.0-only

//! The board columns' focus-into-view seek suite (#1365, D-1365b):
//! the real `BoardLayout` on the shared testing backend. The #1302
//! law's horizontal half: a focus landing in a clipped column (the
//! overflow regime — resolved widths summing past the row) moves the
//! columns row's `content-x` so the landed column is inside the
//! viewport, and a landing in the fitting regime moves nothing (no
//! dead scroll range). Typed length assertions only — no pixel, text
//! or image oracles. The adapter-side landing resolution is already
//! pinned (`journeys.rs`, the board hint arms); this suite is the
//! consumption half — one behavior, one test, lowest layer per
//! surface.

use crate::support;

use slint::ComponentHandle as _;

include!(concat!(env!("OUT_DIR"), "/columns_seek_probe.rs"));

/// A column card model at `width` resolved px, no rows (the seek is
/// column-granular; the cards' own vertical consumption is theirs).
fn column(title: &str, width: f32) -> StackColumnModel {
    StackColumnModel {
        title: title.into(),
        width_px: width,
        ..Default::default()
    }
}

/// The columns layout's window x (the Flickable translation's
/// witness): the element query over the probe's debug-info tree.
fn columns_layout_window_x(window: &ColumnsSeekProbe) -> f32 {
    let elements = support::driver::find_all_by_id_on(window, "BoardLayout::columns");
    assert_eq!(elements.len(), 1, "the columns layout mounts exactly once");
    let pos = elements[0].window_pos();
    pos.x
}

fn run_probe(widths: &'static [f32], job: impl FnOnce(&ColumnsSeekProbe) + Send + 'static) {
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = ColumnsSeekProbe::new().expect("the probe window instantiates");
            let model = std::rc::Rc::new(slint::VecModel::from(
                widths
                    .iter()
                    .enumerate()
                    .map(|(i, w)| column(&format!("col-{i}"), *w))
                    .collect::<Vec<_>>(),
            ));
            window.set_stacks(slint::ModelRc::from(model));
            window.show().expect("show on the testing platform");
            // The viewport the seek law reads: the 1000 × 900 window
            // (the journey window's frame; the probe's default winit
            // window is device-dependent).
            window
                .window()
                .set_size(slint::LogicalSize::new(1000.0, 900.0));
            job(&window);
        },
    );
}

/// The law's overflow leg: three columns at 500 px each (1500 summed
/// against the 1000 px row) — a landing targeting the last column
/// seeks the row left so the clipped column is inside the viewport.
#[test]
fn a_landing_in_a_clipped_column_seeks_the_row() {
    run_probe(&[500.0, 500.0, 500.0], |window| {
        support::driver::advance_probe_clock(0);
        let hint = window.global::<FocusScrollHint>();
        hint.set_target_a(2);
        hint.set_target_b(-1);
        hint.set_nonce(hint.get_nonce() + 1);
        support::driver::advance_probe_clock(0);

        // The read seam is element geometry: the columns layout's
        // window x translates by the Flickable's content-x (at rest it
        // sits at the row's left padding, a small positive offset).
        let x = columns_layout_window_x(window);
        assert!(
            x < 0.0,
            "the clipped column's landing must seek the row left \
             (columns window x = {x}, still at rest)"
        );
        // Structural clamp: no overshoot past the content's right edge
        // (the content-vs-viewport slack bounds the shift; the layout's
        // own padding rides inside the translated content).
        assert!(
            x >= -550.0,
            "the seek clamps to the content (columns window x = {x})"
        );
    });
}

/// The law's fitting leg: three columns at 300 px each (900 summed,
/// inside the 1000 px row) — a landing on any column moves nothing.
#[test]
fn a_landing_in_the_fitting_regime_rearms_nothing() {
    run_probe(&[300.0, 300.0, 300.0], |window| {
        support::driver::advance_probe_clock(0);
        let hint = window.global::<FocusScrollHint>();
        hint.set_target_a(2);
        hint.set_target_b(-1);
        hint.set_nonce(hint.get_nonce() + 1);
        support::driver::advance_probe_clock(0);

        let x = columns_layout_window_x(window);
        assert!(
            x == 0.0,
            "the fitting regime has no dead scroll range \
             (columns window x = {x}, still at rest)"
        );
    });
}
