// SPDX-License-Identifier: AGPL-3.0-only

//! The detail-editor inset suite (round 8 L1, the E-editor/E-recur
//! C1 class): the real `TaskView` on the shared testing backend. The
//! severing inset: the shared editor/track composition's rows never
//! paint flush to the canvas edges — an editor row's window rect
//! stays strictly inside the viewport on both sides. Typed geometry
//! assertions over the element-geometry seam; no pixel/text oracles.
//! The pre-fix full-bleed painted the row surface at x=0 and
//! x=window-width (the analyzer's 17/25/31 px flush runs across the
//! due-editor, focus-cell, track-entries, labels-overlay and
//! recur-editor goldens).

use crate::support;

use support::driver::{DriverOptions, advance_probe_clock};

use slint::ComponentHandle as _;

include!(concat!(env!("OUT_DIR"), "/editor_inset_probe.rs"));

fn run_probe(job: impl FnOnce(&EditorInsetProbe) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = EditorInsetProbe::new().expect("the probe window instantiates");
            window.set_task(TaskDetailModel {
                title: "a very long ascii task title that overflows the card column width".into(),
                stack_title: "To Do".into(),
                due_phrase: "in 12h".into(),
                due_kind: DueKind::Soon,
                scheduled_phrase: "—".into(),
                tracked_total: "0s".into(),
                can_done: true,
                can_track_start: true,
                series_edit_visible: true,
                series_edit_enabled: true,
                ..Default::default()
            });
            window.show().expect("show on the testing platform");
            window
                .window()
                .set_size(slint::LogicalSize::new(1000.0, 900.0));
            advance_probe_clock(0);
            job(&window);
        },
    );
}

/// The effort row (the shared composition's always-on editor row)
/// sits strictly inside the viewport on both sides.
#[test]
fn the_editor_rows_stay_off_the_canvas_edges() {
    run_probe(|window| {
        let width = window
            .window()
            .size()
            .to_logical(window.window().scale_factor())
            .width;
        let elements = support::driver::find_all_by_id_on(window, "TaskView::effort-row");
        assert_eq!(elements.len(), 1, "the effort row mounts exactly once");
        let el = &elements[0];
        let pos = el.absolute_position();
        let size = el.size();
        assert!(
            pos.x > 0.0,
            "the editor row's left edge must sit off the canvas (x = {})",
            pos.x
        );
        assert!(
            pos.x + size.width < width,
            "the editor row's right edge must sit off the canvas (x + w = {} vs {width})",
            pos.x + size.width
        );
    });
}
