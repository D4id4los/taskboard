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
        // #1286: the slot's a11y address is the habit verb, not the
        // task verb — the label is the corpus's address (the address
        // changed with the fix; the pin's shape is unchanged).
        let checkbox = harness
            .driver
            .find_by_label("Log habit +1")
            .expect("the card's done checkbox renders the habit verb");
        checkbox.invoke_default_action();
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::LogHabit { .. }] => {}
            other => panic!("the done slot logs the habit, got {other:?}"),
        }
        // The task verb is not on the habit surface (#1286).
        assert!(
            harness.driver.find_by_label("Mark task done").is_none(),
            "the habit row does not carry the task done verb"
        );
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

// -- US-6: the ruled blank-time semantics (#1212, W-9) --------------

/// The editor-mount preamble for the US-6 tests: the fixture habit on
/// the Today card, the habit editor variant mounted.
fn us6_open_editor(harness: &mut support::harness::Harness) {
    let window = harness.handle.window();
    window.invoke_habits_row_activated(0);
    harness.driver.advance_time(0);
    harness.handle.dispatch_action(ActionId::HabitEdit);
    harness.driver.advance_time(0);
}

/// US-6/S1 (W-9, #1212's ruled semantics — operator ruling comment
/// 7079171625): a blank time field with valid base/offset/snooze mints
/// an override carrying NO time — the anchor-time-only posture. The
/// retired 09:00 fallback never reappears at the boundary.
#[test]
fn us6_s1_blank_mints_time_none() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = state_habit(&mut state, board);
    run(state, false, move |harness| {
        us6_open_editor(harness);
        let window = harness.handle.window();
        window.invoke_habit_reminder_base_picked(1);
        window.invoke_habit_reminder_offset_edited("1".into());
        window.invoke_habit_reminder_snooze_edited("2h".into());
        harness.driver.advance_time(0);
        window.invoke_habit_save_clicked();
        harness.driver.advance_time(0);
        let ovr = taskboard_domain::habit::HabitReminderOverride {
            base: taskboard_domain::habit::NudgeBase::PeriodStart,
            offset_days: 1,
            time: None,
            snooze: taskboard_domain::habit::HabitSnooze::Hours(2),
        };
        match harness.engine.drain().as_slice() {
            [
                StateCommand::UpdateHabit {
                    habit: edited,
                    changes,
                },
            ] => {
                assert_eq!(*edited, habit);
                assert_eq!(
                    changes.reminder,
                    Some(Some(ovr)),
                    "blank time mints the anchor-time-only override"
                );
            }
            other => panic!("the blank-time save routes the update, got {other:?}"),
        }
    });
}

/// US-6/S2 (W-9): a named time still mints `Some` — the optional
/// shape never lost the concrete leg.
#[test]
fn us6_s2_named_time_mints_some() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = state_habit(&mut state, board);
    run(state, false, move |harness| {
        us6_open_editor(harness);
        let window = harness.handle.window();
        window.invoke_habit_reminder_base_picked(1);
        window.invoke_habit_reminder_offset_edited("1".into());
        window.invoke_habit_reminder_time_edited("07:30".into());
        window.invoke_habit_reminder_snooze_edited("2h".into());
        harness.driver.advance_time(0);
        window.invoke_habit_save_clicked();
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [
                StateCommand::UpdateHabit {
                    habit: edited,
                    changes,
                },
            ] => {
                assert_eq!(*edited, habit);
                match changes.reminder {
                    Some(Some(ovr)) => {
                        assert_eq!(
                            ovr.time,
                            Some(chrono::NaiveTime::from_hms_opt(7, 30, 0).unwrap()),
                            "a named time mints Some"
                        );
                    }
                    other => panic!("the named time mints the override, got {other:?}"),
                }
            }
            other => panic!("the named-time save routes the update, got {other:?}"),
        }
    });
}

