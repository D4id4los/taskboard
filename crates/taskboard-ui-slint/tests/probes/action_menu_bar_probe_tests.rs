// SPDX-License-Identifier: AGPL-3.0-only

//! The action-menu bar probe's tier-2 suite (J5 US-1/US-3, plan §4):
//! the real `ActionMenu` composition's quick-bar contract — the bar
//! row mounts structurally over a populated model (bar row + per-entry
//! buttons owning real pixels, the zero-height squeeze guard), the
//! barless model mounts nothing (the other consumers' bit-identical
//! invariant), the bar's click routes by index, and the submenu
//! chevron renders iff the row is a submenu parent (US-3 AC-1, one
//! component all menus inherit). Deterministic: one platform turn per
//! dispatch, no sleeps, no text assertions. The #1309 pins add the
//! occlusion halves: the bar's band disjoint from the scrolled row
//! list's, and a scrolled click routing the bar button.

use crate::support;

use support::driver::{DriverOptions, SlintDriver, find_all_by_id_on, find_by_label_on};

include!(concat!(env!("OUT_DIR"), "/action_menu_bar_probe.rs"));

fn run_probe(job: impl FnOnce(&ActionMenuBarProbeWindow, &mut SlintDriver) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |driver: &mut SlintDriver| {
            let window = ActionMenuBarProbeWindow::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window, driver);
        },
    );
}

/// The event-loop pass that delivers the changed callbacks and the
/// re-layout after a property poke (the menu-shell probe's settle
/// shape, minus its row-specific park).
fn settle(window: &ActionMenuBarProbeWindow) {
    support::driver::advance_probe_clock(0);
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(1.0, 99999.0),
        });
    support::driver::advance_probe_clock(0);
}

/// A real pointer click at the logical window position (the
/// menu-shell probe's dispatch shape): move, press, release — one
/// clock tick between the phases.
fn click_at(window: &ActionMenuBarProbeWindow, x: f32, y: f32) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let p = slint::LogicalPosition::new(x, y);
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: p });
    support::driver::advance_probe_clock(1);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position: p,
        button: PointerEventButton::Left,
    });
    support::driver::advance_probe_clock(1);
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: p,
            button: PointerEventButton::Left,
        });
    support::driver::advance_probe_clock(1);
}

/// Forces the scroll (the #1309 pins' precondition): a real
/// Flickable drag — press inside the row list, drag up past the slop
/// in steps, release — moves the viewport (content-y < 0) through the
/// composition's own pointer path, no hand-poked scroll state.
fn drag_scroll(window: &ActionMenuBarProbeWindow, first_row_y: f32) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let start = slint::LogicalPosition::new(40.0, first_row_y + 10.0);
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: start });
    support::driver::advance_probe_clock(1);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position: start,
        button: PointerEventButton::Left,
    });
    support::driver::advance_probe_clock(1);
    for step in 1..=10_i32 {
        #[allow(clippy::cast_precision_loss)] // drag geometry, 10px steps
        let step_f = step as f32;
        let p = slint::LogicalPosition::new(40.0, start.y - 20.0 * step_f);
        window
            .window()
            .dispatch_event(WindowEvent::PointerMoved { position: p });
        support::driver::advance_probe_clock(1);
    }
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(40.0, start.y - 200.0),
            button: PointerEventButton::Left,
        });
    settle(window);
}

/// J5 US-1/S1 (AC-1) — the quick-bar row mounts structurally: one bar
/// row, the five model rows as icon buttons, each owning real pixels
/// (the zero-height squeeze guard — the H-2 hypothesis's geometry
/// half), and the populated panel is taller than the barless twin by
/// that same bar. The barless twin mounts no `quick-bar-row` at all
/// (the global count stays at the populated instance's one — the
/// "empty renders nothing" invariant).
#[test]
fn us1_s1_the_quick_bar_row_mounts_over_a_populated_model() {
    run_probe(|window, _driver| {
        let rows = find_all_by_id_on(window, "ActionMenu::quick-bar-row");
        assert_eq!(rows.len(), 1, "one bar row over the populated model");
        let row = &rows[0];
        assert!(
            row.size().height > 0.0 && row.size().width > 0.0,
            "the bar row owns real pixels (no squeeze to zero)"
        );

        let buttons = find_all_by_id_on(window, "ActionMenu::quick-bar-button");
        assert_eq!(buttons.len(), 5, "the model's five rows mount");
        for button in &buttons {
            assert!(
                button.size().height > 0.0 && button.size().width > 0.0,
                "every bar button owns real pixels"
            );
        }

        assert!(
            window.get_populated_menu_height() > window.get_barless_menu_height(),
            "the bar contributes its height to the panel"
        );
    });
}

