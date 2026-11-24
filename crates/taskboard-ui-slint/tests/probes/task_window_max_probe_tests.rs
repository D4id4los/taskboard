// SPDX-License-Identifier: AGPL-3.0-only

//! The window-max-width regression suite: the real `TaskView`, a
//! rule-less open subject (the add-repeat affordance's icon-square
//! row live), mounted on the shared testing backend. The view's root
//! merge must never bound the window's max width — the pre-fix
//! collapse clamped the OS window to a row's content width once the
//! refresh settled (2026-11-14 user report).

use crate::support;

use support::driver::{DriverOptions, advance_probe_clock};

use slint::ComponentHandle as _;

include!(concat!(env!("OUT_DIR"), "/task_window_max_probe.rs"));

fn run_probe(job: impl FnOnce(&WindowMaxProbe) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = WindowMaxProbe::new().expect("the probe window instantiates");
            window.set_task(TaskDetailModel {
                title: "Suppe fertigkochen".into(),
                stack_title: "To Do".into(),
                due_phrase: "in 9h · 23:45".into(),
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
            job(&window);
        },
    );
}

/// The invariant: after the view mounts and the runtime's
/// materialization pass has walked the tree, the probe root's
/// horizontal layout max stays effectively unbounded. The pre-fix
/// collapse reported a row-content-derived few hundred px here, which
/// Slint's winit backend then applied as the OS window's max size.
#[test]
fn the_task_view_never_bounds_the_window_max_width() {
    run_probe(|window| {
        advance_probe_clock(0);
        let win = window.window();
        let inner = i_slint_core::window::WindowInner::from_pub(win);
        let rc = inner.component();
        // The field collapse rode the event-loop materialization
        // pass — the probe host drives it explicitly (the tree walk's
        // precondition).
        i_slint_core::item_tree::ensure_item_tree_instantiated(&rc);
        let comp = i_slint_core::item_tree::ItemTreeRc::borrow_pin(&rc);
        let h = comp
            .as_ref()
            .layout_info(i_slint_core::layout::Orientation::Horizontal);
        // The severed cap (Theme.layout-severing-max-width at scale 1)
        // is the floor the fix reads back; the collapse reported far
        // below it.
        assert!(
            h.max >= 99_999.0,
            "the task view's layout must not bound the window's max width (h.max = {})",
            h.max
        );
    });
}
