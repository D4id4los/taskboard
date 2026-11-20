// SPDX-License-Identifier: AGPL-3.0-only

//! The tracker-verb journeys (J3 US-5, round 9 chunk 11 — the §6
//! wiring table's nominated evidence file `tracker_journeys.rs`):
//! the Board-context start/stop twins through each preset's own
//! spelling (US-5's verbatim extension of the r8 rows) and the smart
//! toggle's board-context round trip (R-8 over the walk focus).
//! Assertion discipline is the journeys suite's: typed commands at
//! the fake-engine boundary, never text oracles.

mod support;

use support::focus_journey::{JourneyHarness, golden_state_journey};

use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::keymap::Preset;

/// US-5/S1 (J3 W-5, round 9 chunk 11) — the start/stop twins resolve
/// in the Board chain in every preset and route the exact commands
/// over the walk-focused card: Standard F9/F10, emacs C-c C-i /
/// C-c C-o, vi I/O. Start then stop per preset; the walk focus never
/// moves.
#[test]
fn us5_s1_tracker_pair_routes_on_the_board_in_every_preset() {
    golden_state_journey(|state| {
        let legs = [
            (
                Preset::Standard,
                ActionId::TaskTrackStart,
                ActionId::TaskTrackStop,
            ),
            (
                Preset::Emacs,
                ActionId::TaskTrackStart,
                ActionId::TaskTrackStop,
            ),
            (
                Preset::Vi,
                ActionId::TaskTrackStart,
                ActionId::TaskTrackStop,
            ),
        ];
        for (preset, start, stop) in legs {
            let mut harness = JourneyHarness::mount(state.clone(), preset);
            // Seed the walk focus on the fixture's first card.
            harness.press_preset(preset, ActionId::NavFocusDown);
            harness.advance();

            harness.press_preset(preset, start);
            harness.advance();
            let drained = harness.engine.drain();
            let [taskboard_domain::StateCommand::StartTracking { task, .. }] = &drained[..] else {
                panic!("{preset:?}: the board start routes StartTracking, got {drained:?}");
            };
            assert_eq!(
                harness
                    .engine
                    .snapshot()
                    .tasks
                    .get(task)
                    .expect("tracked")
                    .title,
                "task-0",
                "{preset:?}: the start targeted the walk focus"
            );
            harness.engine.settle(&harness.handle);

            harness.press_preset(preset, stop);
            harness.advance();
            let drained = harness.engine.drain();
            assert!(
                matches!(
                    drained.as_slice(),
                    [taskboard_domain::StateCommand::StopTracking]
                ),
                "{preset:?}: the board stop routes StopTracking, got {drained:?}"
            );
            drop(harness);
        }
    });
}

/// US-5's toggle leg (J3 R-8, round 9 chunk 11) — the smart toggle in
/// Board context round-trips over the walk focus: vi's `c` (the
/// `ViNormal` chain's one row serving all four card contexts) starts,
/// the settle folds the entry, `c` again stops.
#[test]
fn the_board_toggle_round_trips_over_the_walk_focus() {
    golden_state_journey(|state| {
        let preset = Preset::Vi;
        let mut harness = JourneyHarness::mount(state, preset);
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();

        harness.press_preset(preset, ActionId::TaskTrackToggle);
        harness.advance();
        let drained = harness.engine.drain();
        let [taskboard_domain::StateCommand::StartTracking { task, .. }] = &drained[..] else {
            panic!("the vi board toggle starts, got {drained:?}");
        };
        assert_eq!(
            harness
                .engine
                .snapshot()
                .tasks
                .get(task)
                .expect("tracked")
                .title,
            "task-0",
            "the toggle targeted the walk focus"
        );
        harness.engine.settle(&harness.handle);

        harness.press_preset(preset, ActionId::TaskTrackToggle);
        harness.advance();
        let drained = harness.engine.drain();
        assert!(
            matches!(
                drained.as_slice(),
                [taskboard_domain::StateCommand::StopTracking]
            ),
            "the second press stops, got {drained:?}"
        );
    });
}