/// US-6/S3 (W-9, the regression oracle): the clear leg still routes
/// the `Some(None)` set — retiring the mint never touched it.
#[test]
fn us6_s3_clear_leg_unregressed() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = state_habit(&mut state, board);
    let reminder = taskboard_domain::habit::HabitReminderOverride {
        base: taskboard_domain::habit::NudgeBase::PeriodStart,
        offset_days: 1,
        time: Some(chrono::NaiveTime::from_hms_opt(7, 30, 0).unwrap()),
        snooze: taskboard_domain::habit::HabitSnooze::Hours(2),
    };
    state
        .habits
        .habits
        .get_mut(&habit)
        .expect("fixture habit")
        .reminder = Some(reminder);
    run(state, false, move |harness| {
        us6_open_editor(harness);
        let window = harness.handle.window();
        window.invoke_habit_reminder_clear_clicked();
        harness.driver.advance_time(0);
        window.invoke_habit_save_clicked();
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [
                StateCommand::UpdateHabit {
                    habit: edited,
                    changes,
                },
            ] => {
                assert_eq!(*edited, habit);
                assert_eq!(
                    changes.reminder,
                    Some(None),
                    "the clear leg routes the Some(None) set"
                );
            }
            other => panic!("the clear save routes the update, got {other:?}"),
        }
    });
}

/// US-6/S4 (W-9, the typed save gate): a non-blank unparseable time
/// refuses the mint — the gate greys, no `UpdateHabit` reaches the
/// boundary.
#[test]
fn us6_s4_save_gate_refuses_invalid_drafts() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        us6_open_editor(harness);
        let window = harness.handle.window();
        window.invoke_habit_reminder_offset_edited("1".into());
        window.invoke_habit_reminder_time_edited("not-a-time".into());
        window.invoke_habit_reminder_snooze_edited("2h".into());
        harness.driver.advance_time(0);
        window.invoke_habit_save_clicked();
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the unparseable time is a typed invalid draft: no dispatch"
        );
    });
}

// -- US-7: the tombstoned habit's history entry (#1218, W-10) --------

/// The palette preamble: opens the jump-to palette and sets the query
/// (the rows arrive through the landed push pass).
fn us7_open_palette(harness: &mut support::harness::Harness, query: &str) {
    let window = harness.handle.window();
    harness.handle.dispatch_action(ActionId::PaletteSearch);
    harness.driver.advance_time(0);
    window.invoke_palette_query_edited(query.into());
    harness.driver.advance_time(0);
}

/// The palette row index whose row IS the named title's habit row
/// (matched by address; never by rendered text oracle).
fn us7_habit_row_index(harness: &support::harness::Harness, title: &str) -> usize {
    let model = harness.handle.window().get_palette_model();
    (0..model.row_count())
        .find(|i| {
            model
                .row_data(*i)
                .is_some_and(|row| row.history && row.text.as_str() == title)
        })
        .unwrap_or_else(|| panic!("the habit row for {title} mounts in the palette"))
}

/// US-7/AC-1 (W-10, #1218): the search surface lists the tombstoned
/// habit as a result row carrying the typed tombstone bit (no
/// live-row state forged — the row's only verb is History).
#[test]
fn us7_s1_search_mounts_tombstone_history_row() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let mut ghost = taskboard_domain::habit::Habit {
        id: HabitId::from_uuid(uuid::Uuid::now_v7()),
        board,
        title: "ghost".to_owned(),
        description: String::new(),
        target: taskboard_domain::HabitTarget::PerDay(NonZeroCount::new(1).unwrap()),
        order: 1,
        anchor_time: None,
        anchor_day: None,
        scheduled_days: None,
        reminder: None,
        clocks: HabitClocks::all(fixture::PINNED_NOW),
        continued_from: None,
        deleted: true,
        paused_until: None,
        by: None,
    };
    ghost.clocks = HabitClocks::all(fixture::PINNED_NOW);
    state.habits.habits.insert(ghost.id, ghost);
    run(state, false, move |harness| {
        us7_open_palette(harness, "ghost");
        let model = harness.handle.window().get_palette_model();
        let index = us7_habit_row_index(harness, "ghost");
        let row = model.row_data(index).expect("the tombstone row");
        assert!(row.tombstone, "the tombstone bit rides the row, typed");
        assert!(row.history, "the row carries the History affordance bit");
    });
}

