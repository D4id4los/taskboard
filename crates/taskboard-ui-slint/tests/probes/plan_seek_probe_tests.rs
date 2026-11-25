// SPDX-License-Identifier: AGPL-3.0-only

//! The planner's focus-into-view seek suite (#1320, D-1320b): the
//! real `PlanNextDayView` with a many-row pool model on the shared
//! testing backend. The #1302 law (a keyboard focus landing is always
//! scrolled into view) must hold on the planner surface: a landing on
//! a pool row below the fold moves the page Flickable's `content-y`
//! (the view's `page-y` root seam) so the landed row is inside the
//! viewport, and a landing in the fitting regime leaves the offset at
//! rest (no dead scroll range). Typed length assertions only — no
//! pixel or text oracles.
//!
//! The hint is driven through the `FocusScrollHint` global directly
//! (the adapter-side landing resolution is pinned separately in
//! `journeys.rs::planner_walk_landings_arm_the_hint`; this suite is
//! the consumption half — one behavior, one test, lowest layer per
//! surface).

use crate::support;

use slint::ComponentHandle as _;

// The probe's OWN generated module carries the probe-side
// `FocusScrollHint` global type (the probe instantiates its own
// single-instance tree; the lib-crate type belongs to AppWindow
// instances and is deliberately NOT used here).
include!(concat!(env!("OUT_DIR"), "/plan_seek_probe.rs"));

/// Twelve pool rows, each ~2 cards tall in sum — far past the 900px
/// window's fold. Titles are index-addressable diagnostics only.
fn pool_rows(count: usize) -> slint::ModelRc<PlanRowModel> {
    let model = std::rc::Rc::new(slint::VecModel::from(
        (0..count)
            .map(|i| PlanRowModel {
                card: TaskCardModel {
                    title: format!("pool-{i}").into(),
                    ..Default::default()
                },
                time: "".into(),
                effort: "".into(),
                overdue: false,
                cursor: false,
            })
            .collect::<Vec<_>>(),
    ));
    slint::ModelRc::from(model)
}

fn box_rows() -> slint::ModelRc<PlanRowModel> {
    let model = std::rc::Rc::new(slint::VecModel::from(vec![PlanRowModel {
        card: TaskCardModel {
            title: "box-0".into(),
            ..Default::default()
        },
        time: "09:00".into(),
        effort: "".into(),
        overdue: false,
        cursor: false,
    }]));
    slint::ModelRc::from(model)
}

fn run_probe(job: impl FnOnce(&PlanSeekProbe) + Send + 'static) {
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = PlanSeekProbe::new().expect("the probe window instantiates");
            window.set_boxes(box_rows());
            window.set_pool(pool_rows(40));
            window.set_show_pool(true);
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

/// The law's overflow leg: a landing on the last pool row (index 39,
/// far below the 900px fold) seeks the page down — the row's viewport
/// offset lands inside the visible window.
#[test]
fn a_landing_below_the_fold_seeks_the_row_into_view() {
    run_probe(|window| {
        support::driver::advance_probe_clock(0);
        let hint = window.global::<FocusScrollHint>();
        hint.set_target_a(1);
        hint.set_target_b(39);
        hint.set_nonce(hint.get_nonce() + 1);
        support::driver::advance_probe_clock(0);

        let page_y = window.get_page_y();
        assert!(
            page_y < 0.0,
            "the below-the-fold landing must seek the page down \
             (page-y = {page_y}, still at rest)"
        );
    });
}

/// The law's fitting leg: a landing the viewport already shows moves
/// nothing — no dead scroll range in the fitting regime.
#[test]
fn a_landing_inside_the_viewport_rearms_nothing() {
    run_probe(|window| {
        support::driver::advance_probe_clock(0);
        let hint = window.global::<FocusScrollHint>();
        hint.set_target_a(0);
        hint.set_target_b(0);
        hint.set_nonce(hint.get_nonce() + 1);
        support::driver::advance_probe_clock(0);

        let page_y = window.get_page_y();
        assert!(
            page_y == 0.0,
            "the in-view landing must not scroll (page-y = {page_y})"
        );
    });
}
