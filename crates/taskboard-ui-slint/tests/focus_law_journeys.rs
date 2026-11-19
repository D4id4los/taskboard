// SPDX-License-Identifier: AGPL-3.0-only

//! The directional-focus journeys (J3 US-1, round 9 chunk 9 — the §6
//! wiring table's nominated evidence file `focus_law_journeys.rs`):
//! the nav.focus-* per-view readings (the W-2 interpret table), the
//! text-entry guard negatives, the planner's pane-switch grammar, and
//! the #875 extend rows' one-front-per-press seam. Assertion
//! discipline is the journeys suite's: terminal state + the typed
//! commands that reached the fake-engine boundary; titles are
//! element addresses, never text oracles.

mod support;

use support::focus_journey::{JourneyHarness, golden_state_journey};

use slint::ComponentHandle as _;
use slint::Model as _;
use taskboard_ui_slint::View;
use taskboard_ui_slint::actions::{ActionId, ContextId};
use taskboard_ui_slint::keymap::Preset;
use taskboard_ui_slint::keymap::stroke::parse;

/// The emphasized card: `Some((column, row))` of the card whose
/// model carries the focus bit.
fn emphasized_card(harness: &JourneyHarness) -> Option<(usize, usize)> {
    let board = harness.handle.window().get_board_model();
    for column in 0..board.stacks.row_count() {
        let tasks = board.stacks.row_data(column).expect("column").tasks;
        for row in 0..tasks.row_count() {
            if tasks.row_data(row).expect("card").emphasis {
                return Some((column, row));
            }
        }
    }
    None
}

fn text_focus(harness: &JourneyHarness) -> i32 {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::EditingFacts>()
        .get_text_focus_count()
}

/// The task view's walk-stop fact, read back through the generated
/// global.
fn walk_stop(harness: &JourneyHarness) -> i32 {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::TaskViewWalkFacts>()
        .get_stop_id()
}

/// The open detail's mounted subject title (the fixture titles are
/// the journeys' element addresses — identity, never label matching).
fn detail_title(harness: &JourneyHarness) -> String {
    harness.handle.window().get_task_model().title.to_string()
}

/// A detail open over the fixture's first board card, through the
/// preset's own rows.
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

/// US-1/S1 + AC-2 (J3 W-2, round 9 chunk 9) — the `TaskDetail`
/// directional rows walk the CONTROL ladder: C-n steps down the stop
/// ladder (the walk-stop fact moves), C-p steps back up, and the
/// viewed subject NEVER moves (J3 R-3: no viewed-task change from the
/// detail at all — the census-dated round-8 D-2 supersession). Zero
/// commands — navigation is view-internal.
#[test]
fn us1_s1_taskdetail_directional_rows_walk_the_ladder_subject_pinned() {
    golden_state_journey(|state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);
        assert_eq!(
            detail_title(&harness),
            "task-0",
            "the detail opens on the first card"
        );

        // C-n steps the control walk onto a stop (the ladder).
        let mut guard = 0;
        while walk_stop(&harness) == 0 && guard < 10 {
            harness.press_preset(preset, ActionId::NavFocusDown);
            harness.advance();
            guard += 1;
        }
        assert_ne!(
            walk_stop(&harness),
            0,
            "C-n steps the detail's control ladder onto a stop"
        );
        // C-p steps back; the walk runs both axes (the degenerate
        // horizontal — the recorded geometry).
        harness.press_preset(preset, ActionId::NavFocusUp);
        harness.advance();
        // The subject is pinned: the walk never retargets the
        // detail, and the walk stays in the view.
        assert_eq!(
            detail_title(&harness),
            "task-0",
            "the directional walk never changes the viewed task (J3 R-3)"
        );
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::TaskDetail,
            "the view never moves"
        );
        harness.assert_commands(&[]);
    });
}

