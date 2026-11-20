// SPDX-License-Identifier: AGPL-3.0-only

//! The directional-focus journeys (J3 US-1, round 9 chunk 9 — the §6
//! wiring table's nominated evidence file `focus_law_journeys.rs`):
//! the nav.focus-* per-view readings (the W-2 interpret table), the
//! text-entry guard negatives, the planner's pane-switch grammar, and
//! the #875 extend rows' one-front-per-press seam. Assertion
//! discipline is the journeys suite's: terminal state + the typed
//! commands that reached the fake-engine boundary; titles are
//! element addresses, never text oracles.

#[path = "../support/mod.rs"]
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

// -- the rule cursor walk (J3 US-6, round 9 chunk 12) -------------------

use taskboard_domain::AppState;
use taskboard_domain::Insistence;
use taskboard_domain::ReminderKind;
use taskboard_domain::ReminderOffset;
use taskboard_domain::ReminderRule;
use taskboard_domain::ReminderRuleId;
use taskboard_domain::StateCommand;
use taskboard_domain::TaskId;

/// Seeds an ARMED rule on `task` under a fixed ordinal uuid — the
/// roster (id-table) order is then deterministic, so journey row
/// indices name stable rules (the element-address discipline).
fn seed_armed_rule(state: &mut AppState, task: TaskId, ordinal: u128) -> ReminderRuleId {
    use chrono::Timelike;
    let id = ReminderRuleId::from_uuid(uuid::Uuid::from_u128(ordinal));
    let fire =
        chrono::Utc::now().with_nanosecond(0).expect("whole second") + chrono::Duration::hours(24);
    state.reminders.rules.insert(
        id,
        ReminderRule {
            insistence: Insistence::Normal,
            id,
            task,
            kind: ReminderKind::At(
                taskboard_domain::AtInstant::whole_second(fire).expect("whole second"),
            ),
            created_at: fire,
            updated_at: fire,
            by: None,
        },
    );
    id
}

/// Seeds an UNARMED rule (a due-anchored offset over a task with no
/// due date — the D-3 jump arm's row).
fn seed_unarmed_rule(state: &mut AppState, task: TaskId, ordinal: u128) -> ReminderRuleId {
    let id = ReminderRuleId::from_uuid(uuid::Uuid::from_u128(ordinal));
    let now = chrono::Utc::now();
    state.reminders.rules.insert(
        id,
        ReminderRule {
            insistence: Insistence::Normal,
            id,
            task,
            kind: ReminderKind::DueOffset(ReminderOffset::at_due()),
            created_at: now,
            updated_at: now,
            by: None,
        },
    );
    id
}

/// The fixture's first board card — the detail the shared opener
/// mounts (the title is the element address).
fn first_card(state: &AppState) -> TaskId {
    *state
        .tasks
        .iter()
        .find(|(_, task)| task.title == "task-0")
        .map(|(id, _)| id)
        .expect("the fixture's first card")
}

/// The emphasized rule row: the index whose model carries the
/// selected bit (the card-emphasis oracle's row twin).
fn selected_rule_row(harness: &JourneyHarness) -> Option<usize> {
    let rules = harness.handle.window().get_task_model().rules;
    (0..rules.row_count()).find(|row| rules.row_data(*row).expect("row").selected)
}

/// US-6/AC-1 (J3 US-6, round 9 chunk 12; plan tokens
/// `us6_s1_rule_cursor_walks_the_roster` + the fold's walk half) —
/// the disarmed vertical stroke arms the rule cursor at row 0
/// (arm-then-step), further strokes walk with edge clamps, the
/// emphasis follows the cursor through the row model, and the whole
/// walk dispatches NOTHING (the presentation-state fold's boundary
/// face: no cursor-bearing command variant ever reaches the engine).
#[test]
fn us6_s1_rule_cursor_walks_the_roster() {
    golden_state_journey(|mut state| {
        let task = first_card(&state);
        seed_armed_rule(&mut state, task, 0xA11);
        seed_armed_rule(&mut state, task, 0xA12);
        seed_armed_rule(&mut state, task, 0xA13);
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);
        assert_eq!(
            selected_rule_row(&harness),
            None,
            "a fresh detail is disarmed"
        );
        // The first vertical stroke arms at row 0.
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        assert_eq!(
            selected_rule_row(&harness),
            Some(0),
            "the arming stroke lands row 0 (arm-then-step)"
        );
        // Down down walks to the last row; a further stroke clamps.
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        assert_eq!(selected_rule_row(&harness), Some(2));
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        assert_eq!(
            selected_rule_row(&harness),
            Some(2),
            "the last row clamps (no wrap)"
        );
        // Up walks back; row 0 clamps the same way.
        harness.press_preset(preset, ActionId::NavFocusUp);
        harness.advance();
        harness.press_preset(preset, ActionId::NavFocusUp);
        harness.advance();
        assert_eq!(selected_rule_row(&harness), Some(0));
        harness.press_preset(preset, ActionId::NavFocusUp);
        harness.advance();
        assert_eq!(
            selected_rule_row(&harness),
            Some(0),
            "row 0 clamps (no wrap)"
        );
        // The walk is view-internal: zero commands (the fold's
        // boundary face — no cursor-bearing variant exists to send).
        harness.assert_commands(&[]);
    });
}

