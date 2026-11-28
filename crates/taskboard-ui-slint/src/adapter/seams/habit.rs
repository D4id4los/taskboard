// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle as _;

use crate::AppWindow;
use crate::actions::ActionId;

use super::Adapter;
use crate::adapter::install_seam;

/// The −1 recovery's one-shot target (UX R-2 D-3, #786): the row's
/// current window and count from the same row source the section
/// renders — what you click is what decrements. `None` when count 0
/// (the route's typed `NoTarget`) or against stale state.
pub(in crate::adapter) fn habit_undo_target(
    adapter: &Adapter,
    habit: taskboard_domain::HabitId,
) -> Option<(
    taskboard_domain::HabitId,
    taskboard_domain::WindowStart,
    u32,
)> {
    let state = adapter.state.load();
    let habit_row = state.habits.habits.get(&habit)?;
    let now = adapter.day_ctx().now;
    let window = taskboard_domain::WindowStart::of_period(
        habit_row.target.kind(),
        adapter.calendar().today(now),
    );
    // The chunk-5 re-key (#1519 D-1): the −1 reads the derived count
    // of the current canonical window.
    let count = taskboard_domain::habit_event::derive_window_counts(
        habit_row,
        state.habits.events.values(),
        adapter.calendar(),
    )
    .get(&window)
    .copied()
    .unwrap_or(0);
    (count >= 1).then_some((habit, window, count))
}

/// Whether `habit` holds an open time entry (the running timer) —
/// `send_habit_track_toggle`'s router fact read over the published
/// state (display derives, never caches).
pub(in crate::adapter) fn habit_open_entry(
    state: &taskboard_domain::AppState,
    habit: taskboard_domain::HabitId,
) -> Option<taskboard_domain::ids::HabitTimeEntryId> {
    state
        .habits
        .time_log
        .values()
        .find(|entry| entry.habit == habit && entry.end.is_none())
        .map(|entry| entry.id)
}

