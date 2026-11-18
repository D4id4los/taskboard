// SPDX-License-Identifier: AGPL-3.0-only

//! The action-menu bar probe's tier-2 suite (J5 US-1/US-3, plan §4):
//! the real `ActionMenu` composition's quick-bar contract — the bar
//! row mounts structurally over a populated model (bar row + per-entry
//! buttons owning real pixels, the zero-height squeeze guard), the
//! barless model mounts nothing (the other consumers' bit-identical
//! invariant), the bar's click routes by index, and the submenu
//! chevron renders iff the row is a submenu parent (US-3 AC-1, one
//! component all menus inherit). Deterministic: one platform turn per
//! dispatch, no sleeps, no text assertions.

mod support;

use support::driver::{DriverOptions, find_all_by_id_on};

include!(concat!(env!("OUT_DIR"), "/action_menu_bar_probe.rs"));

fn run_probe(job: impl FnOnce(&ActionMenuBarProbeWindow) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = ActionMenuBarProbeWindow::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window);
        },
    );
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
    run_probe(|window| {
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
    run_probe(|window| {
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

/// J5 US-3/S1 (AC-1) — the submenu chevron: exactly one for the one
/// submenu-parent row, none over the twin's leaf-only page, owning
/// real pixels (the fold's structural half: `entry.submenu` ⇒ chevron
/// present, `false` ⇒ absent).
#[test]
fn us3_s1_the_submenu_chevron_renders_iff_the_row_is_a_parent() {
    run_probe(|window| {
        let chevrons = find_all_by_id_on(window, "ActionMenu::submenu-chevron");
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