/// US-7/AC-2 (W-10): activating the tombstoned row's History
/// affordance mounts the history dialog through the id-exact
/// (tombstone-matching) resolution and projects the retained log.
#[test]
fn us7_s2_dialog_projects_tombstoned_log() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let ghost = HabitId::from_uuid(uuid::Uuid::now_v7());
    state.habits.habits.insert(
        ghost,
        taskboard_domain::habit::Habit {
            id: ghost,
            board,
            title: "ghost".to_owned(),
            description: String::new(),
            target: taskboard_domain::HabitTarget::PerDay(NonZeroCount::new(1).unwrap()),
            order: 1,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(fixture::PINNED_NOW),
            continued_from: None,
            deleted: true,
            paused_until: None,
            by: None,
        },
    );
    // The retained log (the "log outlives" fact): one window row the
    // tombstone outlived.
    log_today(&mut state, ghost, 2);
    run(state, false, move |harness| {
        us7_open_palette(harness, "ghost");
        let window = harness.handle.window();
        let index = us7_habit_row_index(harness, "ghost");
        window.invoke_palette_row_activated(i32::try_from(index).expect("row index"));
        harness.driver.advance_time(0);
        assert!(
            window.get_habit_history_open(),
            "the dialog mounts over the tombstoned subject"
        );
        assert!(
            window.get_habit_history().stats.adherence_met > 0,
            "the retained log projects into the dialog's windows"
        );
    });
}

/// US-7/AC-3 (W-10): cancelling the dialog on a tombstoned subject
/// writes nothing (the read-role surface).
#[test]
fn us7_s3_dialog_cancel_writes_nothing() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let ghost = HabitId::from_uuid(uuid::Uuid::now_v7());
    state.habits.habits.insert(
        ghost,
        taskboard_domain::habit::Habit {
            id: ghost,
            board,
            title: "ghost".to_owned(),
            description: String::new(),
            target: taskboard_domain::HabitTarget::PerDay(NonZeroCount::new(1).unwrap()),
            order: 1,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(fixture::PINNED_NOW),
            continued_from: None,
            deleted: true,
            paused_until: None,
            by: None,
        },
    );
    run(state, false, move |harness| {
        us7_open_palette(harness, "ghost");
        let window = harness.handle.window();
        let index = us7_habit_row_index(harness, "ghost");
        window.invoke_palette_row_activated(i32::try_from(index).expect("row index"));
        harness.driver.advance_time(0);
        window.invoke_habit_history_close_requested();
        harness.driver.advance_time(0);
        assert!(
            !window.get_habit_history_open(),
            "the cancel unmounts the dialog"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the dialog's cancel is zero engine state change"
        );
    });
}

/// US-7/AC-4 (W-10, the regression oracle): a live habit's history
/// opens from search identically to the landed live-row mounts.
#[test]
fn us7_s4_live_row_history_unregressed() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit_at(&mut state, board, 0, "water", 2);
    run(state, false, move |harness| {
        us7_open_palette(harness, "water");
        let window = harness.handle.window();
        let index = us7_habit_row_index(harness, "water");
        window.invoke_palette_row_activated(i32::try_from(index).expect("row index"));
        harness.driver.advance_time(0);
        assert!(
            window.get_habit_history_open(),
            "the live row's History affordance mounts the same dialog"
        );
        window.invoke_habit_history_close_requested();
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the read-role round trip writes nothing"
        );
    });
}

// -- US-11: the push-side kiosk belt (#1049, W-15) ------------------

/// US-11/AC-1 (W-15, #1049): the kiosk rig's composed belt posture —
/// no habit-variant detail exists at any point on the push path: the
/// dispatch-time gates are the first line (the editor affordances
/// refuse), and the view never leaves the board root (the belt's
/// terminal-state contract at the rig boundary; the belt mechanism's
/// arm itself is pinned at the `view_state` unit layer — the posture it
/// repairs is unreachable through the gates by design).
#[test]
fn us11_s1_kiosk_push_steps_view_back() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, true, move |harness| {
        let window = harness.handle.window();
        harness.driver.advance_time(0);
        // The push path's entry points: the row seam and the action.
        window.invoke_habits_edit_asked(0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitEdit);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_current_view(),
            View::Board,
            "the kiosk rig never carries a habit-variant detail root"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the kiosk push path writes nothing"
        );
    });
}

/// US-11/AC-2 (W-15, #1049): the desktop rig is unregressed — the
/// same entry points mount the editor (the belt is desktop-inert).
#[test]
fn us11_s2_desktop_push_unregressed() {
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
            "the desktop editor mounts (the belt never fires)"
        );
        window.invoke_habit_editor_cancelled();
        harness.driver.advance_time(0);
        assert_eq!(window.get_current_view(), View::Board);
        assert!(
            harness.engine.drain().is_empty(),
            "the open/cancel round trip writes nothing"
        );
    });
}

// -- US-9: the nudge-schedule preview (#1035, W-13) -----------------

