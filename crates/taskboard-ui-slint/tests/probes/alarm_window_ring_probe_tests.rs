// SPDX-License-Identifier: AGPL-3.0-only

//! The alarm window's selection-ring probe suite (W-4/D-6, #1308
//! US-4/S1): the ring is the selected row's own geometry — the
//! row-local `emphasis` border — never the window's. The structural
//! oracle is the window's `selected-row-width` mirror, written by the
//! selected row itself from its own width; the pre-redesign defect
//! (a window-sized ring Rectangle inside every row) could not produce
//! it. Structural relations only — no label text (AGENTS §5).

use crate::support;

use slint::ComponentHandle as _;
use slint::Model as _;

fn run_probe(job: impl FnOnce(&taskboard_ui_slint::AlarmWindow) + Send + 'static) {
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = taskboard_ui_slint::AlarmWindow::new()
                .expect("the alarm window instantiates on the testing backend");
            // The rows mount before the show so the content-sized
            // layout settles around the real model (the mirror's
            // `changed width` re-fires on the layout pass).
            job(&window);
            window.show().expect("show on the testing platform");
            support::driver::advance_probe_clock(0);
        },
    );
}

/// The ring's geometry source is the selected row's own width: the
/// mirror (written by the row itself) reports a positive width
/// strictly inside the window's canvas — the row-local relation.
/// The mount-time walk delivers the row's `init`/`changed width`
/// mirror writes (the tree-walk precondition).
#[test]
fn us4_s1_selection_ring_is_row_local() {
    run_probe(|window| {
        let model: Vec<taskboard_ui_slint::AlarmRowModel> = vec![
            taskboard_ui_slint::AlarmRowModel {
                title: "tea is ready".into(),
                phrase: "14:05 · overdue".into(),
            },
            taskboard_ui_slint::AlarmRowModel {
                title: "standup in five".into(),
                phrase: "in 4 min".into(),
            },
        ];
        window.set_rows(slint::ModelRc::new(slint::VecModel::from(model)));
        window.set_selected_row(1);
        // A controlled canvas: the testing backend does not apply the
        // content-sized window's preferred size reliably (the #1023
        // fresh-mount quirk family), so the probe pins the row-local
        // relation against an explicit 500 px logical canvas instead.
        window
            .window()
            .set_size(slint::LogicalSize::new(500.0, 200.0));
        support::driver::advance_probe_clock(0);
        support::driver::advance_probe_clock(0);

        let ring = window.get_selected_row_width();
        assert!(
            ring > 0.0,
            "the selected row's ring mirror is written by the row itself (got {ring})"
        );
        assert!(
            ring < 500.0,
            "the ring is row-local: its geometry source stays strictly inside the \
             window canvas (ring {ring} >= canvas 500)"
        );
        assert_eq!(
            window.get_rows().row_count(),
            2,
            "the probe's two rows mounted"
        );
    });
}