/// US-6 (J3 US-6, round 9 chunk 12; plan token
/// `us6_s2_activate_acts_on_the_selected_rule`) — Activate over the
/// armed cursor dispatches the cursor'd row's OWN affordance: an
/// armed rule row's Enter removes exactly that rule (the typed
/// command at the fake-engine boundary), and the cursor prunes on
/// the post-remove pass (the roster changed under it). Disarmed, the
/// same stroke falls through untouched (the detail's Activate
/// grammar is chunk 9's, byte-identical).
#[test]
fn us6_s2_activate_acts_on_the_selected_rule() {
    golden_state_journey(|mut state| {
        let task = first_card(&state);
        let rule = seed_armed_rule(&mut state, task, 0xB21);
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);
        // Arm, then Enter: the remove dispatches for the cursor'd rule.
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        harness.handle.dispatch_action(ActionId::TaskEdit);
        harness.advance();
        harness.assert_commands(&[StateCommand::RemoveReminderRule { rule }]);
        // The engine applied the removal; the settle's refresh pass
        // prunes the dead cursor (D-2: the verb does not clear it,
        // the roster change does).
        harness.engine.settle(&harness.handle);
        assert_eq!(
            selected_rule_row(&harness),
            None,
            "the dead cursor pruned after the roster changed"
        );
        assert_eq!(
            harness.handle.window().get_task_model().rules.row_count(),
            0,
            "the rule is gone"
        );
    });
}

/// US-6 (J3 US-6, round 9 chunk 12; plan token
/// `us6_s2_activate_acts_on_the_selected_rule`, the unarmed leg) —
/// Activate over a cursor'd UNARMED row opens the due editor (the
/// row's jump affordance — the D-3 armed-bit resolution).
#[test]
fn us6_s2_activate_on_an_unarmed_row_opens_the_due_editor() {
    golden_state_journey(|mut state| {
        let task = first_card(&state);
        seed_unarmed_rule(&mut state, task, 0xB31);
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        assert_eq!(selected_rule_row(&harness), Some(0));
        harness.handle.dispatch_action(ActionId::TaskEdit);
        harness.advance();
        assert!(
            harness.handle.window().get_task_model().date_picker_open,
            "the unarmed row's Activate opened the due editor (the jump arm)"
        );
        harness.assert_commands(&[]);
    });
}

/// US-6 (J3 US-6, round 9 chunk 12; plan token
/// `us6_s3_foreign_settles_and_exits_clear_the_cursor`) — the
/// lifecycle: view exit clears (disarmed on re-entry), a foreign
/// settle (the labels overlay) clears, and a subject change starts a
/// fresh roster (the new detail is disarmed even though the old
/// subject's arm preceded it).
#[test]
fn us6_s3_foreign_settles_and_exits_clear_the_cursor() {
    golden_state_journey(|mut state| {
        let task = first_card(&state);
        seed_armed_rule(&mut state, task, 0xC41);
        seed_armed_rule(&mut state, task, 0xC42);
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        assert!(selected_rule_row(&harness).is_some(), "armed");

        // View exit clears: re-entry starts disarmed (D-2 site 1).
        // The board focus is restored to the exited card, so the
        // reopen is the detail open alone (no focus move — the
        // restored anchor already names the ruled task).
        harness.handle.dispatch_action(ActionId::NavFocusBoard);
        harness.advance();
        harness.handle.dispatch_action(ActionId::TaskEdit);
        harness.advance();
        assert_eq!(
            selected_rule_row(&harness),
            None,
            "the re-opened detail is disarmed"
        );
        assert!(
            harness.handle.window().get_task_model().rules.row_count() > 0,
            "the re-opened detail shows the subject's rules"
        );

        // A foreign settle clears (D-2 site 4, the labels overlay).
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        assert!(selected_rule_row(&harness).is_some(), "armed again");
        harness.handle.dispatch_action(ActionId::LabelManage);
        harness.advance();
        assert_eq!(
            selected_rule_row(&harness),
            None,
            "the labels overlay's settle cleared the cursor"
        );
    });
}