/// US-9/AC-1 (W-13, #1035): a valid draft projects the derived
/// schedule — the preview mounts display-only (no dispatch) with the
/// drafted clock time named (the typed time-set bit).
#[test]
fn us9_s1_preview_projects_derived_schedule() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        us6_open_editor(harness);
        let window = harness.handle.window();
        window.invoke_habit_reminder_offset_edited("1".into());
        window.invoke_habit_reminder_time_edited("07:30".into());
        window.invoke_habit_reminder_snooze_edited("2h".into());
        harness.handle.refresh();
        harness.driver.advance_time(0);
        let preview = window.get_habit_model().nudge_preview;
        assert!(preview.visible, "a valid draft shows the preview");
        assert!(preview.time_set, "the named time is the drafted clock time");
        assert!(
            harness.engine.drain().is_empty(),
            "the preview is a pure read: no dispatch"
        );
    });
}

/// US-9/AC-2 (W-13): a refused draft hides the preview — the neutral
/// state, nothing projected.
#[test]
fn us9_s2_invalid_draft_hides_preview() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        us6_open_editor(harness);
        let window = harness.handle.window();
        window.invoke_habit_reminder_snooze_edited("0h".into());
        harness.handle.refresh();
        harness.driver.advance_time(0);
        let preview = window.get_habit_model().nudge_preview;
        assert!(
            !preview.visible,
            "the refused mint hides the preview (the neutral state)"
        );
    });
}

/// US-9/AC-3 (W-13 + #1212): the blank-time draft projects the
/// anchor-time-only posture — the preview mounts with the typed
/// time-unset bit, never an invented clock time.
#[test]
fn us9_s3_blank_time_preview_phrases_anchor() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    run(state, false, move |harness| {
        us6_open_editor(harness);
        let window = harness.handle.window();
        window.invoke_habit_reminder_offset_edited("1".into());
        window.invoke_habit_reminder_snooze_edited("2h".into());
        harness.handle.refresh();
        harness.driver.advance_time(0);
        let preview = window.get_habit_model().nudge_preview;
        assert!(preview.visible, "the blank-time draft is valid");
        assert!(
            !preview.time_set,
            "blank time is the anchor-time-only posture: no clock time"
        );
    });
}

// -- US-8: the per-day heatmap cells (#1144, W-11/W-12) --------------

/// Seeds a Week-kind habit plus per-day overlay facts (the pinned
/// month is 2026-11: 30 days; `PINNED_NOW` is 2026-11-06).
fn us8_state(board: taskboard_domain::BoardId) -> (AppState, HabitId) {
    let (_fixture, mut state) = fixture::journey_state();
    let habit = taskboard_domain::habit::Habit {
        id: HabitId::from_uuid(uuid::Uuid::now_v7()),
        board,
        title: "garden".to_owned(),
        description: String::new(),
        target: taskboard_domain::HabitTarget::PerWeek(NonZeroCount::new(3).unwrap()),
        order: 2,
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
    state.habits.habits.insert(id, habit);
    // Two overlay facts: the 3rd and the 5th carry activity; the rest
    // of the month carries none.
    for (day, count) in [(3u32, 2u32), (5, 1)] {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 11, day).expect("calendar date");
        state.habits.day_log.insert(
            taskboard_domain::HabitDayKey {
                habit: id,
                day: date,
            },
            taskboard_domain::HabitDayLogEntry {
                count,
                clock: fixture::PINNED_NOW,
            },
        );
    }
    (state, id)
}

/// US-8/AC-1 (W-12): the week-kind habit's month grid renders one
/// cell per calendar DAY from the overlay — the 3rd shows 2, the 5th
/// shows 1, and a day without a fact renders empty (no invention).
#[test]
fn us8_s1_day_cells_render_overlay_facts() {
    let board = *fixture::journey_state()
        .1
        .boards
        .keys()
        .next()
        .expect("fixture board");
    let (state, _habit) = us8_state(board);
    run(state, false, move |harness| {
        us7_open_palette(harness, "garden");
        let window = harness.handle.window();
        let index = us7_habit_row_index(harness, "garden");
        window.invoke_palette_row_activated(i32::try_from(index).expect("row index"));
        harness.driver.advance_time(0);
        assert!(window.get_habit_history_open(), "the dialog mounts");
        let cells = window.get_habit_history().cells;
        assert_eq!(cells.row_count(), 30, "November renders one cell per DAY");
        let cell = |day: usize| cells.row_data(day - 1).expect("cell");
        assert_eq!(cell(3).count, 2, "the 3rd shows its overlay count");
        assert_eq!(cell(5).count, 1, "the 5th shows its overlay count");
        let empty = cell(10);
        assert_eq!(empty.count, 0, "a day with no fact renders empty");
    });
}

