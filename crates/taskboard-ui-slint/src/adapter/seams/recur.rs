// SPDX-License-Identifier: AGPL-3.0-only

use super::Adapter;
use super::settle_pointer_conflict;
use crate::adapter::install_seam;
use slint::ComponentHandle as _;
use std::cell::RefCell;
use std::rc::Rc;

use crate::AppWindow;
use crate::actions::ActionId;

/// The recur row's + the habits trio's seams (issue #506, plan
/// D-7/D-8): every new interactive element rides the blanket rule —
/// the hover tip is the button's label, the walk stop is the widget's
/// own focus machinery, and the dispatch goes through the action
/// layer (`route`) from a one-shot target the seam resolved from the
/// same row source the sections render.
#[allow(clippy::too_many_lines)] // one seam table; splitting hides the trio
pub(in crate::adapter) fn install_recur_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    // The make-up valve: the exact `(habit, window, count)` from the
    // row source — what you click is what increments.
    install_seam!(window: window.on_habits_repair_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    else {
        return; // a stale index degrades to a no-op
    };
    let state = a.state.load();
    let Some(habit_row) = state.habits.habits.get(&habit) else {
        return;
    };
    let Some((target_window, count)) = crate::presentation::habit_repair_target(
        habit_row,
        &state.habits.log,
        a.day_ctx().now,
        a.calendar(),
    ) else {
        return; // the affordance armed against stale state
    };
    a.habit_repair = Some((habit, target_window, count));
    // The row click refreshes the focus fact (cross-surface friction
    // plan D-2, #832): click ≡ key composes both directions.
    a.focused_habit = Some(habit);
    drop(state);
    a.route(&window, ActionId::HabitRepair);

    });

    // The habit row's focus fact (cross-surface friction plan D-2,
    // #832; reworked by #1200 chunk A1 D2): the row activation is the
    // task pointer path's shape — the pointer conflict settles first
    // (the `today_row_activated` discipline), the id resolves through
    // the tables, the walk cursor seats via
    // `ViewState::activate_habit_row` (the exclusivity law's landing
    // write; the emphasis ring repaints through the outcomes), and
    // the menu verbs' subject mints as before. Click ≡ key in both
    // directions: the keyboard truth composes with the pointer truth
    // (one selection cursor, never two). Stale index absorbed (the
    // notification-row guard shape); no dispatch, no pass — selection
    // only (habits have no click-to-open verb, H4 reserved).
    install_seam!(window: window.on_habits_row_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let _ = &window; // the fact mutates no window bit
        settle_pointer_conflict(&mut a, &window);
        if let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    {
        a.focused_habit = Some(habit);
        let outcomes = a.view_state.activate_habit_row(habit);
        a.apply_outcomes(&window, &outcomes);
    }

    });

    // The habit row's double-click editor opener (D3a W-4, #1201):
    // the task pointer path's activation shape — the conflict settles,
    // the walk cursor seats (single-click's selection truth is the
    // prefix of the double-click's, US-2/S3 composes), then the
    // `HabitEdit` opener routes over the minted subject. The kiosk
    // gate is the landed dispatch-time refusal (the UiEffect arm),
    // never a second seam-side copy. Stale index absorbed.
    install_seam!(window: window.on_habits_edit_asked, adapter: adapter; // slint: habits-edit-asked
    bind mut (a, window), args (index), {        let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    else {
        return;
    };
    settle_pointer_conflict(&mut a, &window);
    a.focused_habit = Some(habit);
    let outcomes = a.view_state.activate_habit_row(habit);
    a.apply_outcomes(&window, &outcomes);
    a.route(&window, ActionId::HabitEdit);

    });

    // The habit list's gap drop (chunk 8 D5, US-4/AC-1 — the #918
    // board pattern over the flat list): the delegate-resolved gap
    // index is the one-shot verbatim (#1198 chunk A1 D1 — the UI
    // mints target indices only; the engine clamps and resolves the
    // orders) and routes `HabitMove` — the same dispatch seam the
    // menu rows speak. A stale index or gap is absorbed (the pointer
    // seams' posture); kiosk absorbs silently (the editing verb's
    // HAB-14 gate — the slint side never arms, the adapter doubles
    // the gate).
    // slint: habits-drop-asked
    install_seam!(window: window.on_habits_drop_asked, adapter: adapter;
    bind mut (a, window), args (row, gap), {        let _ = &window;
        if a.kiosk {
            return;
        }
        let (Ok(row), Ok(gap)) = (usize::try_from(row), usize::try_from(gap)) else {
            return;
        };
        let Some(&habit) = a.tables.habits.get(row) else {
            return;
        };
        let state = a.state.load_full();
        let Some(habit_row) = state.habits.habits.get(&habit) else {
            return;
        };
        if !habit_row.is_live() {
            return;
        }
        drop(state);
        // A gap past the end is the append slot — the engine clamps.
        a.habit_move = Some((habit, gap));
        a.route(&window, ActionId::HabitMove);
    });

    // The habit row's context menu (H2 D4/B1): the right-click seam —
    // focus mints inside the opener (the row activation's discipline),
    // the curated page opens anchored at the pointer. Stale index
    // absorbed (the notification-row guard shape).
    install_seam!(window: window.on_habits_context_requested, adapter: adapter; // slint callback: habits-context-requested (the habit context menu's right-click seam, H2 W-2)
    bind mut (a, window), args (index, x, y), {        let Ok(index) = usize::try_from(index) else {
        return;
    };
    a.open_habit_context_menu(&window, x, y, index);

    });

    // The recur row: pause open (view intent), the editor's typed
    // commit (the trust boundary parses the date strictly), resume,
    // and the conversion's arm/confirm/cancel.
    install_seam!(window: window.on_task_recur_pause_asked, adapter: adapter;
    bind mut (a, window), args (), {        a
        .route(&window, ActionId::SeriesPause);

    });
    install_seam!(window: window.on_task_recur_pause_accepted, adapter: adapter;
    bind mut (a, window), args (text), {        // The typed date is the trust boundary: strict `%Y-%m-%d`, no
    // leniency — an unparseable draft dispatches nothing.
    let Ok(date) = chrono::NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d") else {
        return;
    };
    a.pause_draft = Some(date);
    a.recur_pause_open = false;
    a.route(&window, ActionId::SeriesPauseCommit);

    });
    install_seam!(window: window.on_task_recur_pause_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        a.recur_pause_open = false;
    a.pause_draft = None;
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_task_recur_resume_clicked, adapter: adapter;
    bind mut (a, window), args (), {        let Some(task) = a.view_state.subject_task() else {
        return;
    };
    a.series_target = Some(task);
    a.route(&window, ActionId::SeriesResume);

    });
    install_seam!(window: window.on_task_recur_convert_asked, adapter: adapter;
    // The arming is the shared helper (cross-surface friction plan
    // D-2, #832) — identicality with the menu-routed effect arm.
    bind mut (a, window), args (), {        let Some(task) = a.view_state.subject_task() else {
        return;
    };
    if !a.arm_series_convert(task) {
        return;
    }
    a.route(&window, ActionId::SeriesConvertArm);

    });
    install_seam!(window: window.on_task_recur_convert_confirmed, adapter: adapter;
    bind mut (a, window), args (), {        let Some(task) = a.view_state.subject_task() else {
        return;
    };
    // The quota draft re-derives from the current rule (the arm's
    // draft may be stale against a mid-confirm pattern edit).
    let pattern = {
        let state = a.state.load();
        let Some(rule) = state.tasks.get(&task).and_then(|t| t.recur.as_ref()) else {
            return;
        };
        crate::presentation::convert_target_default(&rule.pattern)
    };
    a.series_target = Some(task);
    a.convert_target = Some(pattern);
    a.recur_confirm_armed = false;
    a.route(&window, ActionId::SeriesConvertToHabit);

    });
    install_seam!(window: window.on_task_recur_convert_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        a.recur_confirm_armed = false;
    a.series_target = None;
    a.convert_target = None;
    a.refresh_forced(&window);

    });
}
/// The recurrence editor's seams (round-7 D-7.1): the pattern/bound
/// picks, the field mirrors (invalid parse — the trust boundary
/// never trusts the widget), and the detach strip routes its
/// two-step through the action layer.
#[allow(clippy::too_many_lines)] // one seam table; splitting hides the editor
pub(in crate::adapter) fn install_recur_editor_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    use crate::level_state::{MonthlyMode, RecurBoundKind, RecurPatternKind};

    // The affordances' convergence: the add row, the recur row's edit
    // button, and the Ctrl+R binding all land here (click ≡ key).
    install_seam!(window: window.on_series_edit_asked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::SeriesEdit);

    });

    // The pattern/bound/mode picks: the pushed index contracts
    // (0/1/2, 0/1) map to the draft kinds; an out-of-range index is a
    // stale widget degrading to a no-op.
    install_seam!(window: window.on_recur_pattern_picked, adapter: adapter;
    bind mut (a, window), args (index), {        let kind = match index {
            0 => RecurPatternKind::Daily,
            1 => RecurPatternKind::Weekly,
            2 => RecurPatternKind::Monthly,
            _ => return, // a stale index degrades to a no-op
        };
        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.pattern = kind;
        }
        a.set_recur_draft_invalid(&window, false);
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_monthly_mode_picked, adapter: adapter;
    bind mut (a, window), args (index), {        let mode = match index {
            0 => MonthlyMode::OnDay,
            1 => MonthlyMode::NthWeekday,
            _ => return, // a stale index degrades to a no-op
        };
        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.monthly_mode = mode;
        }
        a.set_recur_draft_invalid(&window, false);
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_bound_picked, adapter: adapter;
    bind mut (a, window), args (index), {        let bound = match index {
            0 => RecurBoundKind::Never,
            1 => RecurBoundKind::Until,
            2 => RecurBoundKind::Count,
            _ => return, // a stale index degrades to a no-op
        };
        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.bound = bound;
        }
        a.set_recur_draft_invalid(&window, false);
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_monthly_weekday_picked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(slot) = usize::try_from(index).ok() else {
            return;
        };
        if slot > 6 {
            return; // a stale index degrades to a no-op
        }
        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.monthly_nth_weekday = crate::presentation::weekday_of_slot(slot);
        }
        a.set_recur_draft_invalid(&window, false);
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_weekday_toggled, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(slot) = usize::try_from(index).ok() else {
            return;
        };
        if slot > 6 {
            return; // a stale index degrades to a no-op
        }
        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.weekdays[slot] = !editor.weekdays[slot];
        }
        a.set_recur_draft_invalid(&window, false);
        a.refresh_forced(&window);

    });

    // The text mirrors: the typed text lands in the buffer verbatim
    // (no trim — the parsers are the funnel's business).
    install_seam!(window: window.on_recur_every_days_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.every_days = text.to_string();
        }
        a.set_recur_draft_invalid(&window, false);
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_weekly_interval_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.weekly_interval = text.to_string();
        }
        a.set_recur_draft_invalid(&window, false);
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_monthly_day_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.monthly_day = text.to_string();
        }
        a.set_recur_draft_invalid(&window, false);
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_monthly_nth_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.monthly_nth = text.to_string();
        }
        a.set_recur_draft_invalid(&window, false);
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_until_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.until_text = text.to_string();
        }
        a.set_recur_draft_invalid(&window, false);
        a.refresh_forced(&window);

    });
    install_seam!(window: window.on_recur_count_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(editor) = a.view_state.recur_editor_mut() {
            editor.count = text.to_string();
        }
        a.set_recur_draft_invalid(&window, false);
        a.refresh_forced(&window);

    });

    // The commit funnel (plan D-C1): re-mint the whole buffer; an
    // invalid draft is a silent no-op (the button does not even mount
    // — the seam re-checks, the trust boundary never trusts the
    // widget); the done-subject re-check is the assembly
    // precondition's second gate.
    install_seam!(window: window.on_recur_commit_clicked, adapter: adapter;
    bind mut (a, window), args (), {        let Some(task) = a
            .view_state
            .subject_task()
            .filter(|task| a.state.load().tasks.get(task).is_some_and(taskboard_domain::Task::is_open))
        else {
            return;
        };
        let draft = a.view_state.recur_editor().cloned();
        let Some(draft) = draft else {
            return;
        };
        let now = a.day_ctx().now;
        let day = a.calendar();
        let head = a.state.load().tasks.get(&task).cloned();
        let Some(head) = head else {
            return;
        };
        let Ok(rule) = crate::presentation::assemble_recur_draft(&draft, &head, day, now) else {
            // The refusal is surfaced (#1221): the editor stays open
            // and the invalid flag rides the mirror (the caption's
            // bound) — the no-op dispatch contract is untouched.
            a.set_recur_draft_invalid(&window, true);
            return; // an invalid draft dispatches nothing
        };
        a.set_recur_draft_invalid(&window, false);
        a.recur_edit_target = Some(task);
        a.recur_draft_rule = Some(rule);
        a.route(&window, ActionId::SeriesEditCommit);
        // The commit's consume is the flow's end — the level drops
        // with its buffer, the one-shots were taken by the route.
        let outcomes = a.view_state.end_edit_recur();
        a.apply_outcomes(&window, &outcomes);

    });

    // The clean close: the cancel button, the scrim click, and
    // Escape's law all land here — the draft dies with the level,
    // nothing dispatches.
    install_seam!(window: window.on_recur_editor_cancel_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.recur_edit_target = None;
        a.recur_draft_rule = None;
        a.set_recur_draft_invalid(&window, false);
        let outcomes = a.view_state.end_edit_recur();
        a.apply_outcomes(&window, &outcomes);

    });

    // The detach strip's two-step (plan D-C1): the arm/cancel route
    // the flag effects; the confirm routes the detach command over
    // the opener's one-shot target and ends the flow.
    install_seam!(window: window.on_recur_detach_arm_clicked, adapter: adapter;
    bind mut (a, window), args (), {        // route() takes every one-shot; the strip's steps re-arm
        // the target from the flow (the convert seams' law) so the
        // confirm still routes.
        a.recur_edit_target = a.view_state.task_of_edit_recur();
        a.route(&window, ActionId::SeriesEditDetachArm);

    });
    install_seam!(window: window.on_recur_detach_cancel_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.recur_edit_target = a.view_state.task_of_edit_recur();
        a.route(&window, ActionId::SeriesEditDetachCancel);

    });
    install_seam!(window: window.on_recur_detach_confirm_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.recur_edit_target = a.view_state.task_of_edit_recur();
        a.route(&window, ActionId::SeriesEditDetach);
        // The detach's consume is the flow's end (the commit's shape).
        let outcomes = a.view_state.end_edit_recur();
        a.apply_outcomes(&window, &outcomes);

    });
}
