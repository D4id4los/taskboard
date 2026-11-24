// SPDX-License-Identifier: AGPL-3.0-only

//! The alarm window's max-width severing suite (W-5, #1251 — the
//! 1276151a per-view twin): the real `AlarmWindow` with a default row
//! model on the shared testing backend. The window root's merge must
//! never bound the window's max size — the severing cap at the root
//! (`Theme.layout-severing-max-width`) is what keeps a bounded-max
//! row from collapsing the OS window's max width once mounted.
//!
//! Two-way evidence (stage-3 review, 2026-11-24): the probe FAILS
//! against the pre-redesign shape (a fixed `width:` at the window
//! root — the collapse mechanism #1251 reported) and PASSES with only
//! the sever cap removed — the W-1 redesign left no bounded-max row
//! (elide keeps the rows' layout max unbounded), so the cap is
//! defense-in-depth over a precondition that is currently structurally
//! absent. The probe pins the invariant at the read-back point so the
//! regression class re-entering via any future row geometry still
//! fails here.

use crate::support;

use slint::ComponentHandle as _;

include!(concat!(env!("OUT_DIR"), "/alarm_window_max_probe.rs"));

fn run_probe(job: impl FnOnce(&AlarmWindowMaxProbe) + Send + 'static) {
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = AlarmWindowMaxProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
}

/// The invariant (the 1276151a twin): after the window mounts and the
/// runtime's materialization pass has walked the tree, the root's
/// horizontal layout max stays effectively unbounded. The pre-fix
/// collapse reported the content-derived few hundred px here.
#[test]
fn alarm_window_max_probe_severs_the_layout() {
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
            "the alarm window's layout must not bound the window's max width (h.max = {})",
            h.max
        );
    });
}