/// J5 US-1 (the routing half) — a bar button's click routes by index
/// (click ≡ key): the composition's `invoke-quick-bar` carries the
/// clicked row's index, the seam the adapter resolves against the
/// Rust vec.
#[test]
fn the_bar_button_routes_by_index() {
    run_probe(|window, _driver| {
        assert_eq!(window.get_bar_invoked(), -1, "no fire at mount");
        // The third bar row's a11y button (the FocusScope overlay
        // carries the role one level under the `quick-bar-button`
        // Rectangle; its default action is the same callback the
        // TouchArea's click reaches — §7.1's equivalence). The label
        // is the element's address.
        let third = support::driver::find_by_label_on(window, "Unschedule")
            .expect("the third bar button's a11y surface is present");
        third.invoke_default_action();
        assert_eq!(
            window.get_bar_invoked(),
            2,
            "the click routes the clicked row's index"
        );
    });
}

/// #1309 (the occlusion pin's geometry half) — the layout contract:
/// at rest the bar row's y-band is disjoint from (strictly above) the
/// row list's first row's band — the bar is layout row one, the list
/// row two, and nothing paints under anything. Then, with the row
/// list scrolled by a real drag, the bar's band is UNCHANGED: the bar
/// lives outside the scroll and never scrolls away. (A scrolled row's
/// absolute band legitimately re-enters the bar's band — the
/// Flickable clips its viewport — so the disjointness oracle is the
/// rest geometry; the scrolled occlusion kill is the click pin
/// below.) Presence alone (`us1_s1`) does not pin this — the pre-fix
/// composition mounted the bar at the panel top and let the
/// Flickable's first row paint its opaque surface over the band.
#[test]
fn the_bar_band_is_disjoint_from_the_scrolled_row_list() {
    run_probe(|window, _driver| {
        let bar = find_all_by_id_on(window, "ActionMenu::quick-bar-row")
            .into_iter()
            .next()
            .expect("the bar row mounts");
        let bar_top = bar.absolute_position().y;
        let bar_bottom = bar_top + bar.size().height;
        assert!(bar_bottom > bar_top, "the bar band is non-empty");

        let first_row = find_by_label_on(window, "Edit").expect("the first menu row mounts");
        let unscrolled = first_row.absolute_position().y;
        assert!(
            bar_bottom <= unscrolled,
            "at rest the bar's band is disjoint from the list's first row (never occluded: bar {bar_top}..{bar_bottom}, row {unscrolled})"
        );

        drag_scroll(window, unscrolled);
        let scrolled = first_row.absolute_position().y;
        assert!(
            scrolled < unscrolled,
            "the row list scrolled (the viewport moved up: {unscrolled} -> {scrolled})"
        );
        let bar_after = find_all_by_id_on(window, "ActionMenu::quick-bar-row")
            .into_iter()
            .next()
            .expect("the bar row survives the scroll");
        let after_top = bar_after.absolute_position().y;
        let after_bottom = after_top + bar_after.size().height;
        assert!(
            (after_top - bar_top).abs() < f32::EPSILON
                && (after_bottom - bar_bottom).abs() < f32::EPSILON,
            "the bar's band is unchanged by the scroll (it lives outside the flick)"
        );
    });
}

/// #1309 (the occlusion pin's hit-test half) — a real pointer click
/// on a bar button while the row list is scrolled routes
/// `invoke-quick-bar` by index: the bar is hit-testable, not merely
/// present (the pre-fix Flickable covered the band, so the click
/// landed on the first menu row instead).
#[test]
fn the_scrolled_click_routes_the_bar_button() {
    run_probe(|window, _driver| {
        let third = find_by_label_on(window, "Unschedule")
            .expect("the third bar button's a11y surface is present");
        let position = third.absolute_position();
        let size = third.size();
        let (x, y) = (
            position.x + size.width / 2.0,
            position.y + size.height / 2.0,
        );
        let first_row = find_by_label_on(window, "Edit").expect("the first menu row mounts");

        drag_scroll(window, first_row.absolute_position().y);

        click_at(window, x, y);
        assert_eq!(
            window.get_bar_invoked(),
            2,
            "the scrolled click routes the bar button's index"
        );
    });
}

/// J5 US-3/S1 (AC-1) — the submenu chevron: exactly one for the one
/// submenu-parent row, none over the twin's leaf-only page, owning
/// real pixels (the fold's structural half: `entry.submenu` ⇒ chevron
/// present, `false` ⇒ absent).
#[test]
fn us3_s1_the_submenu_chevron_renders_iff_the_row_is_a_parent() {
    run_probe(|window, _driver| {
        // The chevron lives on the MenuRow primitive now (round-9 W6's
        // absorption) — the seam re-points to the landed id.
        let chevrons = find_all_by_id_on(window, "MenuRow::submenu-chevron");
        assert_eq!(
            chevrons.len(),
            1,
            "the one submenu parent carries one chevron; the leaf-only twin carries none"
        );
        assert!(
            chevrons[0].size().height > 0.0 && chevrons[0].size().width > 0.0,
            "the chevron owns real pixels"
        );
    });
}