/// US-6 (J3 US-6, round 9 chunk 12; plan token
/// `us6_s3_foreign_settles_and_exits_clear_the_cursor`, the subject
/// leg) — opening a DIFFERENT task's detail is a new roster: the arm
/// never carries across subjects.
#[test]
fn us6_s3_a_subject_change_starts_a_fresh_roster() {
    golden_state_journey(|mut state| {
        let task = first_card(&state);
        seed_armed_rule(&mut state, task, 0xC51);
        let other = *state
            .tasks
            .iter()
            .find(|(_, task)| task.title == "task-1")
            .map(|(id, _)| id)
            .expect("the fixture's second card");
        seed_armed_rule(&mut state, other, 0xC52);
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        assert!(selected_rule_row(&harness).is_some(), "armed on task-0");
        // Leave to the board, walk to the second card, open it.
        harness.handle.dispatch_action(ActionId::NavFocusBoard);
        harness.advance();
        harness.press_preset(preset, ActionId::NavFocusDown);
        harness.advance();
        harness.press_preset(preset, ActionId::TaskEdit);
        harness.advance();
        assert_eq!(
            harness.handle.window().get_task_model().title,
            "task-1",
            "the second card's detail is open"
        );
        assert_eq!(
            selected_rule_row(&harness),
            None,
            "the arm never carried across subjects"
        );
    });
}

/// US-6 (J3 US-6, round 9 chunk 12; plan token
/// `us6_s4_pointer_click_moves_the_cursor`) — pointer coherence: a
/// row click moves the cursor onto that row (emphasis follows), and
/// the following Activate dispatches for the CLICKED row's rule.
#[test]
fn us6_s4_pointer_click_moves_the_cursor() {
    golden_state_journey(|mut state| {
        let task = first_card(&state);
        seed_armed_rule(&mut state, task, 0xD61);
        // The clicked row: row 1 in the id-table roster (D61 < D62).
        let clicked = seed_armed_rule(&mut state, task, 0xD62);
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);
        // The pointer path: the row body's click seam (row 1).
        harness.handle.window().invoke_task_rule_row_clicked(1);
        harness.advance();
        assert_eq!(
            selected_rule_row(&harness),
            Some(1),
            "the click moved the cursor onto the clicked row"
        );
        // The following activation acts on the clicked row's rule.
        harness.handle.dispatch_action(ActionId::TaskEdit);
        harness.advance();
        // The activation removed the CLICKED row's rule (the
        // fixed-uuid seeding makes the roster order deterministic).
        harness.assert_commands(&[StateCommand::RemoveReminderRule { rule: clicked }]);
    });
}

/// The chunk-9 negative (J3 US-6, round 9 chunk 12; plan token
/// `us6_n1_disarmed_taskdetail_vertical_stays_the_ladder`) — with an
/// EMPTY rule roster the vertical strokes never arm: the control
/// ladder steps exactly as chunk 9 pinned (the walk-stop fact
/// moves), and the view-internal grammar holds (zero commands).
#[test]
fn us6_n1_disarmed_taskdetail_vertical_stays_the_ladder() {
    golden_state_journey(|state| {
        let preset = Preset::Emacs;
        let mut harness = JourneyHarness::mount(state, preset);
        open_detail_first_card(&mut harness, preset);
        assert_eq!(
            selected_rule_row(&harness),
            None,
            "no rules, no cursor (the empty-roster gate)"
        );
        // The vertical strokes ride the control ladder (the chunk-9
        // pinned grammar): the walk-stop fact moves off 0.
        let mut guard = 0;
        while walk_stop(&harness) == 0 && guard < 10 {
            harness.press_preset(preset, ActionId::NavFocusDown);
            harness.advance();
            guard += 1;
        }
        assert_ne!(
            walk_stop(&harness),
            0,
            "the disarmed verticals step the control ladder exactly as chunk 9 pinned"
        );
        assert_eq!(
            selected_rule_row(&harness),
            None,
            "the empty roster never armed"
        );
        harness.assert_commands(&[]);
    });
}
