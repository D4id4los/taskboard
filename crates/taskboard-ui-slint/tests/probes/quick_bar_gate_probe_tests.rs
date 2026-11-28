// SPDX-License-Identifier: AGPL-3.0-only

//! The quick-bar gate probe suite (#1464's gating law, the
//! action-menu half): over a quick-bar with one withheld middle
//! entry, the shown half renders as two adjacent buttons (no
//! reserved slot band), and the click on the second shown button
//! routes the MODEL index 2 (resolve-by-index over the model, not
//! over the shown children). Typed geometry + the callback seam; no
//! text oracles, no sleeps.

use crate::support;

use support::driver::{DriverOptions, SlintDriver, advance_probe_clock, find_all_by_id_on};

include!(concat!(env!("OUT_DIR"), "/quick_bar_gate_probe.rs"));

fn run_probe(job: impl FnOnce(&QuickBarGateProbe, &mut SlintDriver) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |driver: &mut SlintDriver| {
            let window = QuickBarGateProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            advance_probe_clock(0);
            job(&window, driver);
        },
    );
}

/// The shown half renders, the withheld entry mounts nothing: two
/// buttons resolve, and they sit adjacent — the gap between them is
/// one spacing gap, strictly smaller than a button (a reserved slot
/// would put the gap at button-width or more).
#[test]
fn the_withheld_entry_mounts_no_button_and_no_slot() {
    run_probe(|window, _driver| {
        let mut buttons = find_all_by_id_on(window, "ActionMenu::quick-bar-button");
        buttons.sort_by(|a, b| {
            a.absolute_position()
                .x
                .partial_cmp(&b.absolute_position().x)
                .expect("finite x")
        });
        assert_eq!(
            buttons.len(),
            2,
            "the shown half renders; the withheld entry mounts nothing"
        );
        for button in &buttons {
            let size = button.size();
            assert!(
                size.width > 0.0 && size.height > 0.0,
                "every shown button owns real pixels"
            );
        }
        let (first, second) = (&buttons[0], &buttons[1]);
        let gap = second.absolute_position().x - (first.absolute_position().x + first.size().width);
        assert!(
            gap >= 0.0 && gap < first.size().width,
            "the shown buttons sit adjacent (one spacing gap, no reserved slot: gap {gap} vs width {})",
            first.size().width
        );
    });
}

/// The withheld entry's index is preserved: the click on the SECOND
/// shown button routes the model index 2.
#[test]
fn the_shown_click_routes_by_model_index() {
    run_probe(|window, _driver| {
        assert_eq!(window.get_bar_invoked(), -1, "no fire at mount");
        let mut buttons = find_all_by_id_on(window, "ActionMenu::quick-bar-button");
        buttons.sort_by(|a, b| {
            a.absolute_position()
                .x
                .partial_cmp(&b.absolute_position().x)
                .expect("finite x")
        });
        let second_shown = buttons.last().expect("the second shown button mounts");
        let position = second_shown.absolute_position();
        let size = second_shown.size();
        // The shared pointer-click seam (`support::driver`): move,
        // press, release — one clock tick per phase.
        support::driver::pointer_click_at(
            window.window(),
            position.x + size.width / 2.0,
            position.y + size.height / 2.0,
        );
        assert_eq!(
            window.get_bar_invoked(),
            2,
            "the click routes the shown button's MODEL index"
        );
    });
}