/// The habits section's seams (DR-6/6, D-P13; H2 reworked): the +1
/// resolves the `IdTables.habits` index (stale index → no-op, the
/// notification-row guard shape) and routes `habit.log`; the −1
/// resolves the row's current window (the `habit_undo_target` above)
/// and routes `habit.unlog`; the right-click opens the habit context
/// menu (the reorder pair and the conversion live there now — the
/// slots minted at menu open carry gap indices; the engine renumbers
/// dense, no midpoint arithmetic anywhere).
#[allow(clippy::too_many_lines)] // one seam block per wired callback
pub(in crate::adapter) fn install_habit_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_habits_log_clicked, adapter: adapter; // slint callback: habits-log-clicked (the done slot ≡ strip +1 shared seam, H2 W-1)
    bind mut (a, window), args (index), {        let Some(habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale index degrades to a no-op
    };
    a.habit_target = Some(habit);
    // The row click refreshes the focus fact (cross-surface friction
    // plan D-2, #832): click ≡ key composes both directions.
    a.focused_habit = Some(habit);
    a.route(&window, ActionId::HabitLog);

    });

    // The card play/pause control's routing (journey plan 2026-11-24
    // W-1, US-30/S1, #1305 — `send_card_track_toggle`'s twin): the seam
    // only addresses the target (the one-shot `habit_target`) and
    // routes `HabitTrackToggle` — the state read and the Start/Stop
    // outcome choice live in the router arm (one semantics, one home).
    install_seam!(window: window.on_habits_track_asked, adapter: adapter; // slint: habits-track-asked
    bind mut (a, window), args (index), {        let Some(habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale index degrades to a no-op
    };
    a.habit_target = Some(habit);
    a.focused_habit = Some(habit);
    a.route(&window, ActionId::HabitTrackToggle);

    });

    // The −1 recovery (UX R-2 D-3, #786): the row's current window
    // and count from the same row source the section renders — what
    // you click is what decrements. Count 0 sets no target: the
    // route's typed `NoTarget` (nothing dispatches, no negative log
    // exists). Never quota-enforced (D-P14 symmetrically).
    install_seam!(window: window.on_habits_undo_log_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    else {
        return; // a stale index degrades to a no-op
    };
    a.habit_undo = habit_undo_target(&a, habit);
    // The row click refreshes the focus fact (cross-surface friction
    // plan D-2, #832): click ≡ key composes both directions.
    a.focused_habit = Some(habit);
    if a.habit_undo.is_some() {
        a.route(&window, ActionId::HabitUndoLog);
    }

    });

    // The row seam's reorder pair retires with the row's vertical
    // button stack (H2 D3/D5): reorder lives in the habit context
    // menu now (the `habit.move-up`/`habit.move-down` ids, slots
    // minted at menu open by `habit_menu_entries`); the pointer
    // reorder parity is H3's drag/keys replacement.

    // The create flow (round-7 D-7.2): the header ask routes the
    // toggle intent; the quota keystrokes ride the per-keystroke
    // validity pass; the confirm seam is the trust boundary (board
    // resolved fresh at commit, the quota through the mint); the
    // cancel hits the shared disarm funnel.
    install_seam!(window: window.on_habits_create_asked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::HabitCreate);

    });
    // The HF1 skip/pause/resume seams (W-1/W-3): the row affordances
    // resolve the `IdTables.habits` index (stale index → no-op, the
    // notification-row guard shape); the pause editor's commit is the
    // trust boundary (strict %Y-%m-%d, no leniency — the recur-pause
    // seam's convention); cancel writes nothing (the destructive-role
    // dismissal law).
    install_seam!(window: window.on_habits_skip_asked, adapter: adapter; // slint: habits-skip-asked
    bind mut (a, window), args (index), {        let Some(habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.habit_target = Some(habit);
    a.focused_habit = Some(habit);
    a.route(&window, ActionId::HabitSkip);

    });
    install_seam!(window: window.on_habits_pause_asked, adapter: adapter; // slint: habits-pause-asked
    bind mut (a, window), args (index), {        let Some(habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.focused_habit = Some(habit);
    a.habit_pause_target = Some(habit);
    a.habit_pause_open = true;
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_habits_resume_asked, adapter: adapter; // slint: habits-resume-asked
    bind mut (a, window), args (index), {        let Some(habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.focused_habit = Some(habit);
    a.habit_pause_target = Some(habit);
    a.route(&window, ActionId::HabitResume);

    });
    install_seam!(window: window.on_habits_pause_until_picked, adapter: adapter; // slint: habits-pause-until-picked
    bind mut (a, window), args (text), {        let Ok(date) =
        chrono::NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
    else {
        return; // an unparseable draft dispatches nothing
    };
    // The draft gate (US-2/S2's escape leg; the quota-validity
    // posture): a through-date before today covers nothing and reads
    // as a stale badge — the draft is refused here (typed notice, the
    // editor stays open) AND the planner still validates
    // (`InvalidHabitPause` remains authoritative across a rollover).
    if date < a.calendar().today(a.day_ctx().now) {
        a.notice = Some(crate::adapter::Notice {
            text: crate::presentation::habit_pause_invalid_phrase(),
            until: chrono::Utc::now() + crate::presentation::MENU_NOTICE_HORIZON,
        });
        a.push_notice(&window);
        return;
    }
    a.pause_draft = Some(date);
    a.habit_pause_open = false;
    a.route(&window, ActionId::HabitPauseCommit);

    });
    install_seam!(window: window.on_habits_pause_cancelled, adapter: adapter; // slint: habits-pause-cancelled
    bind mut (a, window), args (), {        a.disarm_habit_pause(&window);

    });
    install_seam!(window: window.on_habits_create_quota_edited, adapter: adapter;
    bind mut (a, window), args (text), {        a.habit_create_quota = text.to_string();
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_habits_create_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        a.disarm_habit_create(&window);

    });
    install_seam!(window: window.on_habits_create_confirmed, adapter: adapter; [else ( false)]
    bind mut (a, window), args (title, quota_text, target_index), {        // The board resolves fresh at commit — never cached at arm
    // (what you see is what the habit lands on) and ahead of every
    // field refusal: a board that went away under the armed form
    // disarms it silently, whatever the fields hold (the stale-arm
    // law owns the seam's first word).
    // Round-6 A1 D-4 (2026-11-24, #1301): the today view is
    // board-free — the target is the FIRST live board (canonical
    // order, the seed law's default); the picker is #1314's territory.
    let state = a.state.load();
    let Some(board) = taskboard_domain::order::live_boards(&state)
        .first()
        .map(|board| board.id)
    else {
        drop(state);
        a.disarm_habit_create(&window);
        return false;
    };
    drop(state);
    // A whitespace-empty title dispatches nothing (D-P14): the
    // raw text is never rewritten, an empty one never reaches the
    // engine.
    let title = title.trim();
    if title.is_empty() {
        return false;
    }
    // The quota is the trust boundary's mint: zero, junk, and
    // overflow all refuse structurally — an absorbed miss keeps the
    // text (the rule-add seam's return convention). Empty text is
    // the field's default: it commits as 1.
    let Ok(quota_count) = crate::presentation::parse_habit_quota(&quota_text) else {
        return false;
    };
    // The target index is structurally bounded by the
    // SegmentedControl, but the seam is total: out of range refuses.
    let target = match target_index {
        0 => taskboard_domain::HabitTarget::PerDay(quota_count),
        1 => taskboard_domain::HabitTarget::PerWeek(quota_count),
        2 => taskboard_domain::HabitTarget::PerMonth(quota_count),
        _ => return false,
    };
    a.habit_create_draft = Some(crate::router::HabitCreateDraft {
        board,
        title: title.to_string(),
        target,
    });
    // The form closes on commit (the timers form's shape); the
    // routed dispatch consumes the one-shot draft. The property is
    // set here — `route`'s trailing sync is not a derive pass (the
    // convert strip's direct-set precedent).
    a.habit_create_open = false;
    a.habit_create_quota.clear();
    window.set_habits_create_open(false);
    a.route(&window, ActionId::HabitCreateCommit);
    true

    });

    // The inline quota edit's seams (W-5, #961): the phrase click
    // resolves the row and routes the same opener effect the key
    // routes (click ≡ key — one arming site); the stepper rungs edit
    // the adapter-held draft (floor 0 — the typed invalid draft); the
    // commit mints the one-shot at the trust boundary (the
    // `NonZeroCount` mint over the habit's CURRENT kind — the inline
    // edit is a one-number edit, the kind is locked); Escape/outside
    // (the host's dismissal) hits the disarm funnel.
    install_seam!(window: window.on_habits_quota_edit_asked, adapter: adapter; // slint: habits-quota-edit-asked
    bind mut (a, window), args (index), {        let Some(&habit) = a
        .tables
        .habits
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    else {
        return; // a stale index degrades to a no-op
    };
    // The row click refreshes the focus fact (cross-surface friction
    // plan D-2, #832): click ≡ key composes both directions. The
    // opener resolves the subject from this fact (one arming site).
    a.focused_habit = Some(habit);
    a.route(&window, ActionId::HabitQuotaEdit);

    });
    install_seam!(window: window.on_habits_quota_stepped, adapter: adapter; // slint: habits-quota-stepped
    bind mut (a, window), args (delta), {        // The stepper's floor is 0 — the typed invalid draft renders
    // (commit blocked); no upper clamp beyond i32 (the mint refuses
    // overflow at commit, and a u32 quota bounds the reachable
    // values).
    a.habit_quota_count = (a.habit_quota_count + delta).max(0);
    a.sync_quota_popover(&window);

    });
    install_seam!(window: window.on_habits_quota_committed, adapter: adapter; // slint: habits-quota-committed
    bind mut (a, window), args (), {        // The zero draft never routes (US-4/AC-3): the typed
    // invalid-draft state blocks the commit — the popover stays
    // open, the rungs remain the recovery.
    if a.habit_quota_count < 1 {
        return;
    }
    // The habit and its CURRENT kind resolve fresh at commit — never
    // cached at arm (what you see is what the habit lands on; the
    // kind lock reads the stored row, not the popover's stale echo).
    let Some(habit) = a.habit_edit_subject() else {
        a.disarm_habit_quota(&window);
        return;
    };
    let state = a.state.load();
    let Some(row) = state.habits.habits.get(&habit) else {
        drop(state);
        a.disarm_habit_quota(&window);
        return;
    };
    let Some(count) = u32::try_from(a.habit_quota_count).ok()
        .and_then(|count| taskboard_domain::NonZeroCount::new(count).ok())
    else {
        return; // unreachable past the guard above; the mint is total
    };
    let target = match row.target {
        taskboard_domain::HabitTarget::PerDay(_) => {
            taskboard_domain::HabitTarget::PerDay(count)
        }
        taskboard_domain::HabitTarget::PerWeek(_) => {
            taskboard_domain::HabitTarget::PerWeek(count)
        }
        taskboard_domain::HabitTarget::PerMonth(_) => {
            taskboard_domain::HabitTarget::PerMonth(count)
        }
    };
    drop(state);
    a.habit_quota_draft = Some(crate::router::HabitQuotaDraft { habit, target });
    // The popover closes on commit (the create form's shape); the
    // routed dispatch consumes the one-shot draft.
    window.set_habits_quota_open(false);
    a.route(&window, ActionId::HabitQuotaCommit);

    });
    install_seam!(window: window.on_habits_quota_dismissed, adapter: adapter; // slint: habits-quota-dismissed
    bind mut (a, window), args (), {        a.disarm_habit_quota(&window);

    });
}
/// The habit editor's seams (H4 D5): the draft mirrors, the per-kind
/// day row, and the save/cancel pair. Every mirror mutates only the
/// adapter-owned draft (no engine state — the commit computes the
/// sparse changeset); the validity flavours re-parse at the trust
/// boundary per edit (the count mints through `NonZeroCount`, the
/// month day through `MonthDay`, the time through the due editor's
/// lenient parser).
#[allow(clippy::too_many_lines)] // ten seams, one grammar
pub(in crate::adapter) fn install_habit_editor_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    // US-10 (W-14, #1048): the variant's title/description ride the
    // SHARED edit flows (Flow::EditTitle/EditDescription over
    // EditSubject::Habit) — the draft's private mirror fields are
    // gone. First keystroke arms the flow over the stored row (the
    // display-mirror rule); later keystrokes land in the buffer; the
    // settle rules (commit non-empty / discard empty) fire at the
    // editor's save (the staged-Save grammar's commit funnel).
    install_seam!(window: window.on_habit_title_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let subject = a.habit_edit_draft.as_ref().map(|draft| draft.habit);
        let armed = a.view_state.habit_edit_flow_armed(subject, false);
        if let Some(habit) = subject {
            if !armed {
                let state = a.state.load();
                let outcomes = a.view_state.begin_habit_title_edit(&state, habit);
                a.apply_outcomes(&window, &outcomes);
            }
            a.view_state.set_buffer(t.to_string());
        }
    });
    install_seam!(window: window.on_habit_description_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let subject = a.habit_edit_draft.as_ref().map(|draft| draft.habit);
        let armed = a.view_state.habit_edit_flow_armed(subject, true);
        if let Some(habit) = subject {
            if !armed {
                let state = a.state.load();
                let outcomes = a.view_state.begin_habit_description_edit(&state, habit);
                a.apply_outcomes(&window, &outcomes);
            }
            a.view_state.set_buffer(t.to_string());
        }
    });
    // The quota row: the kind re-seats the target (a now-mismatched
    // day is carried inert — the D5 decision), the count mints
    // per-keystroke (the create form's validity grammar).
    install_seam!(window: window.on_habit_quota_kind_picked, adapter: adapter;
    bind mut (a, window), args (index), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            let kind = match index {
                0 => taskboard_domain::PeriodKind::Day,
                1 => taskboard_domain::PeriodKind::Week,
                _ => taskboard_domain::PeriodKind::Month,
            };
            if let Some(count) = draft.parsed_count() {
                draft.target = match kind {
                    taskboard_domain::PeriodKind::Day => {
                        taskboard_domain::HabitTarget::PerDay(count)
                    }
                    taskboard_domain::PeriodKind::Week => {
                        taskboard_domain::HabitTarget::PerWeek(count)
                    }
                    taskboard_domain::PeriodKind::Month => {
                        taskboard_domain::HabitTarget::PerMonth(count)
                    }
                };
            }
        }
        // The draft moved outside the state Arc — the mirror pass
        // re-derives (#1432, the reminder-seam precedent): without it
        // the pushed `habit_model` (the segment's selected index, the
        // kind-gated rows) keeps the previous kind until an unrelated
        // pass happens to run.
        a.refresh_forced(&window);
    });
    install_seam!(window: window.on_habit_quota_count_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.quota_count = t.to_string();
            draft.quota_valid =
                draft.parsed_count().is_some() || draft.quota_count.trim().is_empty();
            // Re-seat the target from the fresh parse when it holds.
            if let Some(count) = draft.parsed_count() {
                draft.target = match draft.quota_kind_index() {
                    0 => taskboard_domain::HabitTarget::PerDay(count),
                    1 => taskboard_domain::HabitTarget::PerWeek(count),
                    _ => taskboard_domain::HabitTarget::PerMonth(count),
                };
            }
        }
    });
    install_seam!(window: window.on_habit_time_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.time_text = t.to_string();
            draft.time_edited = true;
            match crate::due_edit::parse_time_lenient(t.trim()) {
                Some(time) => {
                    draft.time_valid = true;
                    draft.anchor_time = Some(time);
                    draft.time_set = true;
                }
                None => {
                    draft.time_valid = false;
                }
            }
        }
    });
    install_seam!(window: window.on_habit_time_cleared, adapter: adapter;
    bind mut (a, window), args (), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.time_text.clear();
            draft.anchor_time = None;
            draft.time_set = false;
            draft.time_valid = true;
            draft.time_edited = true;
        }
    });
    // The 7-slot weekday picker (the PerWeek variant's day row).
    install_seam!(window: window.on_habit_weekday_picked, adapter: adapter;
    bind mut (a, window), args (index), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            let wd = match index {
                0 => chrono::Weekday::Mon,
                1 => chrono::Weekday::Tue,
                2 => chrono::Weekday::Wed,
                3 => chrono::Weekday::Thu,
                4 => chrono::Weekday::Fri,
                5 => chrono::Weekday::Sat,
                _ => chrono::Weekday::Sun,
            };
            draft.anchor_day = Some(taskboard_domain::habit::AnchorDay::Weekday(wd));
            draft.day_edited = true;
        }
    });
    // The 1..=31 day field (the PerMonth variant's row) — the mint
    // rejects 0 and past 31 (the type's own trust boundary).
    install_seam!(window: window.on_habit_month_day_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.month_day = t.to_string();
            draft.day_edited = true;
            match t
                .trim()
                .parse::<u8>()
                .ok()
                .and_then(|n| taskboard_domain::habit::MonthDay::new(n).ok())
            {
                Some(day) => {
                    draft.month_valid = true;
                    draft.anchor_day =
                        Some(taskboard_domain::habit::AnchorDay::MonthDay(day));
                }
                None => {
                    draft.month_valid = false;
                }
            }
        }
    });
    // ---- HF4 (W-6): the schedule picker's toggle seams -----------------
    // The picker is a draft-backed multi-select; the draft holds the
    // raw mask (a mid-edit empty selection is legal — the save gate
    // blocks it, US-5 AC-3).
    install_seam!(window: window.on_habit_schedule_day_toggled, adapter: adapter;
    bind mut (a, window), args (index, checked), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            let bit = 1_u8
                << u32::try_from(index).unwrap_or(u32::MAX).min(6);
            if checked {
                draft.schedule_bits |= bit;
            } else {
                draft.schedule_bits &= !bit;
            }
            draft.schedule_edited = true;
        }
    });
    // ---- HF4 (W-7): the reminder override's field seams ---------------
    install_seam!(window: window.on_habit_reminder_base_picked, adapter: adapter;
    bind mut (a, window), args (index), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.reminder_base = usize::try_from(index).unwrap_or(0).min(1);
            draft.reminder_edited = true;
        }
        // The live preview (#1035, W-13): the draft moved outside the
        // state Arc — the mirror pass re-derives so the preview row
        // and the validity gates read the current draft.
        a.refresh_forced(&window);
    });
    install_seam!(window: window.on_habit_reminder_offset_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.reminder_offset = t.to_string();
            draft.reminder_edited = true;
        }
        // The live preview (#1035, W-13): the draft moved outside the
        // state Arc — the mirror pass re-derives so the preview row
        // and the validity gates read the current draft.
        a.refresh_forced(&window);
    });
    install_seam!(window: window.on_habit_reminder_time_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.reminder_time = t.to_string();
            draft.reminder_edited = true;
        }
        // The live preview (#1035, W-13): the draft moved outside the
        // state Arc — the mirror pass re-derives so the preview row
        // and the validity gates read the current draft.
        a.refresh_forced(&window);
    });
    install_seam!(window: window.on_habit_reminder_snooze_edited, adapter: adapter;
    bind mut (a, window), args (t), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.reminder_snooze = t.to_string();
            draft.reminder_edited = true;
        }
        // The live preview (#1035, W-13): the draft moved outside the
        // state Arc — the mirror pass re-derives so the preview row
        // and the validity gates read the current draft.
        a.refresh_forced(&window);
    });
    // The clear leg (US-6/S3): the explicit `Some(None)` reverts the
    // habit to the plain O-1 default nudge.
    install_seam!(window: window.on_habit_reminder_clear_clicked, adapter: adapter;
    bind mut (a, window), args (), {
        let _ = &window;
        if let Some(draft) = a.habit_edit_draft.as_mut() {
            draft.reminder_clear = true;
            draft.reminder_edited = true;
        }
    });
    // The save: ONE sparse `UpdateHabit` computed against the stored
    // row, then the editor closes (the back grammar — the subject
    // dies with the root swap). An empty changeset is unreachable
    // (the gate greys); a stale draft's dead subject reads no row.
    install_seam!(window: window.on_habit_save_clicked, adapter: adapter; // slint callback: habit-save-clicked (H4 W-4)
    bind mut (a, window), args (), {
        let draft = a.habit_edit_draft.clone();
        if let Some(draft) = draft
            && let Some(stored) =
                a.state.load().habits.habits.get(&draft.habit).cloned()
        {
            // US-10 (W-14, #1048): the armed shared flow settles
            // first by the landed taxonomy (commit non-empty, discard
            // empty; the staging law) — the title/description ride
            // the flow's own sparse `UpdateHabit`, then the draft's
            // remaining fields commit sparse as landed. The save
            // gate's changeset leg reads the flow too: a text-only
            // edit keeps the save affordance armed (the D5
            // empty-dispatch law is satisfied by the flow's own
            // command — an empty draft changeset dispatches nothing).
            let text_edit = [
                (a.view_state.habit_edit_flow_armed(Some(draft.habit), false), stored.title.clone()),
                (a.view_state.habit_edit_flow_armed(Some(draft.habit), true), stored.description.clone()),
            ]
            .into_iter()
            .any(|(armed, current)| {
                armed && a.view_state.buffer().trim() != current.trim()
            });
            if draft.can_save(&stored) || text_edit {
                let state = a.state.load();
                for outcome in a.view_state.settle_flow(&state) {
                    a.apply_outcomes(&window, std::slice::from_ref(&outcome));
                }
                let changes = draft.changes(&stored);
                if changes != taskboard_domain::HabitChanges::default() {
                    a.send(taskboard_domain::StateCommand::UpdateHabit {
                        habit: draft.habit,
                        changes,
                    });
                }
                a.habit_edit_draft = None;
                // The origin return (#1303): the recorded entry
                // snapshot restores — never the `NavFocusBoard`
                // hard-route (Today stays Today, Board stays Board).
                let outcomes = a.view_state.close_habit_editor();
                a.apply_outcomes(&window, &outcomes);
                a.finish_mutation(&window);
            }
        }
    });
    // The cancel (the back button's twin): discard-and-close —
    // nothing dispatches.
    install_seam!(window: window.on_habit_editor_cancelled, adapter: adapter;
    bind mut (a, window), args (), {
        a.habit_edit_draft = None;
        // US-10: an armed shared flow stages nothing on cancel (the
        // landed staging law) — the buffer dies with the flow.
        a.view_state.discard_edit();
        // The origin return (#1303): the recorded entry snapshot
        // restores — never the `NavFocusBoard` hard-route (Today
        // stays Today, Board stays Board).
        let outcomes = a.view_state.close_habit_editor();
        a.apply_outcomes(&window, &outcomes);
        a.finish_mutation(&window);
    });
}