/// US-1/S2 (J3 W-2) — the Today horizontals are the control-walk
/// first cut (J3 R-4): C-f/C-b (nav.focus-right/left) step the
/// control walk, the verticals keep the item walk, all four stay in
/// the view with zero commands.
#[test]
fn today_horizontal_is_the_control_walk_first_cut() {
    golden_state_journey(|state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        harness.press_preset(preset, ActionId::ViewToday);
        harness.advance();
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Today,
            "today is open"
        );
        for action in [
            ActionId::NavFocusRight,
            ActionId::NavFocusLeft,
            ActionId::NavFocusDown,
            ActionId::NavFocusUp,
        ] {
            harness.press_preset(preset, action);
            harness.advance();
        }
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::Today,
            "the directional walk stays in the view"
        );
        harness.assert_commands(&[]);
    });
}

/// US-1/S1 per view (J3 W-2) — the vi motion keys step the board
/// walks: j/k walk the cards, l enters the stack walk (the
/// nav.focus carriers), the focus emphasis follows, nothing
/// dispatches.
#[test]
fn vi_motion_keys_step_the_board_walks() {
    golden_state_journey(|state| {
        let preset = Preset::Vi;
        let mut harness = JourneyHarness::mount(state, preset);
        // l enters the stack walk at the aimed column's head (the
        // step_stack bodies behind nav.focus-left/right).
        harness.press_preset(preset, ActionId::NavFocusRight);
        harness.advance();
        assert_eq!(
            emphasized_card(&harness),
            Some((0, 0)),
            "l lands the stack walk on the aimed column's head"
        );
        // j walks the cards within the column; k steps back.
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        assert_eq!(
            emphasized_card(&harness),
            Some((0, 1)),
            "j walks to the next card"
        );
        harness.press_preset(preset, ActionId::NavFocusUp);
        harness.advance();
        assert_eq!(
            emphasized_card(&harness),
            Some((0, 0)),
            "k steps back to the first card"
        );
        harness.assert_commands(&[]);
    });
}

/// US-1/S3 (J3 W-2) — the text-entry guard negatives per preset
/// family: under an engaged field the directional keys reach the
/// field (emacs C-n, standard arrows, vi's j under insert); the walk
/// never runs, the editor stays armed, nothing dispatches. The
/// guards are the inherited scopes — the test proves the vocabulary
/// never competes with them.
#[test]
fn the_text_entry_guards_own_the_directional_keys() {
    golden_state_journey(|state| {
        // Emacs: C-n inside the title editor reaches the field
        // (the insert chain masks the view context — no
        // NavFocusDown row binds there, so the physical C-n is
        // delivered raw and the editor scope owns it).
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state.clone(), preset);
        open_detail_first_card(&mut harness, preset);
        harness.press_preset(preset, ActionId::TaskTitleEdit);
        harness.advance();
        assert!(text_focus(&harness) > 0, "the title editor holds focus");
        harness.stroke(parse("Ctrl+n").expect("Ctrl+n"));
        harness.advance();
        assert!(
            text_focus(&harness) > 0,
            "C-n stays in the field (the guard owns it)"
        );
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::TaskDetail,
            "the view never moves under the guard"
        );
        harness.assert_commands(&[]);
        drop(harness);

        // Standard: the arrows inside the title editor are the
        // field's cursor motions.
        let preset = Preset::Standard;
        let mut harness = JourneyHarness::mount(state.clone(), preset);
        open_detail_first_card(&mut harness, preset);
        harness.press_preset(preset, ActionId::TaskTitleEdit);
        harness.advance();
        assert!(text_focus(&harness) > 0, "the title editor holds focus");
        harness.stroke(parse("Down").expect("Down"));
        harness.advance();
        harness.stroke(parse("Up").expect("Up"));
        harness.advance();
        assert!(
            text_focus(&harness) > 0,
            "the arrows stay in the field (the guard owns them)"
        );
        harness.assert_commands(&[]);
        drop(harness);

        // Vi: j under insert reaches the field (the shadowing-
        // without-masking law); the insert leg arms on the board
        // (the create field's insert chain).
        let preset = Preset::Vi;
        let mut harness = JourneyHarness::mount(state, preset);
        harness.press_preset(preset, ActionId::ModeInsert);
        harness.advance();
        assert!(
            harness.chain().contains(&ContextId::ViInsert),
            "insert mode is armed"
        );
        harness.stroke(parse("j").expect("j"));
        harness.advance();
        assert!(
            harness.chain().contains(&ContextId::ViInsert),
            "j under insert stays a printable (no walk, no mode flip)"
        );
        harness.assert_commands(&[]);
    });
}

