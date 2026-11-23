//! The habit-surface journeys (chunk D3a: the 2026-11-22
//! habit-surface journeys UX plan's US-1..US-5, wiring rows
//! W-1..W-8 + W-16). Assertion discipline is the journeys suite's:
//! terminal state + the typed commands that reached the fake-engine
//! boundary; labels are addresses, never text oracles; every refusal
//! is a typed outcome, never a message substring.

// SPDX-License-Identifier: AGPL-3.0-only

use crate::support;

use support::driver::JourneyDriver as _;
use support::fixture;
use support::harness::Harness;

use slint::ComponentHandle as _;
use slint::Model as _;
use taskboard_domain::habit::{HabitClocks, HabitLogEntry, HabitLogKey, WindowStart};
use taskboard_domain::{AppState, HabitId, NonZeroCount, PeriodKind, StateCommand};
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::keymap::stroke::{self, Stroke};
use taskboard_ui_slint::{UiSettings, View};

/// The journeys' habit fixture (the lib fixture's twin): one live
/// daily habit (quota 2) on the fixture board, at the pinned instant.
fn state_habit_at(
    state: &mut AppState,
    board: taskboard_domain::BoardId,
    order: i64,
    title: &str,
    quota: u32,
) -> HabitId {
    let habit = taskboard_domain::habit::Habit {
        id: HabitId::from_uuid(uuid::Uuid::now_v7()),
        board,
        title: title.to_owned(),
        description: String::new(),
        target: taskboard_domain::HabitTarget::PerDay(NonZeroCount::new(quota).unwrap()),
        order,
        anchor_time: None,
        anchor_day: None,
        scheduled_days: None,
        reminder: None,
        clocks: HabitClocks::all(fixture::PINNED_NOW),
        continued_from: None,
        deleted: false,
        paused_until: None,
        by: None,
    };
    let id = habit.id;
    state.habits.habits.insert(habit.id, habit);
    id
}

fn state_habit(state: &mut AppState, board: taskboard_domain::BoardId) -> HabitId {
    state_habit_at(state, board, 0, "water", 2)
}

/// Writes `count` into the habit's current day window (the log-row
/// fixture shape: the whole-second clock, the LWW guard's law).
fn log_today(state: &mut AppState, habit: HabitId, count: u32) {
    let today = fixture::PINNED_NOW.date_naive();
    state.habits.log.insert(
        HabitLogKey {
            habit,
            window: WindowStart::of_period(PeriodKind::Day, today),
        },
        HabitLogEntry {
            count,
            clock: fixture::PINNED_NOW,
            by: None,
            skip: false,
        },
    );
}

fn keyboard(harness: &Harness) -> taskboard_ui_slint::KeyboardState<'_> {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::KeyboardState>()
}

/// The journeys' preamble: the Today posture with the render clock
/// frozen at the fixture's pinned instant (the clock law: the habit
/// fixtures anchor their log windows at `PINNED_NOW`).
fn run<R>(
    state: taskboard_domain::AppState,
    kiosk: bool,
    job: impl FnOnce(&mut Harness) -> R + Send + 'static,
) -> R
where
    R: Send + 'static,
{
    support::harness::run_journey_no_escape_with_settings(
        state,
        UiSettings {
            kiosk,
            clock: Some(fixture::PINNED_NOW),
            ..UiSettings::default()
        },
        job,
    )
}

fn parse(spelling: &str) -> Stroke {
    stroke::parse(spelling).unwrap_or_else(|e| panic!("{spelling} parses: {e}"))
}

fn habit_card_field(
    harness: &Harness,
    index: usize,
    read: impl Fn(&taskboard_ui_slint::TaskCardModel) -> bool,
) -> bool {
    let model = harness.handle.window().get_habits_model();
    read(&model.row_data(index).expect("habit row").card)
}

// -- US-1: the card is self-contained; the strip goes; target fills
//    the slot (W-1/W-2/W-3) ------------------------------------------

/// US-1/AC-1 (W-3): the detached strip's redundant Log habit button
/// is gone — the structural presence fold over the section's a11y
/// tree — and the card's done slot IS the remaining +1 affordance
/// (the landed seam: clicking it logs the habit).
#[test]
fn us1_s1_log_button_removed_checkbox_logs() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        harness.driver.advance_time(0);
        assert!(
            harness.driver.find_all_by_label("Log habit").is_empty(),
            "the detached strip's Log habit button is gone"
        );
        // The done slot routes the log (the landed seam, still wired).
        let checkbox = harness
            .driver
            .find_by_label("Mark task done")
            .expect("the card's done checkbox renders");
        checkbox.invoke_default_action();
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::LogHabit { .. }] => {}
            other => panic!("the done slot logs the habit, got {other:?}"),
        }
    });
}

