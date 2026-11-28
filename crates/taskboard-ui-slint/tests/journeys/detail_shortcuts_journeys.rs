// SPDX-License-Identifier: AGPL-3.0-only

//! The task-detail shortcut journeys (J3 US-4, round 9 chunk 11 —
//! the §6 wiring table's nominated evidence file
//! `detail_shortcuts_journeys.rs`): the detail's done pair through
//! its new preset strokes (`us4_s1`) and the ONE smart tracking toggle
//! in both directions over the detail and board contexts (`us4_s2`).
//! Assertion discipline is the journeys suite's: terminal state +
//! the typed commands that reached the fake-engine boundary; titles
//! are element addresses, never text oracles.

use crate::support;

use support::focus_journey::{JourneyHarness, golden_state_journey};

use slint::ComponentHandle as _;
use taskboard_ui_slint::EditingFacts;
use taskboard_ui_slint::View;
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::keymap::Preset;
use taskboard_ui_slint::keymap::stroke::parse;

/// The `EditingFacts` focus count (the text field's focus seam — the
/// opener's observable).
fn text_focus(harness: &JourneyHarness) -> i32 {
    harness
        .handle
        .window()
        .global::<EditingFacts>()
        .get_text_focus_count()
}

/// A detail open over the fixture's first board card, through the
/// preset's own rows (the shared directional-journey preamble's
/// shape, kept local to this binary's oracles).
fn open_detail_first_card(harness: &mut JourneyHarness, preset: Preset) {
    harness.press_preset(preset, ActionId::NavFocusDown);
    harness.press_preset(preset, ActionId::TaskEdit);
    harness.advance();
    assert_eq!(
        harness.handle.window().get_current_view(),
        View::TaskDetail,
        "the detail is open"
    );
}

/// US-4/S1 (J3 W-4, round 9 chunk 11) — the done pair is keyboard
/// reachable at the detail in Standard: `Space` completes the viewed
/// subject (`SetTaskDone` over it), the settle folds the done fact, and
/// `Ctrl+Space` reopens it (the state-correct pick inverts). No
/// pointer, no Tab.
#[test]
fn us4_s1_done_pair_through_the_detail_strokes() {
    golden_state_journey(|state| {
        let preset = Preset::Standard;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);

        harness.press_preset(preset, ActionId::TaskComplete);
        harness.advance();
        let drained = harness.engine.drain();
        assert!(
            matches!(
                drained.as_slice(),
                [taskboard_domain::StateCommand::SetTaskDone { done: true, .. }]
            ),
            "Space completes the viewed subject, got {drained:?}"
        );
        harness.engine.settle(&harness.handle);

        harness.press_preset(preset, ActionId::TaskReopen);
        harness.advance();
        let drained = harness.engine.drain();
        assert!(
            matches!(
                drained.as_slice(),
                [taskboard_domain::StateCommand::SetTaskDone { done: false, .. }]
            ),
            "Ctrl+Space reopens the done subject, got {drained:?}"
        );
    });
}

/// US-4/S2 (J3 W-4, round 9 chunk 11) — the ONE smart tracking toggle
/// (R-8) in both directions at the detail (emacs' bare `C-c i`, the
/// R-2 bare-vs-chord contract stroke): the first press starts (the
/// `StartTracking` command), the settle folds the open entry so the
/// second press routes the stop half — the same stroke, the
/// state-correct half both times. The board leg resolves the walk
/// focus as the target (R-4: focus, not a quick-bar card): Standard's
/// `Ctrl+F9` starts over the walked card.
#[test]
fn us4_s2_smart_toggle_routes_both_halves_over_the_target() {
    golden_state_journey(|state| {
        // The detail leg: start, then stop, same stroke.
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state.clone(), preset);
        open_detail_first_card(&mut harness, preset);
        harness.press_preset(preset, ActionId::TaskTrackToggle);
        harness.advance();
        let drained = harness.engine.drain();
        assert!(
            matches!(
                drained.as_slice(),
                [taskboard_domain::StateCommand::StartTracking { .. }]
            ),
            "the idle subject starts, got {drained:?}"
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
            "the running subject stops under the same stroke, got {drained:?}"
        );
        drop(harness);

        // The board leg: the walk focus IS the toggle target (R-4).
        let preset = Preset::Standard;
        let mut harness = JourneyHarness::mount(state, preset);
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        harness.press_preset(preset, ActionId::TaskTrackToggle);
        harness.advance();
        let drained = harness.engine.drain();
        let [taskboard_domain::StateCommand::StartTracking { task, .. }] = &drained[..] else {
            panic!("the board toggle starts over the walked card, got {drained:?}");
        };
        // The walked card is the fixture's first card: its title is
        // the element address.
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
        harness.assert_commands(&[]);
    });
}

/// The back verb (J3 US-4, round 9 chunk 11 — the D-table's Back
/// ruling): the board-return closes the detail in every preset, and
/// zero commands dispatch (pure view intent). DATED SUPERSESSION
/// 2026-11-28 (#1257, the keybinding-coverage plan D3): the
/// field-departing Alt axis bound `NavFocusLeft` at `Alt+Left` in the
/// view contexts of Standard and Emacs, so the Global `Alt+Left`
/// alternate is shadowed there by context precedence (the C-5/D-5
/// shadow is the recorded cost) — the D-table's primaries carry the
/// journey now: `Alt+1` (Standard), `C-x C-b` (Emacs), and vi's
/// `Alt+Left` Global alternate, which stays unshadowed (vi carries no
/// axis rows).
#[test]
fn the_board_return_closes_the_detail_in_every_preset() {
    golden_state_journey(|state| {
        for preset in [Preset::Standard, Preset::Emacs, Preset::Vi] {
            let mut harness = JourneyHarness::mount(state.clone(), preset);
            open_detail_first_card(&mut harness, preset);
            match preset {
                Preset::Standard => harness.stroke(parse("Alt+1").expect("Alt+1")),
                Preset::Emacs => {
                    harness.stroke(parse("Ctrl+x").expect("Ctrl+x"));
                    harness.stroke(parse("Ctrl+b").expect("Ctrl+b"));
                }
                Preset::Vi => harness.stroke(parse("Alt+Left").expect("Alt+Left")),
            }
            harness.advance();
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Board,
                "{preset:?}: the board return closes the detail"
            );
            harness.assert_commands(&[]);
        }
    });
}

/// R-3's no-spurious-commit pin (round 9 chunk 11): the keyboard
/// opener (Ctrl+e) really focuses the always-on effort field (the
/// `EditingFacts` focus count moves), and a focus + blur without an
/// edit dispatches NOTHING — the M9 commit-on-focus-loss law plus
/// the D-5 last-committed guard own the field; the opener arms
/// nothing.
#[test]
fn the_keyboard_effort_open_focuses_and_never_spuriously_commits() {
    golden_state_journey(|state| {
        let preset = Preset::Standard;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);
        let before = text_focus(&harness);
        harness.press_preset(preset, ActionId::TaskEffortEdit);
        harness.advance();
        assert!(
            text_focus(&harness) > before,
            "the opener focused the effort field"
        );

        harness.handle.window().invoke_task_effort_focus_lost();
        harness.advance();
        harness.assert_commands(&[]);
    });
}
