// SPDX-License-Identifier: AGPL-3.0-only

//! The board-move journeys (J3 US-2, round 9 chunk 9 — the §6 wiring
//! table's nominated evidence file `board_move_journeys.rs`): the
//! move survives directional hops (the W-3 kept arms — the settle
//! matrix never cancels the flow under the directional keys).

mod support;

use support::driver::{DriverOptions, JourneyDriver, SlintDriver};
use support::fake_engine::FakeEngine;
use support::fixture;
use support::health::detached_health;

use arc_swap::ArcSwap;
use slint::Model as _;
use std::sync::Arc;
use taskboard_domain::{AppState, StateCommand};
use taskboard_ui_slint::UiSettings;
use taskboard_ui_slint::actions::ActionId;

/// The minimal move-journey harness (the `focus_law_journeys` shape).
struct Harness {
    driver: SlintDriver,
    handle: taskboard_ui_slint::UiHandle,
    engine: FakeEngine,
}

fn dummy_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

fn dummy_system_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

impl Harness {
    fn mount_with_settings(driver: SlintDriver, state: AppState, settings: UiSettings) -> Self {
        let state = Arc::new(ArcSwap::from_pointee(state));
        let (commands, engine) = FakeEngine::channel(Arc::clone(&state));
        let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
        let handle = taskboard_ui_slint::mount(
            state,
            dummy_receiver(),
            dummy_system_receiver(),
            commands,
            requests_tx,
            settings,
            detached_health(),
            None,
        )
        .expect("mount succeeds on the testing backend");
        let mut harness = Self {
            driver,
            handle,
            engine,
        };
        harness.driver.bind(&harness.handle);
        harness
            .handle
            .dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
        harness
    }

    fn advance(&mut self) {
        self.driver.advance_time(0);
    }

    fn press_preset(&mut self, preset: taskboard_ui_slint::keymap::Preset, action: ActionId) {
        let keymap = taskboard_ui_slint::keymap::Keymap::from_preset(preset);
        let chain = vec![taskboard_ui_slint::actions::ContextId::Board];
        let hint = keymap
            .first_binding_display(action, &chain)
            .unwrap_or_else(|| panic!("{action:?} is bound in the visible chain {chain:?}"));
        for spelling in &hint {
            let stroke = taskboard_ui_slint::keymap::stroke::parse(spelling)
                .unwrap_or_else(|error| panic!("{action:?}'s hint {spelling:?}: {error}"));
            self.driver.press_stroke(stroke);
        }
    }

    fn assert_commands(&mut self, expected: &[StateCommand]) {
        let drained = self.engine.drain();
        assert_eq!(&drained, expected, "the boundary captured exactly these");
    }

    /// The move aim: `Some(column)` of the column whose cards carry
    /// the move-aim badge state (the drop target's model fact).
    fn aim(&self) -> Option<usize> {
        let board = self.handle.window().get_board_model();
        (0..board.stacks.row_count())
            .find(|column| board.stacks.row_data(*column).expect("column").move_aim)
    }
}

/// US-2/S1 (J3 W-3, round 9 chunk 9) — the move survives directional
/// hops: C-c C-w arms the move, C-f/C-b hop the drop column (the aim
/// badge follows the column), C-n rides the target ring, and Return
/// completes the move with exactly one `MoveTask` — the settle
/// matrix never cancels the flow under the directional keys (the
/// kept arms; the `StackNext` settle bug class is dead on this path).
/// C-g aborts (the US-2 escape cell rides the existing move-cancel
/// journeys).
#[test]
fn us2_s1_the_move_survives_directional_column_hops() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let preset = taskboard_ui_slint::keymap::Preset::Emacs;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            harness.press_preset(preset, ActionId::NavFocusDown);
            harness.advance();
            harness.press_preset(preset, ActionId::TaskMove);
            harness.advance();
            assert_eq!(harness.aim(), Some(0), "the move arms on the home column");

            // C-f hops the drop column right; C-b back; the move is
            // alive and dispatches nothing (the kept arms).
            harness.press_preset(preset, ActionId::NavFocusRight);
            harness.advance();
            assert_eq!(harness.aim(), Some(1), "C-f hopped the drop column");
            harness.assert_commands(&[]);
            harness.press_preset(preset, ActionId::NavFocusLeft);
            harness.advance();
            assert_eq!(harness.aim(), Some(0), "C-b hopped the drop column back");
            harness.press_preset(preset, ActionId::NavFocusRight);
            harness.advance();

            // C-n rides the target ring (the vertical kept arm); the
            // move survives.
            harness.press_preset(preset, ActionId::NavFocusDown);
            harness.advance();
            harness.assert_commands(&[]);

            // Return completes: exactly one MoveTask at the boundary.
            harness.press_preset(preset, ActionId::TaskEdit);
            harness.advance();
            match harness.engine.drain().as_slice() {
                [StateCommand::MoveTask { .. }] => {}
                other => panic!("the move completes after the hops, got {other:?}"),
            }
        },
    );
}
