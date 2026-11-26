// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use super::Adapter;
use super::{send_card_track_toggle, send_toggled_done};
use crate::AppWindow;
use crate::actions::ActionId;
use crate::adapter::install_seam;
use crate::level_state::TrackFormState;
use slint::ComponentHandle as _;

/// The tracking seams (DR-5/4, D-TU11): the chip's stop and the detail
/// view's two verbs route the matching registry actions — the chip is
/// unconditional (`StopTracking` is total), the detail verbs ride the
/// focused task (every detail invocation has the subject focused,
/// D-TU5's fallback path). No seam for the context-menu arms: the
/// generic menu invocation already re-dispatches registry entries over
/// the focused task.
/// The steppers' and the quick-list picks' shared seed-on-step
/// semantics (r4 D23): the parse seeds midnight when unset, ±15
/// minutes wraps over midnight, a pick writes the slot verbatim.
pub(in crate::adapter) fn apply_time(
    form: &mut TrackFormState,
    which: u8,
    mutation: impl Fn(chrono::NaiveTime) -> chrono::NaiveTime,
) {
    let current = if which == 0 {
        &form.start_text
    } else {
        &form.end_text
    };
    let base = crate::due_edit::parse_time_lenient(current)
        .unwrap_or_else(|| chrono::NaiveTime::from_hms_opt(0, 0, 0).expect("midnight"));
    let text = crate::due_edit::format_time_hm(mutation(base));
    if which == 0 {
        form.start_text = text;
    } else {
        form.end_text = text;
    }
}

