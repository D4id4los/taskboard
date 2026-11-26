// SPDX-License-Identifier: AGPL-3.0-only

//! The habit entry-height parity probe suite (#1018, the
//! task-card-title precedent): the real `HabitsSection` with one
//! habit row beside a bare `TaskCard`; the pin asserts the two
//! cards' realized heights match — the strip-below-keeps-ladder-step
//! law (H2 D3: the row's entry height is a task row's, no fixed px,
//! the card's own min-height governs). Equality of the two reads is
//! the whole assertion; no fixed-px constant. Mounts from
//! `probe_pixel.rs` (rendered geometry; the group's
//! `HEADLESS_RENDERER`).

use crate::support;

use slint::ComponentHandle as _;
use slint::{ModelRc, SharedString, VecModel};

use support::driver::{DriverOptions, JourneyDriver as _, SlintDriver, find_by_label_on};

include!(concat!(env!("OUT_DIR"), "/habit_entry_height_probe.rs"));

fn run_probe(job: impl FnOnce(&HabitEntryHeightProbe, &mut SlintDriver) + Send + 'static) {
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let window = HabitEntryHeightProbe::new().expect("the probe window instantiates");
            window.show().expect("show on the testing platform");
            job(&window, driver);
        },
    );
}

/// The one-habit-row seed (the section's model): the card substrate
/// carries the habit title (the accessible address the geometry read
/// uses).
fn one_habit_row() -> ModelRc<HabitCardModel> {
    let card = TaskCardModel {
        title: SharedString::from("water"),
        due_phrase: SharedString::from(""),
        scheduled_phrase: SharedString::from(""),
        effort_phrase: SharedString::from(""),
        due_kind: DueKind::None,
        done: false,
        can_done: true,
        emphasis: false,
        dragging_source: false,
        selected: false,
        anchor: false,
        track_state: TrackCardState::None,
        decorations: ModelRc::new(VecModel::from(std::vec::Vec::new())),
        label_chips: ModelRc::new(VecModel::from(std::vec::Vec::new())),
        label_chips_overflow: 0,
    };
    let rows = vec![HabitCardModel {
        card,
        repair: false,
        repair_undoable: false,
        met: false,
        skip: false,
        paused: false,
        paused_until: "".into(),
        progress_phrase: "0/2".into(),
        // #1315: the card model's projected hint-keys — the height
        // probe needs no bindings (an empty model renders nothing).
        hint_keys: ModelRc::new(VecModel::from(std::vec::Vec::new())),
    }];
    ModelRc::new(VecModel::from(rows))
}

/// #1018's pin: the habit card and the bare task card resolve to the
/// same ladder step — the realized heights match exactly (equality,
/// no fixed-px constant). The habit card's height rides its
/// accessible address (the title, the D16 address); the reference
/// card's rides the fixture's typed seam.
#[test]
fn the_habit_card_height_matches_the_task_card_ladder_step() {
    run_probe(|window, driver| {
        window.set_rows(one_habit_row());
        driver.advance_time(0);
        let task_card_height = window.get_task_card_height();
        let habit_card_height = find_by_label_on(window, "water")
            .map(|card| card.size().height)
            .filter(|height| *height > 0.0)
            .unwrap_or_else(|| panic!("the habit card renders with its title address"));
        assert!(task_card_height > 0.0, "the reference task card renders");
        assert_eq!(
            habit_card_height, task_card_height,
            "the habit card resolves to the task card's ladder step"
        );
    });
}
