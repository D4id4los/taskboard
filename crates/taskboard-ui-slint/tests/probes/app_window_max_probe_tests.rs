// SPDX-License-Identifier: AGPL-3.0-only

//! The app window's max-width severing suite (#1378 — the 1276151a
//! per-view twin pattern): the real `AppWindow` on the shared testing
//! backend. The window root's merge must never bound the window's max
//! size — the sever cap at the root
//! (`Theme.layout-severing-max-width`) is what keeps a bounded-max
//! row from collapsing the OS window's max width once mounted.
//! `AppWindow` hosts every severed `ui/views/` root, so its own
//! Window merge is the last unguarded merge point of exactly the
//! class #778/#1197/#1251 collapsed through; the census boundary
//! (`ui/views/` only) cannot see it, and this probe is the witness.

use crate::support;

use slint::ComponentHandle as _;

include!(concat!(env!("OUT_DIR"), "/app_window_max_probe.rs"));

fn run_probe(job: impl FnOnce(&AppWindowMaxProbe) + Send + 'static) {
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = AppWindowMaxProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

/// The invariant (the 1276151a twin): after the window mounts and the
/// runtime's materialization pass has walked the tree, the root's
/// horizontal layout max stays effectively unbounded. The pre-fix
/// shape (no sever at the Window root) lets any future bounded-max
/// row/section reach the merge here.
#[test]
fn app_window_max_probe_severs_the_layout() {
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
        // is the floor the fix reads back.
        assert!(
            h.max >= 99_999.0,
            "the app window's layout must not bound the window's max width (h.max = {})",
            h.max
        );
    });
}
