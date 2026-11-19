// SPDX-License-Identifier: AGPL-3.0-only

//! The board-move journeys (J3 US-2, round 9 chunk 9 — the §6 wiring
//! table's nominated evidence file `board_move_journeys.rs`): the
//! move survives directional hops (the W-3 kept arms — the settle
//! matrix never cancels the flow under the directional keys).

mod support;

use support::focus_journey::{JourneyHarness, golden_state_journey};

use slint::Model as _;
use taskboard_domain::StateCommand;
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::keymap::Preset;

/// The move aim: `Some(column)` of the column whose cards carry
/// the move-aim badge state (the drop target's model fact).
fn aim(harness: &JourneyHarness) -> Option<usize> {
    let board = harness.handle.window().get_board_model();
    (0..board.stacks.row_count())
        .find(|column| board.stacks.row_data(*column).expect("column").move_aim)
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
    golden_state_journey(|state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        harness.press_preset(preset, ActionId::TaskMove);
        harness.advance();
        assert_eq!(aim(&harness), Some(0), "the move arms on the home column");

        // C-f hops the drop column right; C-b back; the move is
        // alive and dispatches nothing (the kept arms).
        harness.press_preset(preset, ActionId::NavFocusRight);
        harness.advance();
        assert_eq!(aim(&harness), Some(1), "C-f hopped the drop column");
        harness.assert_commands(&[]);
        harness.press_preset(preset, ActionId::NavFocusLeft);
        harness.advance();
        assert_eq!(aim(&harness), Some(0), "C-b hopped the drop column back");
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
    });
}