/// US-8/AC-3 (W-12, R-2): the stats oracle stays window-canonical —
/// overlay facts alone never credit adherence.
#[test]
fn us8_s2_window_adherence_oracle_unregressed() {
    let board = *fixture::journey_state()
        .1
        .boards
        .keys()
        .next()
        .expect("fixture board");
    let (state, _habit) = us8_state(board);
    run(state, false, move |harness| {
        us7_open_palette(harness, "garden");
        let window = harness.handle.window();
        let index = us7_habit_row_index(harness, "garden");
        window.invoke_palette_row_activated(i32::try_from(index).expect("row index"));
        harness.driver.advance_time(0);
        let numbers = window.get_habit_history().stats;
        assert_eq!(
            numbers.adherence_met, 0,
            "day overlay facts never feed the window-canonical oracle"
        );
    });
}

// -- US-10: the editor mirrors onto the shared flows (#1048, W-14) ----

/// US-10/AC-1 (W-14): the habit variant's title edits route through
/// the shared `Flow::EditTitle` over the habit subject — the flow's
/// own sparse `UpdateHabit` carries the commit (the draft's private
/// mirror field is gone).
#[test]
fn us10_s1_habit_variant_mounts_shared_editors() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = state_habit(&mut state, board);
    run(state, false, move |harness| {
        us6_open_editor(harness);
        let window = harness.handle.window();
        window.invoke_habit_title_edited("oceanside".into());
        harness.driver.advance_time(0);
        window.invoke_habit_save_clicked();
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [
                StateCommand::UpdateHabit {
                    habit: edited,
                    changes,
                },
            ] => {
                assert_eq!(*edited, habit);
                assert_eq!(
                    changes.title.as_deref(),
                    Some("oceanside"),
                    "the shared flow's commit carries the title"
                );
            }
            other => panic!("the title flow routes the update, got {other:?}"),
        }
    });
}

/// US-10/AC-2 (W-14): the description edits route through the shared
/// `Flow::EditDescription` over the habit subject, same grammar.
#[test]
fn us10_s2_habit_description_flow_mounts_shared_surface() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = state_habit(&mut state, board);
    run(state, false, move |harness| {
        us6_open_editor(harness);
        let window = harness.handle.window();
        window.invoke_habit_description_edited("deep water only".into());
        harness.driver.advance_time(0);
        window.invoke_habit_save_clicked();
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [
                StateCommand::UpdateHabit {
                    habit: edited,
                    changes,
                },
            ] => {
                assert_eq!(*edited, habit);
                assert_eq!(
                    changes.description.as_deref(),
                    Some("deep water only"),
                    "the shared flow's commit carries the description"
                );
            }
            other => panic!("the description flow routes the update, got {other:?}"),
        }
    });
}

/// US-10/AC-3 (W-14, the regression oracle): the task variant's title
/// flow rides the same widened grammar and still routes the task's
/// own `UpdateTask` — one flow grammar, two subjects.
#[test]
fn us10_s3_task_flows_unregressed() {
    // The pinned-clock seeding: the Today table reads the frozen
    // render clock, so the fixture's overdue row must be derived from
    // the same instant (the clock law's fixture-anchor tool).
    let (_fixture, state) = fixture::journey_state_at(fixture::PINNED_NOW);
    run(state, false, move |harness| {
        let window = harness.handle.window();
        // Focus a today row (click ≡ key's focus half; section 0 row 0
        // is the fixture's overdue task), then open the detail.
        window.invoke_today_row_activated(0, 0);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_current_view(),
            View::TaskDetail,
            "the task detail mounts"
        );
        harness.handle.dispatch_action(ActionId::TaskTitleEdit);
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_current_view(),
            View::TaskDetail,
            "the task detail mounts"
        );
        window.invoke_task_title_committed("rewritten".into());
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::UpdateTask { changes, .. }] => {
                assert_eq!(
                    changes.title.as_deref(),
                    Some("rewritten"),
                    "the task variant's flow is unregressed"
                );
            }
            other => panic!("the task title flow routes the update, got {other:?}"),
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