/// US-1/AC-2 (W-1): the undo-log and repair affordances sit ON the
/// card line (the trailing slot) — the geometry fold: the undo
/// button's rect is inside the habit card's rect, not on a detached
/// strip row below it.
#[test]
fn us1_s2_undo_repair_in_card_trailing() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = state_habit(&mut state, board);
    log_today(&mut state, habit, 1);
    run(state, false, move |harness| {
        harness.driver.advance_time(0);
        let undo = harness
            .driver
            .find_by_label("Take one habit log back")
            .expect("the undo-log affordance renders");
        let card = harness
            .driver
            .find_by_label("water")
            .expect("the habit card renders");
        let (ux, uy) = (undo.absolute_position().x, undo.absolute_position().y);
        let (cx, cy) = (card.absolute_position().x, card.absolute_position().y);
        let size = card.size();
        assert!(
            ux >= cx && ux <= cx + size.width && uy >= cy && uy <= cy + size.height,
            "the undo-log button rides the card line: button ({ux},{uy}) inside card ({cx},{cy}, {}x{})",
            size.width,
            size.height,
        );
    });
}

/// US-1/AC-2 (W-1, the command leg): the trailing undo-log button
/// dispatches the exact `AdjustHabitLog` of the landed typed undo
/// side (T-23) over the row's current window.
#[test]
fn us1_s3_undo_log_dispatches_adjust() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = state_habit(&mut state, board);
    log_today(&mut state, habit, 1);
    run(state, false, move |harness| {
        harness.driver.advance_time(0);
        harness.driver.click_by_label("Take one habit log back");
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::AdjustHabitLog { count: 0, .. }] => {}
            other => panic!("the trailing undo routes the exact adjust, got {other:?}"),
        }
    });
}

/// US-1/AC-3 (W-2): the done slot carries the typed target-met bit —
/// the met window fills the card's done visual (`card.done`), an
/// unmet one keeps it empty. Window-aware by construction: the bit
/// derives from the same `met` fold the progress chip reads.
#[test]
fn us1_s4_target_renders_filled_slot() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let met = state_habit_at(&mut state, board, 0, "met habit", 2);
    log_today(&mut state, met, 2);
    let unmet = state_habit_at(&mut state, board, 1, "unmet habit", 2);
    log_today(&mut state, unmet, 1);
    run(state, false, move |harness| {
        harness.driver.advance_time(0);
        assert!(
            habit_card_field(harness, 0, |card| card.done),
            "the met window fills the done slot"
        );
        assert!(
            !habit_card_field(harness, 1, |card| card.done),
            "the unmet window keeps the done slot empty"
        );
    });
}

/// US-1/AC-4 (W-3, the keyboard twins survive the move): the landed
/// undo/repair chords still route over the focused row — the strip's
/// removal costs no keyboard path.
#[test]
fn us1_s5_keyboard_paths_survive_the_move() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = state_habit(&mut state, board);
    log_today(&mut state, habit, 1);
    run(state, false, move |harness| {
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::NavLastItem);
        harness.driver.advance_time(0);
        harness.driver.press_stroke(parse("Ctrl+Shift+u"));
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::AdjustHabitLog { .. }] => {}
            other => panic!("the undo chord routes over the focused row, got {other:?}"),
        }
    });
}

// -- US-2: the double-click opens the editor (W-4) ------------------

/// US-2/AC-1 (W-4): the card's double-click shell routes
/// `habit_edit_asked` → `ActionId::HabitEdit` → `UiEffect::HabitEdit`:
/// the shared detail surface mounts the habit variant; nothing
/// dispatches (the opener is a view intent).
#[test]
fn us2_s1_double_click_opens_habit_editor() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        let window = harness.handle.window();
        harness.driver.advance_time(0);
        window.invoke_habits_edit_asked(0);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_current_view(),
            View::TaskDetail,
            "the editor mounts the habit variant"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "opening the editor dispatches nothing"
        );
    });
}

/// US-2/AC-2 (W-4, the kiosk gate): the double-click dispatch on
/// kiosk is the landed typed refusal — no editor mounts, nothing
/// reaches the boundary.
#[test]
fn us2_s2_kiosk_double_click_typed_refusal() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, true, move |harness| {
        let window = harness.handle.window();
        harness.driver.advance_time(0);
        window.invoke_habits_edit_asked(0);
        harness.driver.advance_time(0);
        assert_ne!(
            window.get_current_view(),
            View::TaskDetail,
            "the editor never mounts on kiosk"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the kiosk dispatch sends nothing"
        );
    });
}

