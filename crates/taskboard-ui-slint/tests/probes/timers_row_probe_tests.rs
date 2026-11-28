// SPDX-License-Identifier: AGPL-3.0-only

//! The timers creation-row probe suite (#1491): at the kiosk scale
//! (`Theme.base-scale` 3.0, ~333 logical px of viewport) the
//! creation row's three children must stay inside the viewport in
//! BOTH breakpoint arms, and the stacked arm must mount the
//! insistence picker BELOW the fields (not beside). Typed geometry
//! assertions only — no pixel oracles, no text oracles. The pre-fix
//! row painted past the right canvas edge at 3.0 (the analyzer's
//! 133 px flush run on `today-3.0-1000`).

use crate::support;

use support::driver::{DriverOptions, advance_probe_clock, find_all_by_id_on};

include!(concat!(env!("OUT_DIR"), "/timers_row_probe.rs"));

/// Drives one arm: the pushed breakpoint fact, the theme content
/// scale, and a viewport the arm's breakpoint band actually serves —
/// ~333 logical px (the 3.0/1000-px golden cell's window width) for
/// the compact arm, 1000 for the regular arms.
fn run_arm(
    breakpoint: Breakpoint,
    scale: f32,
    width: f32,
    job: impl FnOnce(&TimersRowProbe) + Send + 'static,
) {
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let window = TimersRowProbe::new().expect("the probe window instantiates");
            window.global::<Theme>().set_base_scale(scale);
            window
                .global::<Responsive>()
                .set_current_breakpoint(breakpoint);
            window.show().expect("show on the testing platform");
            window
                .window()
                .set_size(slint::LogicalSize::new(width, 900.0));
            advance_probe_clock(0);
            job(&window);
        },
    );
}

/// Every named child of the creation row sits strictly inside the
/// viewport horizontally: x >= 0 and x + width <= 333 (the
/// pre-fix row's min-content overflow painted past the right edge).
fn assert_children_inside_viewport(window: &TimersRowProbe, viewport: f32, ids: &[&str]) {
    for id in ids {
        let elements = find_all_by_id_on(window, id);
        assert_eq!(elements.len(), 1, "{id} mounts exactly once");
        let element = &elements[0];
        let pos = element.absolute_position();
        let size = element.size();
        assert!(
            pos.x >= 0.0,
            "{id} must not paint past the left edge (x = {})",
            pos.x
        );
        assert!(
            pos.x + size.width <= viewport,
            "{id} must not paint past the right edge (x + w = {} vs {viewport})",
            pos.x + size.width
        );
    }
}

/// The compact arm: the row stacks — every child inside the
/// viewport, and the insistence picker mounts BELOW the fields row
/// (segmented-y > fields-y) starting at the fields' x.
#[test]
fn the_stacked_arm_keeps_every_child_inside_the_viewport() {
    run_arm(Breakpoint::Compact, 3.0, 333.0, |window| {
        assert_children_inside_viewport(
            window,
            333.0,
            &[
                "TimersSection::stacked-label-field",
                "TimersSection::stacked-when-field",
                "TimersSection::insistence-picker",
            ],
        );
        let picker = &find_all_by_id_on(window, "TimersSection::insistence-picker")[0];
        let fields = &find_all_by_id_on(window, "TimersSection::stacked-label-field")[0];
        assert!(
            picker.absolute_position().y > fields.absolute_position().y,
            "the picker mounts below the fields row (picker y = {}, fields y = {})",
            picker.absolute_position().y,
            fields.absolute_position().y
        );
        assert_eq!(
            picker.absolute_position().x,
            fields.absolute_position().x,
            "the picker starts at the fields' x (the stacked column)"
        );
    });
}

/// The regular arm at desktop scale: today's single row — every
/// child inside the viewport, and the insistence picker stays
/// BESIDE the fields (one row: picker y == fields y).
#[test]
fn the_wide_arm_keeps_every_child_inside_the_viewport() {
    run_arm(Breakpoint::Regular, 1.0, 1000.0, |window| {
        assert_children_inside_viewport(
            window,
            1000.0,
            &[
                "TimersSection::label-field",
                "TimersSection::when-field",
                "TimersSection::wide-insistence-picker",
            ],
        );
        let picker = &find_all_by_id_on(window, "TimersSection::wide-insistence-picker")[0];
        let fields = &find_all_by_id_on(window, "TimersSection::label-field")[0];
        assert_eq!(
            picker.absolute_position().y,
            fields.absolute_position().y,
            "the wide arm keeps the picker on the fields' row"
        );
    });
}

/// The kiosk-crowding arm (#1491's actual cell): a regular-width
/// window at kiosk content scale — the row's min-content grows 3x
/// while the window stays regular — stacks, and every child stays
/// inside the viewport with the picker below the fields.
#[test]
fn the_kiosk_scale_crowding_stacks_the_regular_row() {
    run_arm(Breakpoint::Regular, 3.0, 1000.0, |window| {
        assert_children_inside_viewport(
            window,
            1000.0,
            &[
                "TimersSection::stacked-label-field",
                "TimersSection::stacked-when-field",
                "TimersSection::insistence-picker",
            ],
        );
        let picker = &find_all_by_id_on(window, "TimersSection::insistence-picker")[0];
        let fields = &find_all_by_id_on(window, "TimersSection::stacked-label-field")[0];
        assert!(
            picker.absolute_position().y > fields.absolute_position().y,
            "the picker mounts below the fields rows (picker y = {}, fields y = {})",
            picker.absolute_position().y,
            fields.absolute_position().y
        );
        assert_eq!(
            picker.absolute_position().x,
            fields.absolute_position().x,
            "the picker starts at the fields' x (the stacked column)"
        );
    });
}