#[allow(clippy::too_many_lines)] // one seam per callback, kept flat
pub(in crate::adapter) fn install_tracking_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_tracking_chip_stop_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::TaskTrackStop);

    });

    install_seam!(window: window.on_task_track_start_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::TaskTrackStart);

    });

    install_seam!(window: window.on_task_track_stop_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::TaskTrackStop);

    });

    // The entries overlay's open affordance (r4 chunk 10, D-TS8):
    // routes `task.track-entries` over the VIEW'S SUBJECT — the
    // one-shot target seam (the notification row's track-start
    // precedent; the subject is not guaranteed focused). The open
    // arm's kiosk/subject refusals are the interpret arms'
    // dispatch-time gating.
    install_seam!(window: window.on_task_track_entries_clicked, adapter: adapter; [try] // slint callback: task-track-entries-clicked (the entries overlay's opener, r4 D-TS6)
    bind mut (a, window), args (), {        a.track_target = a.view_state.subject_task();
    a.route(&window, ActionId::TaskTrackEntries);

    });

    // The dismissal layer's seam (D-TS1): an outside click is a
    // ladder step (form → list; list → close) — the view-state
    // machine resolves it; the armed delete clears with the overlay
    // (the state-truth rule — nothing ticks).
    install_seam!(window: window.on_track_entries_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        a.track_entry_target = None;
    // The outside click is a ladder step (D-TS1): the form child ends
    // alone (N1's generic rung); the list closes the overlay.
    let state = a.state.load();
    let outcomes = match a.view_state.cancel_top_child(&state) {
        Some(outcomes) => outcomes,
        None => a.view_state.close_track_entries(),
    };
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });

    // The armed delete (D-TS5): the first activate arms (the adapter
    // holds the armed id; the push mirrors the index), the second
    // dispatches `task.track-entry-delete` — the router resolves the
    // one-shot target into the existing `DeleteTimeEntry`. Disarm
    // rides the push (a dropped row or a closed overlay renders
    // unarmed) and the dismissal seam (state-truth, nothing ticks).
    install_seam!(window: window.on_track_entry_delete_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(entry) = a
        .tables
        .track_entries
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return; // a stale index
    };
    if a.track_entry_target == Some(entry) {
        // The confirm: the target fills the one-shot ctx and the
        // router dispatches; the overlay stays open (the
        // multi-delete posture — the row vanishes on refresh).
        a.route(&window, ActionId::TaskTrackEntryDelete);
    } else {
        a.track_entry_target = Some(entry);
        a.refresh_forced(&window);
    }

    });

    // The add seam (D-TS4): opens the unseeded form over the subject;
    // the seeding lands in the same window as row-activation's.
    install_seam!(window: window.on_track_form_add_requested, adapter: adapter;
    bind mut (a, window), args (), {        let Some(subject) = a.view_state.subject_task() else {
        return;
    };
    let outcomes = a.view_state.open_track_entry_form(subject, None);
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);
    a.seed_track_form(None);
    a.refresh_forced(&window);

    });

    // The form's seams (r4 D-TS4): every intent mutates the
    // adapter-owned state and refreshes — the widget renders what it
    // is pushed and reports what the user did (§5.1). The commit
    // rides the one funnel (`commit_track_form`).
    install_seam!(window: window.on_track_form_date_edited, adapter: adapter;
    bind mut (a, window), args (text), {        if let Some(form) = a.view_state.track_form_mut() {
        form.date_text = text.to_string();
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_track_form_grid_toggled, adapter: adapter;
    bind mut (a, window), args (), {        if a.view_state.picker().is_some() {
        // The close half: the child ends `Cancelled` — the exit rides
        // the one seam and the form ignores it; the refresh is what
        // unmounts the grid.
        let state = a.state.load();
        let outcomes = a.view_state.close_entry_date_picker(&state);
        a.apply_outcomes(&window, &outcomes);
        a.finish_mutation(&window);
        return;
    }
    let outcomes = a.view_state.open_entry_date_picker();
    a.apply_outcomes(&window, &outcomes);
    // Seed the fresh child's picker state (the level mints clean):
    // `date_text`'s anchor — the flattened toggle's noon arithmetic
    // verbatim.
    let date_text = a
        .view_state
        .track_form()
        .map_or(String::new(), |form| form.date_text.clone());
    let anchor = chrono::NaiveDate::parse_from_str(&date_text, "%Y-%m-%d").ok();
    let anchor = anchor.and_then(|d| {
        use chrono::TimeZone as _;
        chrono::Local
            .from_local_datetime(&d.and_hms_opt(12, 0, 0).expect("noon"))
            .single()
            .map(|local| local.with_timezone(&chrono::Utc))
    });
    if let Some(child) = a.view_state.picker_mut() {
        child.picker = crate::due_edit::DuePickerState::open(
            chrono::Local::now().date_naive(),
            anchor,
            &chrono::Local,
        );
    }
    a.refresh_forced(&window);

    });
    // The date-grid overlay's light-dismiss (the overlay migration,
    // M4): the same close half the toggle's close arm rides — the
    // child ends `Cancelled`, the form and its mirrors stay, the
    // refresh unmounts the grid.
    // slint: track-form-grid-dismiss-requested
    install_seam!(window: window.on_track_form_grid_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        if a.view_state.picker().is_some() {
        let state = a.state.load();
        let outcomes = a.view_state.close_entry_date_picker(&state);
        a.apply_outcomes(&window, &outcomes);
        a.finish_mutation(&window);
        // The drop is pure presentation (no command): the flag's
        // unmount is the forced pass's derive — the date-picker
        // dismissal's shape.
        a.refresh_forced(&window);
    }

    });
    install_seam!(window: window.on_track_form_grid_key, adapter: adapter;
    bind mut (a, window), args (code), {        use crate::due_edit::DatePickerKey as K;
        let Some(key) = crate::due_edit::DatePickerKey::from_code(code) else {
        return;
    };
    // The grid's Enter ends the child `Cancelled` (D4): a grid that
    // stayed open under a field-focused Enter would be a half-mounted
    // level — the exact shape the protocol forbids. Nothing rendered
    // changes but the grid closing: its mirror follows the child.
    // The commit is the click's or the typed field's.
    if key == K::Commit {
        let state = a.state.load();
        let outcomes = a.view_state.close_entry_date_picker(&state);
        a.apply_outcomes(&window, &outcomes);
        a.finish_mutation(&window);
        return;
    }
    let Some(child) = a.view_state.picker_mut() else {
        return; // a stale stroke against a closed grid
    };
    let picker = &mut child.picker;
    match key {
        K::Left => picker.move_days(-1),
        K::Right => picker.move_days(1),
        K::Up => picker.move_days(-7),
        K::Down => picker.move_days(7),
        K::PrevMonth => picker.move_months(-1),
        K::NextMonth => picker.move_months(1),
        K::MonthStart => picker.month_edge(true),
        K::MonthEnd => picker.month_edge(false),
        // The grid's Enter was the Cancelled rung above; the time
        // half is unused in the form's date-only picker.
        K::Commit | K::TimeBack | K::TimeForward => {}
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_track_form_day_clicked, adapter: adapter;
    bind mut (a, window), args (cell), {        use chrono::TimeZone as _;
        let Some(date) = a
        .view_state
        .picker()
        .and_then(|child| {
            child
                .picker
                .grid()
                .get(usize::try_from(cell).unwrap_or(usize::MAX))
                .and_then(|c| c.date)
        })
    else {
        return; // a blank or a stale index
    };
    // The click completes the child with the picked day (N2): the
    // noon-anchored instant — the parent's consumption formats its
    // local date back (the `LevelResult::Date` round trip), so the
    // cursor poke and the direct write retire with the flattened
    // model.
    let noon = date.and_hms_opt(12, 0, 0).expect("noon");
    let resolved = chrono::Local
        .from_local_datetime(&noon)
        .single()
        .or_else(|| chrono::Local.from_local_datetime(&noon).latest());
    let instant = resolved.map_or_else(
        || chrono::Utc.from_utc_datetime(&noon),
        |local| local.with_timezone(&chrono::Utc),
    );
    let state = a.state.load();
    let outcomes = a.view_state.end_top_flow_with(
        &state,
        crate::surfaces::GuestExit::Completed(Some(
            crate::surfaces::LevelResult::Date(instant),
        )),
    );
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);

    });
    install_seam!(window: window.on_track_form_start_edited, adapter: adapter;
    bind mut (a, window), args (text), {        // The keystroke arms the digit filter (round-5 F10, D-3.5).
    if let Some(form) = a.view_state.track_form_mut() {
        form.start_filter_armed = true;
        form.start_text = text.to_string();
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_track_form_end_edited, adapter: adapter;
    bind mut (a, window), args (text), {        // The keystroke arms the digit filter (round-5 F10, D-3.5).
    if let Some(form) = a.view_state.track_form_mut() {
        form.end_filter_armed = true;
        form.end_text = text.to_string();
    }
    a.refresh_forced(&window);

    });

    // The quick lists' typed-shim seams (round-5 D6): append to the
    // field's mirror, run the field's own edited logic.
    install_seam!(window: window.on_track_form_start_quick_typed, adapter: adapter;
    bind mut (a, window), args (key), {        if let Some(form) = a.view_state.track_form_mut() {
        form.start_filter_armed = true;
        let mut text = form.start_text.clone();
        text.push_str(&key);
        form.start_text = text;
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_track_form_end_quick_typed, adapter: adapter;
    bind mut (a, window), args (key), {        if let Some(form) = a.view_state.track_form_mut() {
        form.end_filter_armed = true;
        let mut text = form.end_text.clone();
        text.push_str(&key);
        form.end_text = text;
    }
    a.refresh_forced(&window);

    });

    // The quick-times chevrons' seams (round-5 F8): the filter
    // disarms for this open (the track surface has no settle funnel —
    // the recorded asymmetry). The shared nonce re-grabs the fresh
    // panel (one panel mounts per group at a time — the site
    // discriminator picks which).
    install_seam!(window: window.on_track_form_start_quick_toggled, adapter: adapter;
    bind mut (a, window), args (), {        if let Some(form) = a.view_state.track_form_mut() {
        form.start_filter_armed = false;
    }
    a.refresh_forced(&window);
    let nonce = window.get_track_form_quick_nonce();
    window.set_track_form_quick_nonce(nonce.wrapping_add(1));

    });
    install_seam!(window: window.on_track_form_end_quick_toggled, adapter: adapter;
    bind mut (a, window), args (), {        if let Some(form) = a.view_state.track_form_mut() {
        form.end_filter_armed = false;
    }
    a.refresh_forced(&window);
    let nonce = window.get_track_form_quick_nonce();
    window.set_track_form_quick_nonce(nonce.wrapping_add(1));

    });
    for (name, which, pick) in [
        ("start_stepped", 0u8, false),
        ("start_picked", 0u8, true),
        ("end_stepped", 1u8, false),
        ("end_picked", 1u8, true),
    ] {
        let seam = Rc::clone(adapter);
        let seam_weak = window.as_weak();
        let handler = move |window: &AppWindow, arg: i32| {
            let mut a = seam.borrow_mut();
            // A picker-source write disarms the field's digit filter
            // (round-5 F10).
            if let Some(form) = a.view_state.track_form_mut() {
                if which == 0 {
                    form.start_filter_armed = false;
                } else {
                    form.end_filter_armed = false;
                }
                if pick {
                    let minutes = arg;
                    apply_time(form, which, |_| {
                        crate::due_edit::time_of_minutes(minutes).unwrap_or_else(|| {
                            chrono::NaiveTime::from_hms_opt(0, 0, 0).expect("midnight")
                        })
                    });
                } else {
                    let delta = arg;
                    apply_time(form, which, |time| {
                        crate::due_edit::step_minutes(time, delta)
                    });
                }
            }
            a.refresh_forced(window);
        };
        match name {
            "start_stepped" => {
                let w = seam_weak.clone();
                window.on_track_form_start_stepped(move |arg| {
                    if let Some(win) = w.upgrade() {
                        handler(&win, arg);
                    }
                });
            }
            "start_picked" => {
                let w = seam_weak.clone();
                window.on_track_form_start_picked(move |arg| {
                    if let Some(win) = w.upgrade() {
                        handler(&win, arg);
                    }
                });
            }
            "end_stepped" => {
                let w = seam_weak.clone();
                window.on_track_form_end_stepped(move |arg| {
                    if let Some(win) = w.upgrade() {
                        handler(&win, arg);
                    }
                });
            }
            _ => {
                let w = seam_weak;
                window.on_track_form_end_picked(move |arg| {
                    if let Some(win) = w.upgrade() {
                        handler(&win, arg);
                    }
                });
            }
        }
    }

    // The commit funnel (F24): the end field's Return. `true` steps
    // the surface back to the list; `false` keeps the form open with
    // the invalid flavour (the absorbed-miss convention keeps the
    // typed text).
    install_seam!(window: window.on_track_form_commit_requested, adapter: adapter; [else ( false)]
    bind mut (a, window), args (), {        a.commit_track_form(&window)

    });

    // The row-activate seam (D-TS3): opens the seeded edit form
    // (the commit funnel owns its Return); the seed resolves
    // through the id table (never a viewport offset).
    install_seam!(window: window.on_track_entry_row_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(subject) = a.view_state.subject_task() else {
        return;
    };
    let Some(entry) = a
        .tables
        .track_entries
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    let outcomes = a.view_state.open_track_entry_form(subject, Some(entry));
    a.apply_outcomes(&window, &outcomes);
    a.finish_mutation(&window);
    a.seed_track_form(Some(entry));
    a.refresh_forced(&window);

    });

    // The task view's done checkbox (r4 D16b/F15): raw-send the
    // toggled SetTaskDone over the VIEW'S SUBJECT — deliberately not
    // the registry route (`TaskComplete`/`TaskReopen` act on the
    // focused task; the cycle's fallback subjects
    // `first_task_of(subject_stack)` when focus is empty, so the
    // subject is not guaranteed focused — the card checkbox's recorded
    // reason, adapted to the view). A stale/dead subject degrades to a
    // no-op (the index-seam precedent). No
    // `settle_pointer_conflict`: the board checkbox settles because
    // the board owns the capture-settle lane; the view's other clicks
    // don't settle and neither does this one.
    // The card play/pause control (UX-R8 chunk-5 D-6): the detail
    // card's seam — the subject task.
    install_seam!(window: window.on_task_track_toggled, adapter: adapter;
    bind mut (a, window), args (), {        let Some(task) = a.view_state.subject_task() else {
        return;
    };
    send_card_track_toggle(&mut a, &window, task);

    });
    install_seam!(window: window.on_task_done_toggled, adapter: adapter;
    bind (a, _window), args (), {            let Some(task) = a.view_state.subject_task() else {
        return;
    };
    if !a.state.load().tasks.contains_key(&task) {
        return;
    }
    send_toggled_done(&a, task);

    });
}