/// US-2/AC-3 (W-4, the A1 tripwire): a single click selects only —
/// the emphasis ring paints, no editor mounts (the gesture conflicts
/// A1 fixed must not reintroduce).
#[test]
fn us2_s3_single_click_selects_only() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        let window = harness.handle.window();
        harness.driver.advance_time(0);
        window.invoke_habits_row_activated(0);
        harness.driver.advance_time(0);
        assert!(
            habit_card_field(harness, 0, |card| card.emphasis),
            "the click seats the walk cursor (A1)"
        );
        assert_ne!(
            window.get_current_view(),
            View::TaskDetail,
            "the single click never mounts the editor"
        );
    });
}

// -- US-3: the keyboard remainder (W-5/W-6/W-16) --------------------

/// US-3/AC-1 (W-6, the walk regression leg): with tasks AND habits on
/// the day, stepping the walk down past the last task row lands the
/// cursor on the first habit row (one selection truth — A1's grammar).
#[test]
fn us3_s1_walk_crosses_habit_rows() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_two_habits(&mut state, board);
    run(state, false, move |harness| {
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::NavLastItem);
        harness.driver.advance_time(0);
        let emphasis = |harness: &Harness| -> Vec<bool> {
            (0..2)
                .map(|i| habit_card_field(harness, i, |card| card.emphasis))
                .collect()
        };
        assert_eq!(
            emphasis(harness),
            vec![false, true],
            "the walk's tail is the last habit row"
        );
        harness.handle.dispatch_action(ActionId::NavPrevItem);
        harness.driver.advance_time(0);
        assert_eq!(
            emphasis(harness),
            vec![true, false],
            "stepping back up crosses into the habit rows"
        );
    });
}

fn state_two_habits(state: &mut AppState, board: taskboard_domain::BoardId) {
    state_habit_at(state, board, 0, "water", 2);
    state_habit_at(state, board, 10, "stretch", 2);
}

/// US-3/AC-2 (W-16): Return over a focused habit row opens the habit
/// editor (the shared activation row — the `TaskEdit` parity twin); the
/// fall-through keeps the task landing's editor on the same stroke.
#[test]
fn us3_s2_enter_activates_habit_editor() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_two_habits(&mut state, board);
    run(state, false, move |harness| {
        let window = harness.handle.window();
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::NavLastItem);
        harness.driver.advance_time(0);
        harness.driver.press_stroke(parse("Return"));
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_current_view(),
            View::TaskDetail,
            "Return over the habit landing opens the habit editor"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "opening the editor dispatches nothing"
        );
    });
}

/// US-3/AC-3 (W-5): the new `Ctrl+Shift+l` preset chord routes the
/// exact `LogHabit` for the focused row's current window — the R-5
/// supersession, the `HabitQuotaEdit` key's focused-row precedent.
#[test]
fn us3_s3_log_chord_resolves_over_focused_row() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_two_habits(&mut state, board);
    run(state, false, move |harness| {
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::NavLastItem);
        harness.driver.advance_time(0);
        harness.driver.press_stroke(parse("Ctrl+Shift+l"));
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::LogHabit { .. }] => {}
            other => panic!("the log chord routes over the focused row, got {other:?}"),
        }
    });
}

/// US-3/AC-4 (W-5, the refusal leg): a habit-target chord without a
/// focused habit row stays on the landed typed `NoTarget` path —
/// nothing dispatches, the refusal is the outcome.
#[test]
fn us3_s4_no_focus_typed_refusal() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitLog);
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the subjectless chord refuses typed, nothing dispatches"
        );
    });
}

/// US-3/AC-5 (W-6, the row-menu regression leg): the landed `RowMenu`
/// chord over a focused habit row opens the habit context menu —
/// click ≡ key by construction.
#[test]
fn us3_s5_row_menu_opens_habit_menu() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::NavLastItem);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::RowMenu);
        harness.driver.advance_time(0);
        let keyboard = keyboard(harness);
        assert!(keyboard.get_menu_open(), "the row menu opened");
        let entries = keyboard.get_menu_entries();
        let actions: Vec<String> = (0..entries.row_count())
            .map(|i| entries.row_data(i).expect("row").action.to_string())
            .collect();
        assert!(
            actions.iter().any(|a| a == "habit.log"),
            "the habit menu opened over the focused row: {actions:?}"
        );
    });
}

// -- US-4: the pause editor dismisses (W-7) -------------------------

