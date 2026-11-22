// SPDX-License-Identifier: AGPL-3.0-only

//! The today window-max-width regression suite (#1029, the
//! plan-next-day twin): the real `TodayView` with a default model on
//! the shared testing backend. The view root's merge must never
//! bound the window's max width — a bounded-max row would collapse
//! the OS window's max size once mounted.

use crate::support;

use slint::ComponentHandle as _;

include!(concat!(env!("OUT_DIR"), "/today_window_max_probe.rs"));

fn run_probe(job: impl FnOnce(&WindowMaxProbe) + Send + 'static) {
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = WindowMaxProbe::new().expect("the probe window instantiates");
            window.set_today(TodayModel::default());
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

/// The invariant (the plan-next-day probe's twin): after the view
/// mounts and the runtime's materialization pass has walked the
/// tree, the probe root's horizontal layout max stays effectively
/// unbounded. The pre-fix collapse reported the content-derived
/// few hundred px here.
#[test]
fn the_today_view_never_bounds_the_window_max_width() {
    run_probe(|window| {
        support::driver::advance_probe_clock(0);
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
            "the today view's layout must not bound the window's max width (h.max = {})",
            h.max
        );
    });
}
