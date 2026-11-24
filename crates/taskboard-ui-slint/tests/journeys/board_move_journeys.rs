// SPDX-License-Identifier: AGPL-3.0-only

//! The board-move journeys (J3 US-2, round 9 chunk 9 — the §6 wiring
//! table's nominated evidence file `board_move_journeys.rs`): the
//! move survives directional hops (the W-3 kept arms — the settle
//! matrix never cancels the flow under the directional keys).

use crate::support;

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

        // Return completes: exactly one command at the boundary — the
        // same-stack aim mints the engine-side reorder (B3 #1247).
        harness.press_preset(preset, ActionId::TaskEdit);
        harness.advance();
        match harness.engine.drain().as_slice() {
            [StateCommand::TaskReorder { .. }] => {}
            other => panic!("the move completes after the hops, got {other:?}"),
        }
    });
}

/// US-2/S2 (#1033) — the legacy stack-walk chords join the kept arms:
/// `StackNext` (the `g l` `ViNormal` chord, the Standard/emacs
/// `Ctrl+Shift+…` rows) dispatched mid-`Flow::Move` hops the drop
/// column instead of settling the flow — the settle matrix never sees
/// the stroke, the aim badge follows the hop, and the boundary stays
/// silent. The `Flow::None` counterpart (the stack walk itself) is
/// pinned by `journeys.rs::the_stack_walk_wraps_at_both_ends` — this
/// journey pins only the move-flow kept arms.
#[test]
fn us2_s2_the_stack_chords_hop_the_drop_column_mid_move() {
    golden_state_journey(|state| {
        // The Standard preset carries the chords' Board rows
        // (Ctrl+Shift+Left/Right — the issue's repro spelling).
        let preset = Preset::Standard;
        let mut harness = JourneyHarness::mount(state, preset);
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        harness.press_preset(preset, ActionId::TaskMove);
        harness.advance();
        assert_eq!(aim(&harness), Some(0), "the move arms on the home column");

        // The stack-walk chord hops the drop column right; the move
        // is alive and dispatches nothing (the kept arm — the settle
        // matrix never cancels the flow).
        harness.press_preset(preset, ActionId::StackNext);
        harness.advance();
        assert_eq!(aim(&harness), Some(1), "the chord hopped the drop column");
        harness.assert_commands(&[]);

        // The twin chord hops back (StackPrev → -1, the clamp edge).
        harness.press_preset(preset, ActionId::StackPrev);
        harness.advance();
        assert_eq!(aim(&harness), Some(0), "the twin chord hopped back");

        // The flow still completes: exactly one command — the
        // same-stack aim mints the engine-side reorder (B3 #1247).
        harness.press_preset(preset, ActionId::TaskEdit);
        harness.advance();
        match harness.engine.drain().as_slice() {
            [StateCommand::TaskReorder { .. }] => {}
            other => panic!("the move completes after the chord hops, got {other:?}"),
        }
    });
}

/// US-2/S2 (#1033, the stack-move half) — the same chord rides the
/// insertion-slot ring mid-`Flow::MoveStack`: `StackNext` steps the
/// slot (the highlight follows `min(slot, len-1)`), the moving
/// stack's selection never changes, and the boundary stays silent —
/// pre-fix the stroke settled the flow and re-ran the stack walk.
#[test]
fn us2_s2_the_stack_chords_step_the_insertion_slot_mid_stack_move() {
    golden_state_journey(|state| {
        let preset = Preset::Standard;
        let mut harness = JourneyHarness::mount(state, preset);
        harness.press_preset(preset, ActionId::StackMove);
        harness.advance();
        let board = harness.handle.window().get_board_model();
        assert_eq!(board.move_target, 0, "the stack move arms at its own slot");

        // The chord steps the insertion slot; the flow survives.
        harness.press_preset(preset, ActionId::StackNext);
        harness.advance();
        let board = harness.handle.window().get_board_model();
        assert_eq!(board.move_target, 1, "the chord stepped the insertion slot");
        harness.assert_commands(&[]);

        // The twin chord steps back.
        harness.press_preset(preset, ActionId::StackPrev);
        harness.advance();
        let board = harness.handle.window().get_board_model();
        assert_eq!(board.move_target, 0, "the twin chord stepped back");

        // The wrap-back lands the append slot (slot 0 wraps to 2 of
        // 0..=2 — after the last stack, a real reorder; slot 1 is the
        // stack's own position and dispatches nothing on confirm by
        // the minimal-set design), then Return completes the stack
        // move — the flow was alive under the chords.
        harness.press_preset(preset, ActionId::StackPrev);
        harness.advance();
        harness.stroke(taskboard_ui_slint::keymap::stroke::parse("Return").expect("Return parses"));
        harness.advance();
        let drained = harness.engine.drain();
        assert!(
            drained
                .iter()
                .any(|c| matches!(c, StateCommand::MoveStack { .. })),
            "the stack move completes after the chord steps"
        );
    });
}