/// US-4/AC-1 (W-7): Escape dismisses the mounted pause editor through
/// the global escape/cancel preset — the editor unmounts, nothing
/// dispatches (cancel writes nothing).
#[test]
fn us4_s1_escape_cancels_pause_editor() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        let window = harness.handle.window();
        harness.driver.advance_time(0);
        window.invoke_habits_pause_asked(0);
        harness.driver.advance_time(0);
        assert!(window.get_habits_pause_open(), "the pause editor mounted");
        for _ in 0..3 {
            if !window.get_habits_pause_open() {
                break;
            }
            harness.driver.press_stroke(parse("Escape"));
            harness.driver.advance_time(0);
        }
        assert!(
            !window.get_habits_pause_open(),
            "Escape dismisses the pause editor"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the dismissal writes nothing"
        );
    });
}

/// US-4/AC-2 (W-7): a pointer-down outside the editor dismisses it
/// clean — the light-dismiss strips band the published rect and route
/// the same cancel; nothing writes.
#[test]
fn us4_s2_outside_click_cancels_clean() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        let window = harness.handle.window();
        harness.driver.advance_time(0);
        window.invoke_habits_pause_asked(0);
        harness.driver.advance_time(0);
        assert!(window.get_habits_pause_open(), "the pause editor mounted");
        let facts = window.global::<taskboard_ui_slint::HabitPauseFacts>();
        let (ex, ey, ew, eh) = (
            facts.get_anchor_x(),
            facts.get_anchor_y(),
            facts.get_anchor_width(),
            facts.get_anchor_height(),
        );
        assert!(ew > 0.0 && eh > 0.0, "the editor published its rect");
        // Below the editor band, within the view (the below-strip).
        harness.driver.click_at(ex + 30.0, ey + eh + 30.0);
        harness.driver.advance_time(0);
        assert!(
            !window.get_habits_pause_open(),
            "the outside pointer-down dismisses the editor"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the dismissal writes nothing"
        );
    });
}

/// US-4/AC-3 (W-7, the commit leg): on a fresh mount the landed
/// commit path is unregressed — a typed date commits `PauseHabit`.
#[test]
fn us4_s3_commit_path_unregressed() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        let window = harness.handle.window();
        harness.driver.advance_time(0);
        window.invoke_habits_pause_asked(0);
        harness.driver.advance_time(0);
        window.invoke_habits_pause_until_picked("2999-12-31".into());
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::PauseHabit { until: Some(_), .. }] => {}
            other => panic!("the commit path routes the pause, got {other:?}"),
        }
    });
}

// -- US-5: the typed quota seed (W-8) -------------------------------

/// US-5/AC-1+2 (W-8): the popover's seed travels the typed path — a
/// quota above the draft property's i32 domain seeds the clamped max
/// (documented at the seam), never the silent floor of 1.
#[test]
fn us5_s1_seed_clamps_typed_no_floor() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    // The clamp leg: a quota the draft property cannot represent.
    state_habit_at(&mut state, board, 0, "huge", u32::MAX);
    run(state, false, move |harness| {
        let window = harness.handle.window();
        harness.driver.advance_time(0);
        window.invoke_habits_quota_edit_asked(0);
        harness.driver.advance_time(0);
        assert!(
            window.get_habits_quota_open(),
            "the popover mounted over the row"
        );
        assert_eq!(
            window.get_habits_quota_count(),
            i32::MAX,
            "the seed clamps at the draft domain's max — never the silent 1"
        );
    });
}

/// US-5/AC-3 (W-8, the commit leg): the seeded popover's commit
/// routes the landed `UpdateHabit` path.
#[test]
fn us5_s2_quota_commit_routes_update_habit() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        let window = harness.handle.window();
        harness.driver.advance_time(0);
        window.invoke_habits_quota_edit_asked(0);
        harness.driver.advance_time(0);
        window.invoke_habits_quota_committed();
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::UpdateHabit { .. }] => {}
            other => panic!("the commit routes the update, got {other:?}"),
        }
    });
}

// -- the kiosk posture (the plan's platform scope) ------------------

/// The plan's kiosk scope (US-1/S2 platform note): the affordances
/// render disabled, the editor surfaces stay unmountable, and the
/// dispatch-time gates hold — nothing reaches the engine.
#[test]
fn kiosk_gates_the_redesigned_card() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, true, move |harness| {
        let window = harness.handle.window();
        harness.driver.advance_time(0);
        // The editor opener's kiosk gate holds: no editor mounts,
        // nothing reaches the boundary.
        harness.handle.dispatch_action(ActionId::HabitEdit);
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the kiosk dispatch is structurally inert"
        );
        // The quota popover's opener is equally inert (the R-3 gate).
        window.invoke_habits_quota_edit_asked(0);
        harness.driver.advance_time(0);
        assert!(
            !window.get_habits_quota_open(),
            "the popover never mounts on kiosk"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the kiosk dispatch stays inert"
        );
    });
}