/// The planner's pane-switch grammar (J3 R-5, round 9 chunk 9): the
/// directional left/right flip the focused pane (boxes ↔ pool), the
/// fresh open anchors on the boxes pane, and nothing dispatches (the
/// pane fact is view-internal; the in-pane walk is the recorded
/// out-of-scope follow-up).
#[test]
fn planner_directional_rows_switch_the_pane() {
    golden_state_journey(|state| {
        let preset = Preset::Standard;
        let mut harness = JourneyHarness::mount(state, preset);
        harness.press_preset(preset, ActionId::ViewPlanNextDay);
        harness.advance();
        let pane = |harness: &JourneyHarness| harness.handle.window().get_plan_model().focus_pane;
        assert_eq!(pane(&harness), 0, "a fresh open anchors on the boxes pane");
        harness.press_preset(preset, ActionId::NavFocusRight);
        harness.advance();
        assert_eq!(pane(&harness), 1, "Right lands on the pool pane");
        harness.press_preset(preset, ActionId::NavFocusLeft);
        harness.advance();
        assert_eq!(pane(&harness), 0, "Left lands back on the boxes pane");
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::PlanNextDay,
            "the pane switch stays in the planner"
        );
        harness.assert_commands(&[]);
    });
}

/// #875 (round 9 chunk 9) — the Standard extend row: one Shift+Down
/// press extends the selection by exactly one member (the
/// double-dispatch seam — the route-level row wins, the named-key
/// intercept stays the fallback front and never also fires; the
/// count of selected cards is the typed oracle).
#[test]
fn the_standard_extend_row_extends_one_member_per_press() {
    golden_state_journey(|state| {
        let preset = Preset::Standard;
        let mut harness = JourneyHarness::mount(state, preset);
        // The typed oracle is the selection COUNT over the whole
        // board: exactly the anchor plus the one extension member
        // — no double dispatch, no stray members anywhere.
        // Seed the walk focus on the first card, then anchor it.
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        harness
            .handle
            .window()
            .invoke_board_task_clicked_modifiers(0, 0, true, false);
        harness.stroke(parse("Shift+Down").expect("Shift+Down"));
        harness.advance();
        assert_eq!(
            harness.selected_count(),
            2,
            "the anchor plus exactly one extension member (one front, one press)"
        );
        harness.assert_commands(&[]);
    });
}

/// #875 (round 9 chunk 9) — the vi and emacs extend rows: a Shift+j
/// press folds the shift into the glyph (the D6 law) and `J` extends
/// the selection; emacs' Ctrl+Shift+n twin rides the chorded case
/// law. One front per press (the same typed oracle).
#[test]
fn the_vi_and_emacs_extend_rows_extend_one_member_per_press() {
    golden_state_journey(|state| {
        // Vi: j seeds the walk, J extends.
        let preset = Preset::Vi;
        let mut harness = JourneyHarness::mount(state.clone(), preset);
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        harness.stroke(parse("J").expect("J"));
        harness.advance();
        assert_eq!(
            harness.selected_count(),
            2,
            "the anchor plus exactly one J extension (one front, one press)"
        );
        harness.assert_commands(&[]);
        drop(harness);

        // Emacs: Ctrl+Shift+n extends after seeding the walk.
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        harness.stroke(parse("Ctrl+Shift+n").expect("Ctrl+Shift+n"));
        harness.advance();
        assert_eq!(
            harness.selected_count(),
            2,
            "the anchor plus exactly one Ctrl+Shift+n extension (one front, one press)"
        );
        harness.assert_commands(&[]);
    });
}
