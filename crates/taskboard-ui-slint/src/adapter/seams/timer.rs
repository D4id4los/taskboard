// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, SharedString};

use super::Adapter;
use crate::AppWindow;
use crate::actions::ActionId;
use crate::adapter::install_seam;

/// The timers surface's seams (DR-3/3, D-I8): the form's commit
/// parses the grammar at the seam (the rule-add absorbed-miss
/// convention) and routes; the section's cancel resolves the
/// `IdTables.timers` index; the chip click focuses the today view
/// (the existing `ViewToday` action — no new `UiEffect`).
#[allow(clippy::too_many_lines)] // the seam bundle: one block per intent, no second parser
pub(in crate::adapter) fn install_timer_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_timers_when_edited, adapter: adapter;
    bind mut (a, window), args (text), {        a.timer_when_draft = text.to_string();
    a.refresh_forced(&window);

    });

    install_seam!(window: window.on_timers_create_accepted, adapter: adapter; [else ( false)]
    bind mut (a, window), args (label, when_text, alarm_rung), {        // A whitespace-empty label dispatches nothing (D-P14): the raw
    // label never gets rewritten, an empty one never reaches the
    // engine.
    if label.trim().is_empty() {
        return false;
    }
    // A parse miss keeps the timing text (the rule-add seam's
    // return convention; the UI owns no error chrome). The tz is
    // the untested glue edge — the CLI's "only the edge reads
    // Local" shape.
    let Some(when) =
        crate::presentation::parse_timer_when(&when_text, a.day_ctx().now, &chrono::Local)
    else {
        return false;
    };
    // Both outcomes clear the field — the validity draft follows.
    a.timer_when_draft.clear();
    // The rung rides the seam's structural bool (r4 D-T7): the
    // two-rung enum's alarm bit, minted at this trust boundary.
    let insistence = if alarm_rung {
        taskboard_domain::Insistence::Alarm
    } else {
        taskboard_domain::Insistence::Normal
    };
    a.timer_draft = Some(crate::router::TimerDraft {
        label: label.into(),
        when,
        insistence,
    });
    a.route(&window, ActionId::TimerCreate);
    true

    });

    install_seam!(window: window.on_timers_cancel_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        // A stale index degrades to a no-op (the notification_row
    // guard shape).
    let Some(alarm) = a
        .tables
        .timers
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
    else {
        return;
    };
    a.timer_target = Some(alarm);
    a.route(&window, ActionId::TimerCancel);

    });

    // The chip click focuses the today view: the section sits
    // top-of-page (D-P18), so the existing FocusToday lands it
    // visibly (D-P2).
    install_seam!(window: window.on_timers_chip_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ViewToday);

    });

    // The break-rule seams (T-14, #797): the timers family's own
    // grammar. The create form's commit parses at the trust boundary
    // (the `WorkThreshold` mint — zero/negative unrepresentable) and
    // routes `BreakRuleCreate` over the one-shot draft; the remove
    // stages through the armed confirm (the chunk-2 one-armed-subject
    // rule — arming touches no engine state; only the confirm routes
    // `DeleteBreakRule`, whose cascade is NotUndoable).
    install_seam!(window: window.on_timers_break_create_toggled, adapter: adapter;
    bind mut (a, window), args (), {        a.break_rule_form_open = !a.break_rule_form_open;
    if !a.break_rule_form_open {
        a.break_rule_threshold_draft.clear();
    }
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_timers_break_threshold_edited, adapter: adapter;
    bind mut (a, window), args (text), {        a.break_rule_threshold_draft = text.to_string();
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_timers_break_create_accepted, adapter: adapter; [else ( false)]
    bind mut (a, window), args (label, threshold_text, alarm_rung), {        // A whitespace-empty label dispatches nothing (the timer
        // form's D-P14 shape).
        if label.trim().is_empty() {
            return false;
        }
        let Ok(threshold) = taskboard_domain::WorkThreshold::parse_text_form(&threshold_text)
        else {
            // A parse miss keeps the text (the rule-add seam's
            // return convention; the validity flavour renders).
            return false;
        };
        // The roaming board's target (#1335): the D-4 law's FIRST
        // live board in canonical order (the habit-create twin's
        // exact shape) — the board-free Today view has no selected
        // board, and minting from `view_state.board()` was the
        // retired board-scoping's leftover. The picker is #1314's
        // territory.
        let state = a.state.load();
        let Some(board) = taskboard_domain::order::live_boards(&state)
            .first()
            .map(|board| board.id)
        else {
            drop(state);
            return false;
        };
        drop(state);
        let insistence = if alarm_rung {
            taskboard_domain::Insistence::Alarm
        } else {
            taskboard_domain::Insistence::Normal
        };
        a.break_rule_draft = Some(crate::router::BreakRuleDraft {
            board,
            label: label.trim().to_owned(),
            threshold,
            insistence,
        });
        a.route(&window, ActionId::BreakRuleCreate);
        a.break_rule_threshold_draft.clear();
        a.break_rule_form_open = false;
        true

    });
    install_seam!(window: window.on_timers_break_remove_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(rule) = a
            .tables
            .break_rules
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            return; // a stale index against an older tables pass
        };
        // US-9 (W-10, #1066's roster half): the seam re-rooted onto
        // the registry verb — the click moves the roster cursor (the
        // K4 coherence law) and the arm toggle runs in the routed
        // arm, one semantics one home (click ≡ key by construction;
        // the repeat-is-not-the-confirm letter preserved).
        a.view_state.select_break_rule_cursor(rule);
        a.route(&window, ActionId::BreakRuleRemove);

    });
    install_seam!(window: window.on_timers_break_remove_confirmed, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(rule) = a
            .tables
            .break_rules
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .copied()
        else {
            a.break_rule_armed = None; // a stale confirm disarms
            return;
        };
        // The armed identity types the verb (the settings
        // `settings_remove_confirmed` letter): clears the flag BEFORE
        // the send; a stale index (not the armed subject) disarms
        // silently.
        let Some(armed) = a.break_rule_armed.take() else {
            return;
        };
        if armed != rule {
            return;
        }
        a.break_rule_target = Some(rule);
        a.route(&window, ActionId::BreakRuleRemoveConfirm);

    });
    install_seam!(window: window.on_timers_break_remove_cancelled, adapter: adapter;
    bind mut (a, window), args (), {        if a.break_rule_armed.take().is_some() {
            a.refresh_forced(&window);
        }

    });

    // The mouse setter's seams (r4 D33): open/dismiss flip the window
    // property (pure presentation, the palette-open precedent); every
    // intent updates the setter's draft state and re-runs the one
    // composer; the commit funnels through `timers-create-accepted` —
    // the popover adds zero parse paths (the one-parser grep
    // contract). Quick presets are quick-fill, not auto-commit (D-T6).
    install_seam!(window: window.on_timers_setter_opened, adapter: adapter;
    bind mut (a, window), args (), {        // D3 (#1306): the panel mounts conditionally (its preset
    // in-out dies with it), but the window-root two-way mirror
    // outlives the panel — the open seam reseeds it (the
    // focus-nonce seam's spelling).
    window.set_timer_setter_preset_index(0);
    window.set_timer_setter_open(true);
    a.refresh_forced(&window);
    // The focus hand-off lands after the mount pass (the
    // date-picker's changed-focus idiom; an init grab alone is
    // dropped on a fresh conditional mount).
    let nonce = window.get_timer_setter_focus_nonce();
    window.set_timer_setter_focus_nonce(nonce.wrapping_add(1));

    });
    install_seam!(window: window.on_timer_setter_dismiss_requested, adapter: adapter;
    bind mut (a, window), args (), {        a.clear_timer_setter();
    window.set_timer_setter_open(false);

    });
    install_seam!(window: window.on_timer_setter_preset_picked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(minutes) = crate::presentation::TIMER_PRESETS_MIN
        .get(usize::try_from(index).unwrap_or(usize::MAX))
    else {
        return;
    };
    let text = crate::presentation::preset_when_text(*minutes);
    a.setter_composed = Some(text.clone());
    a.setter_fire = chrono::TimeDelta::try_minutes(i64::from(*minutes))
        .and_then(|step| a.day_ctx().now.checked_add_signed(step));
    a.timer_when_draft = text;
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_timer_setter_time_edited, adapter: adapter;
    bind mut (a, window), args (text), {        // The keystroke arms the digit filter (round-5 F10, D-3.5).
    a.setter_filter_armed = true;
    a.setter_time_text = text.to_string();
    a.setter_time = crate::due_edit::parse_time_lenient(&text);
    a.recompose_timer_setter();
    a.refresh_forced(&window);

    });

    // The quick list's typed-shim seam (round-5 D6): append to the
    // mirror, run the field's own edited logic.
    install_seam!(window: window.on_timer_setter_quick_typed, adapter: adapter;
    bind mut (a, window), args (key), {        let mut text = a.setter_time_text.clone();
    text.push_str(&key);
    a.setter_filter_armed = true;
    a.setter_time = crate::due_edit::parse_time_lenient(&text);
    a.setter_time_text = text;
    a.recompose_timer_setter();
    a.refresh_forced(&window);

    });

    // The quick-times chevron's seam (round-5 F8): the filter
    // disarms for this open; no settle funnel rides the setter
    // surface (the recorded asymmetry — the latch is the due
    // editor's). The nonce re-grabs the fresh panel.
    install_seam!(window: window.on_timer_setter_quick_toggled, adapter: adapter;
    bind mut (a, window), args (), {        a.setter_filter_armed = false;
    a.refresh_forced(&window);
    let nonce = window.get_timer_setter_quick_nonce();
    window.set_timer_setter_quick_nonce(nonce.wrapping_add(1));

    });
    install_seam!(window: window.on_timer_setter_time_stepped, adapter: adapter;
    bind mut (a, window), args (n), {        // A picker-source write disarms the digit filter (round-5 F10).
    a.setter_filter_armed = false;
    // Seed-on-step (r4 D23's semantics): an unset time anchors at
    // midnight first — the first `+` yields 00:15, the first `−`
    // wraps to 23:45. The wrap stays within the picked day (the
    // day offset is an explicit choice; the step never flips it).
    let time = a.setter_time.unwrap_or(chrono::NaiveTime::MIN);
    let stepped = crate::due_edit::step_minutes(time, n);
    a.setter_time = Some(stepped);
    a.setter_time_text = crate::due_edit::format_time_hm(stepped);
    a.recompose_timer_setter();
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_timer_setter_time_picked, adapter: adapter;
    bind mut (a, window), args (minutes), {        a.setter_filter_armed = false;
    let Some(time) = crate::due_edit::time_of_minutes(minutes) else {
        return;
    };
    a.setter_time = Some(time);
    a.setter_time_text = crate::due_edit::format_time_hm(time);
    a.recompose_timer_setter();
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_timer_setter_day_picked, adapter: adapter;
    bind mut (a, window), args (index), {        a.setter_day_offset = u32::try_from(index).unwrap_or(0);
    a.recompose_timer_setter();
    a.refresh_forced(&window);

    });
    install_seam!(window: window.on_timer_setter_insist_picked, adapter: adapter;
    bind mut (a, window), args (index), {        a.setter_insist = index == 1;
    // D1 (#1306): the standard form, not the bare_mut carve-out —
    // the echo is reply-sourced, so the pick must re-push through
    // the derive pass (the day seam's shape).
    a.refresh_forced(&window);

    });
    let commit = Rc::clone(adapter);
    let commit_weak = window.as_weak();
    window.on_timer_setter_commit_requested(move |label| {
        let Some(window) = commit_weak.upgrade() else {
            return false;
        };
        // The composed grammar is the only text this seam submits —
        // and it rides `timers-create-accepted` verbatim: popover ≡
        // text form ≡ same command (the parity contract, acceptance
        // b). An unset composition never reaches the parser.
        let composed = {
            let a = commit.borrow();
            if label.trim().is_empty() {
                return false;
            }
            a.setter_composed.clone()
        };
        let Some(text) = composed else {
            return false;
        };
        let insist = commit.borrow().setter_insist;
        let ok = window.invoke_timers_create_accepted(label, SharedString::from(text), insist);
        if ok {
            commit.borrow_mut().clear_timer_setter();
            window.set_timer_setter_open(false);
        }
        ok
    });
}
