// SPDX-License-Identifier: AGPL-3.0-only

//! The adapter seam inventory (chunk 4.2, plan §7 — the dropped-install
//! net, D6): a two-sided structural manifest assertion over the
//! `adapter.rs` source (the `icon_mapping`/`pin_guard` precedent — a
//! structural manifest assertion over sources, not a text assertion on
//! program output). Side 1 (totality): every name in `SEAM_INVENTORY`
//! is installed via `window.on_<name>(` exactly once — a dropped
//! install fails here. Side 2 (no unlisted installs): the total count
//! of `window.on_` occurrences equals `SEAM_INVENTORY.len()` — an
//! added-but-unlisted install fails here. The census is
//! conversion-agnostic: it counts installs, not the syntax that
//! constructs them, so it keeps guarding every future chunk
//! regardless of macro vs explicit wiring.

const ADAPTER_SOURCE: &str = include_str!("../../src/adapter.rs");

/// The 175 installed window callbacks in `adapter.rs` as of the
/// focus-settings-editor chunk (plan §2.1 census; the #275 close
/// comment counted 110 at `5d8c0751` — the seam surface grows ~30
/// installs per GUI
/// round, so this list changes on purpose when a new seam lands, and
/// both test sides force the list to stay honest).
const SEAM_INVENTORY: &[&str] = &[
    // The priority pickers' seams (UX R-3 D-1, #788): the row
    // openers route the landed actions; the slot commit and the
    // dismissal are the picker's own seams.
    // The status panel's seams (OP3 W-1): the mount's snapshot-on-open
    // dispatch and the Refresh control's re-query — the fold/failure
    // directions are invokes (Rust → slint), not installs.
    "status_panel_open",
    "status_panel_refresh",
    // The status panel's remediation lane (the D2 command plan
    // W-1..W-4/W-6): the rows' arm moments, the armed strip's
    // decision pair, and the outcome fold (the W-6 fold installs
    // like the export twin — the drain invokes it outside the
    // borrow).
    "status_holder_kill",
    "status_lock_force_unlock",
    "status_daemon_restart",
    "status_remediation_armed",
    "status_remediation_outcome_shown",
    "task_importance_edit_asked",
    "task_urgency_edit_asked",
    "priority_slot_clicked",
    "priority_dismiss_requested",
    // The labels section/overlay seams (UX R-3 D-2/D-5, #789): the
    // chip x, the roster toggle, the form funnel, and the armed
    // delete's pair (UX R-3 D-2's r2 grammar).
    "chip_unassign_clicked",
    "labels_manage_asked",
    "labels_overlay_dismiss_requested",
    "labels_overlay_row_toggled",
    "labels_overlay_add_requested",
    "labels_overlay_edit_requested",
    "labels_overlay_title_edited",
    "labels_overlay_colour_edited",
    "labels_overlay_swatch_clicked",
    "labels_overlay_form_commit_requested",
    "labels_overlay_form_cancelled",
    "labels_overlay_delete_clicked",
    "labels_overlay_delete_cancelled",
    // The restore sections' seams (the undo-family journey plan
    // W-2/W-4/W-9): the armed-confirm pairs over the published
    // rosters and the rule list's clear-all.
    "label_restore_requested",
    "label_restore_cancelled",
    // The bulk restore pair (#1245, the labels overlay's restore-all
    // row) and the deleted-habits section's quartet (#1290/#1245 —
    // the menu trio retired with the enumeration).
    "label_restore_all_requested",
    "label_restore_all_cancelled",
    "habits_habit_restore_requested",
    "habits_habit_restore_cancelled",
    "habits_habit_restore_all_requested",
    "habits_habit_restore_all_cancelled",
    "entry_restore_requested",
    "entry_restore_cancelled",
    "rules_clear_asked",
    "rules_clear_cancelled",
    // The date-grid overlay's light-dismiss (the overlay migration,
    // M4).
    "track_form_grid_dismiss_requested",
    "banner_mute_clicked",
    "banner_row_activated",
    "banner_row_open_requested",
    "banner_snooze_clicked",
    "board_add_task_activated",
    "board_bar_archived_activated",
    // The armed strips' confirm/cancel seams (UX R-2 D-4, #787).
    "settings_remove_confirmed",
    "settings_remove_cancelled",
    "settings_files_action_confirmed",
    "settings_files_action_cancelled",
    // The habit row's −1 recovery seam (UX R-2 D-3, #786): the
    // audited AdjustHabitLog over the row's current window.
    "habits_undo_log_clicked",
    // The armed-delete strips' pair (UX R-2 D-2, #784): Confirm
    // routes the armed verb, Cancel disarms — one install each.
    // The armed multi-delete strip's pair (UX-R8 chunk-5 D-9, #882):
    // the #784 grammar's third instance.
    "task_multi_delete_strip_confirmed",
    "task_multi_delete_strip_cancelled",
    "board_delete_strip_confirmed",
    "board_delete_strip_cancelled",
    // The board ceremony strips' confirm/cancel pair (OP1, W-1/W-3/
    // W-6): one pair, the armed subject surface-wide decides the verb.
    "board_ceremony_confirmed",
    "board_ceremony_cancelled",
    "stack_delete_strip_confirmed",
    "stack_delete_strip_cancelled",
    "board_bar_segment_activated",
    // The board color strip's pair (UX R-4 D-1, #791): a swatch
    // click commits the exact SetBoardColor, the dismissal disarms.
    "board_color_picked",
    "board_color_strip_dismissed",
    "board_body_activated",
    "board_body_context_requested",
    "board_card_drag_armed",
    "board_card_drag_finished",
    "board_card_dropped",
    // The card play/pause control's seams (UX-R8 chunk-5 D-6, US-4):
    // the four addressing chains (board/compact, stack, today,
    // detail) route the existing tracking ids (#782's fold).
    "board_task_track_toggled",
    "stack_task_track_toggled",
    "today_row_track_toggled",
    "task_track_toggled",
    // The gap drop grammar (UX-R8 D-2, US-2): the gap-zone hover and
    // drop reports (the #760 fold — this inventory is the stage-3
    // cross-check root).
    "board_gap_dropped",
    "board_gap_hovered",
    "board_create_committed",
    "board_create_edited",
    "board_create_field_committed",
    "board_create_field_edited",
    "board_default_preset_clicked",
    "board_drag_hovered",
    "board_header_activated",
    "board_header_context_requested",
    "board_new_stack_clicked",
    "board_rename_field_committed",
    "board_rename_field_edited",
    "board_segment_context_requested",
    "board_stack_create_committed",
    "board_stack_create_edited",
    "board_stack_rename_committed",
    "board_stack_rename_edited",
    "board_stack_width_drag",
    "board_stack_width_press",
    "board_stack_width_release",
    "board_switcher_activated",
    "board_task_activated",
    "board_switcher_card_dropped",
    "board_switcher_drop_hovered",
    "board_task_clicked",
    // The modifier-click grammar (UX-R8 D-5, US-4/S1): the card's
    // modifier release (the #760 fold).
    "board_task_clicked_modifiers",
    "board_task_context_requested",
    "board_task_done_toggled",
    "capture_accepted",
    // The slint twin is the ModalDialogHost mount's scrim dismissal
    // seam (round-9 W1): callback "capture-dismiss-requested".
    "capture_dismiss_requested",
    "capture_text_edited",
    "date_editor_focus_lost",
    "date_picker_dismiss_requested",
    "date_picker_grid_focus_changed",
    "focus_note_accepted",
    "focus_note_edited",
    "focus_overlay_close_requested",
    "focus_stop_clicked",
    "review_card_clicked",
    "walk_answer_edited",
    "walk_candidate_activated",
    "walk_date_commit_clicked",
    "walk_date_edited",
    "walk_file_clicked",
    "walk_file_commit_clicked",
    "walk_file_next_clicked",
    "walk_file_prev_clicked",
    "walk_finish_clicked",
    "walk_keep_clicked",
    "walk_leave_clicked",
    "walk_pick_clicked",
    "walk_record_clicked",
    "walk_abort_accept_clicked",
    "walk_abort_arm_requested",
    "walk_abort_dismiss_clicked",
    "walk_button_invoked", // the slint spelling: "walk-button-invoked" (J4 W-1)
    "walk_plan_clicked",
    "walk_tomorrow_clicked",
    "focus_switcher_invoked",
    // The habit row's focus fact (cross-surface friction plan D-2,
    // #832): the activation mints the menu verbs' subject.
    // The double-click editor opener (D3a W-4, #1201); the plan cell
    // spells it kebab: "habits-edit-asked" (the slint callback).
    "habits_edit_asked",
    "habits_row_activated",
    // H2 (round 9 chunk 6): the row's right-click context seam joins;
    // the convert trio and the reorder pair retire (the menu owns
    // them), the +1 renames to the done-slot-shared log seam.
    // seam (H2 W-2): callback "habits-context-requested".
    "habits_context_requested",
    // The habit list's gap drop (chunk 8 D5, US-4/AC-1): callback
    // "habits-drop-asked" — the #918 board pattern over the flat list.
    "habits_drop_asked",
    // The HF1 skip/pause/resume seams (W-1/W-3): the row affordances
    // resolve the IdTables.habits index; the pause editor's commit is
    // the trust boundary (strict %Y-%m-%d); cancel writes nothing.
    // callbacks "habits-skip-asked", "habits-pause-asked",
    // "habits-resume-asked", "habits-pause-until-picked",
    // "habits-pause-cancelled".
    "habits_skip_asked",
    "habits_pause_asked",
    "habits_resume_asked",
    "habits_pause_until_picked",
    "habits_pause_cancelled",
    "habits_create_asked",
    "habits_create_cancelled",
    "habits_create_confirmed",
    "habits_create_quota_edited",
    // seam (H2 W-1): callback "habits-log-clicked".
    "habits_log_clicked",
    // The inline quota edit's seams (HF2, W-5 #961): the phrase
    // click, the stepper rungs, the commit, and the dismissal funnel
    // (callbacks "habits-quota-edit-asked" / "-stepped" /
    // "-committed" / "-dismissed").
    "habits_quota_edit_asked",
    "habits_quota_stepped",
    "habits_quota_committed",
    "habits_quota_dismissed",
    "habits_repair_clicked",
    "intercept_key",
    // The undo branch picker's seams (UF2, #353): the mount ack, the
    // plain reflex rows, the two row commits, the dismissal (callbacks
    // "undo-branches-open" / "-line-undo" / "-line-redo" /
    // "-target-picked" / "-branch-picked" / "-picker-dismissed").
    "undo_branches_open",
    "undo_line_undo",
    "undo_line_redo",
    "undo_target_picked",
    "undo_branch_picked",
    "undo_picker_dismissed",
    "menu_dismiss_requested",
    "menu_entry_invoked",
    "menu_level_back",
    // The quick actions bar's invocation (UX-R8 chunk-5 D-2, US-1):
    // the bar-button seam resolves against the open-time vec (#782's
    // fold — the inventory is the cross-check root).
    "menu_quick_bar_invoked",
    "menu_submenu_entry_invoked",
    "notification_cancel_clicked",
    "notification_dismiss_clicked",
    "notification_done_clicked",
    "notification_mute_clicked",
    "notification_row_open_requested",
    "notification_snooze_span_picked",
    // The habit editor's seams (H4 D5): the draft mirrors, the
    // per-kind day row, and the save/cancel pair.
    "habit_title_edited",
    "habit_description_edited",
    "habit_quota_kind_picked",
    "habit_quota_count_edited",
    "habit_time_edited",
    "habit_time_cleared",
    "habit_weekday_picked",
    "habit_month_day_edited",
    // HF4 (W-6/W-7): the schedule picker's toggle seam ("habit-schedule-day-toggled")
    // and the override section's field seams ("habit-reminder-base-picked",
    // "habit-reminder-offset-edited", "habit-reminder-time-edited",
    // "habit-reminder-snooze-edited", "habit-reminder-clear-clicked").
    "habit_schedule_day_toggled",
    "habit_reminder_base_picked",
    "habit_reminder_offset_edited",
    "habit_reminder_time_edited",
    "habit_reminder_snooze_edited",
    "habit_reminder_clear_clicked",
    "habit_save_clicked",
    // The J1 W-4 cross-check's anchor: the slint-side spelling of the
    // save seam is the kebab "habit-save-clicked" (the #760 grammar —
    // ux-coverage greps this inventory for the plan's callback cell).
    // "habit-save-clicked"
    "habit_editor_cancelled",
    // The habit editor's History row (HF3 W-8): callback
    // "habit-history-clicked".
    "habit_editor_history_clicked",
    // The history dialog's pager flip (HF3 W-8): callback
    // "habit_history_month_flipped".
    "habit_history_month_flipped",
    // The history dialog's read-role dismissal (HF3 W-8): callback
    // "habit_history_close_requested".
    "habit_history_close_requested",
    "notification_track_start_clicked",
    "plan_box_activated",
    "plan_box_earlier",
    "plan_box_later",
    "plan_box_remove",
    "plan_place_cancel",
    "plan_place_commit",
    "plan_effort_edited",
    "plan_time_edited",
    "plan_pool_activated",
    "palette_dismiss_requested",
    "palette_query_edited",
    // The search row's History affordance (D3b W-10, #1218); the plan
    // cell spells it kebab: "palette-row-activated".
    "palette_row_activated",
    "reflection_editor_cancelled",
    "reflection_editor_committed",
    "reflection_editor_energy_edited",
    "reflection_editor_flag_toggled",
    "reflection_editor_mood_edited",
    "reflection_editor_text_edited",
    "reflection_remove_cancelled",
    "reflection_remove_clicked",
    "reflection_remove_confirmed",
    "reflection_row_activated",
    "rule_add_custom_accepted",
    "rule_add_preset_clicked",
    "rule_offset_edited",
    "settings_back_clicked",
    "settings_field_edited",
    "settings_focus_chain_picked",
    "settings_focus_enabled_picked",
    "settings_focus_notify_picked",
    "settings_general_committed",
    "settings_general_focus_lost",
    "settings_preset_picked",
    "settings_reminders_picked",
    "settings_create_clicked",
    "settings_disconnect_clicked",
    "settings_files_picked",
    "settings_refresh_clicked",
    "settings_files_board_action_clicked",
    "files_poll_secs_committed", // the slint spelling: "files-poll-secs-committed" (J4 W-6)
    "settings_files_board_restore_clicked",
    "settings_publish_picked",
    "settings_remote_bind_picked",
    "settings_remove_clicked",
    "settings_save_clicked",
    "settings_scale_committed",
    "settings_scale_focus_lost",
    "settings_scale_pressed",
    "settings_scale_released",
    "settings_sink_picked",
    "settings_view_mounted",
    "stack_create_committed",
    "stack_create_edited",
    "stack_task_activated",
    "stack_task_clicked",
    "stack_task_context_requested",
    "stack_task_done_toggled",
    "task_back_clicked",
    "task_date_accepted",
    "task_date_day_clicked",
    "task_date_edited",
    "task_date_picker_key",
    "task_date_picker_toggled",
    "task_date_quick_toggled",
    // The intra-picker click arm (task-detail-editor UX plan US-2/D-6,
    // 2026-11-17): the TimeField's field click arms the one-shot
    // focus-steal latch before the focus move.
    "task_date_time_field_armed",
    // The effort row's seams (the overlay migration, M9): callback
    // "task-effort-focus-lost" (the always-on row's commit funnel;
    // the D-5 guard).
    // #838, 2026-11-17): the inline editor's open/mirror/commit/
    // clear/cancel and the focus-loss settle (the date-editor buffer
    // pattern over adapter state).
    "task_effort_edited",
    "task_effort_committed",
    "task_effort_focus_lost",
    "task_date_quick_typed",
    "task_date_time_accepted",
    "task_date_time_edited",
    "task_date_time_picked",
    "task_description_committed",
    "task_description_edited",
    "task_description_edit_opened",
    "task_done_toggled",
    "task_due_edit_opened",
    // The export dialog's seams (the J2 export plan W-1..W-5): the
    // Settings row's opener, the close, the pickers'/fields' mirrors,
    // the open-after toggle, and the Run dispatch. The plan's §6
    // tokens (the ux-coverage cross-check; slint's hyphens are Rust's
    // underscores here): the W-1 pair "export-dialog-open / export-dialog-close"
    // (one seam each),
    // "export-settings-open", "export-spec-picked", "export-field-edited",
    // "export-open-after-toggled", "export-run", "export-outcome-shown".
    "export_dialog_open",
    "export_settings_open",
    "export_dialog_close",
    "export_spec_picked",
    "export_field_edited",
    "export_open_after_toggled",
    "export_run",
    "export_outcome_shown",
    // The export dialog's preview/clipboard lane (the D2 preview
    // plan W-1..W-4): the dispatch seams, the settled-reply display
    // fold, and the D-5 family-gating fold.
    "export_preview",
    "export_clipboard_copy",
    "export_preview_shown",
    "export_preview_gated",
    "recur_bound_picked",
    "recur_commit_clicked",
    "recur_count_edited",
    "recur_detach_arm_clicked",
    "recur_detach_cancel_clicked",
    "recur_detach_confirm_clicked",
    "recur_editor_cancel_clicked",
    "recur_every_days_edited",
    "recur_monthly_day_edited",
    "recur_monthly_mode_picked",
    "recur_monthly_nth_edited",
    "recur_monthly_weekday_picked",
    "recur_pattern_picked",
    "recur_until_edited",
    "recur_weekday_toggled",
    "recur_weekly_interval_edited",
    "series_edit_asked",
    "task_recur_convert_asked",
    "task_recur_convert_cancelled",
    "task_recur_convert_confirmed",
    "task_recur_pause_accepted",
    "task_recur_pause_asked",
    "task_recur_pause_cancelled",
    "task_recur_resume_clicked",
    "task_rule_jump_to_due_clicked",
    "task_rule_remove_clicked",
    "task_rule_row_clicked",
    "task_rule_unmute_clicked",
    "task_scheduled_edit_opened",
    "task_title_committed",
    "task_title_edit_activated",
    "task_title_edited",
    "task_track_entries_clicked", // the slint spelling: "task-track-entries-clicked" (r4 D-TS6; the #760 grammar)
    "task_track_start_clicked",
    "task_track_stop_clicked",
    "text_flow_focus_lost",
    "timers_break_create_accepted",
    "timers_break_create_toggled",
    "timers_break_remove_cancelled",
    "timers_break_remove_clicked",
    "timers_break_remove_confirmed",
    "timers_break_threshold_edited",
    "timers_cancel_clicked",
    "timers_chip_clicked",
    "timers_create_accepted",
    "timer_setter_commit_requested",
    "timer_setter_day_picked",
    "timer_setter_dismiss_requested",
    "timer_setter_insist_picked",
    "timer_setter_preset_picked",
    "timer_setter_quick_toggled",
    "timer_setter_quick_typed",
    "timer_setter_time_edited",
    "timer_setter_time_picked",
    "timer_setter_time_stepped",
    "timers_setter_opened",
    "timers_when_edited",
    "today_row_activated",
    "today_row_clicked",
    "today_row_context_requested",
    "today_row_done_toggled",
    "today_row_extent",
    "today_tracked_activated",
    "toolbar_inbox_clicked",
    "toolbar_menu_clicked",
    "toolbar_switcher_board_clicked",
    "toolbar_switcher_today_clicked",
    "track_entries_dismiss_requested",
    "track_entry_delete_clicked",
    "track_entry_row_activated",
    "track_form_add_requested",
    "track_form_commit_requested",
    "track_form_date_edited",
    "track_form_day_clicked",
    "track_form_end_edited",
    "track_form_end_picked",
    "track_form_end_stepped",
    "track_form_grid_key",
    "track_form_grid_toggled",
    "track_form_end_quick_toggled",
    "track_form_end_quick_typed",
    "track_form_start_edited",
    "track_form_start_quick_toggled",
    "track_form_start_quick_typed",
    "track_form_start_picked",
    "track_form_start_stepped",
    "tracking_chip_stop_clicked",
    "window_width_changed",
];

/// Word-bounded occurrence count of `needle` in `source`: a longer
/// identifier extending the needle must not count.
fn word_bounded_matches(source: &str, needle: &str) -> usize {
    source
        .match_indices(needle)
        .filter(|(at, _)| {
            source[at + needle.len()..]
                .chars()
                .next()
                .is_none_or(|next| !matches!(next, 'a'..='z' | '0'..='9' | '_'))
        })
        .count()
}

/// The manifest's exactly-once side: every listed name must appear
/// exactly once, else the deviations name the drift.
fn assert_exactly_once(counts: Vec<(String, usize)>, what: &str) {
    let deviations: Vec<String> = counts
        .into_iter()
        .filter_map(|(name, count)| (count != 1).then(|| format!("{name}: {count} {what}")))
        .collect();
    assert!(
        deviations.is_empty(),
        "{what} deviate from exactly-once:\n{}",
        deviations.join("\n")
    );
}

fn install_counts() -> Vec<(String, usize)> {
    SEAM_INVENTORY
        .iter()
        .map(|name| {
            // A seam is spelled `window.on_<name>(` when wired
            // explicitly and `window.on_<name>,` at an
            // `install_seam!` invocation.
            let needle = format!("window.on_{name}");
            (
                (*name).to_owned(),
                word_bounded_matches(ADAPTER_SOURCE, &needle),
            )
        })
        .collect()
}

#[test]
fn every_listed_seam_is_installed_exactly_once() {
    assert_exactly_once(install_counts(), "installs");
}

#[test]
fn no_unlisted_installs_exist() {
    let total = ADAPTER_SOURCE.matches("window.on_").count();
    assert_eq!(
        total,
        SEAM_INVENTORY.len(),
        "adapter.rs installs {total} seams but the inventory lists {} — an added install must be listed; a dropped one already failed the totality side",
        SEAM_INVENTORY.len()
    );
}

/// The adapter→UI push functions (the focus-restoration chunk's
/// restore/mask seams): the `invoke_` twins of the install manifest.
/// Same two-sided law — exactly-once, and no unlisted push function.
const INVOKE_INVENTORY: &[&str] = &[
    // The focus-origin restore's anchor-family pushes (US-1/W-1,
    // D-F1; the `// slint:` comments in app-window.slint name them).
    "focus_task_row",
    "focus_stack_header",
    "focus_board_cell",
    // The control walk's clamped stop (US-2/W-2, D-W2).
    "mask_focus",
    // The status panel's settled-reply fold (OP3 W-2): one invoke
    // (the drain's settled arm).
    "status_fields_folded",
    // The status panel's remediation outcome fold (the D2 command
    // plan W-6): one invoke in this file — the lane-full/closed arm
    // of the confirm dispatch (the normal settle path rides the
    // lib.rs cadence poll, outside this manifest's source).
    "status_remediation_outcome_shown",
    // The pre-existing timers push (the timers surface's create seam —
    // the manifest's retrospective rider, caught by the totality side).
    "timers_create_accepted",
    // The detail-cell push, per dispatch origin (#1100 — the round-9
    // chunk-11 exemption split back into exactly-once seams): the
    // focus-origin restore's kinds 4/5 ride `focus_detail_cell`; the
    // effort field's keyboard opener (BeginEffortEdit's kind 6) rides
    // its own `focus_effort_opener` — the *seam* is per-origin, the
    // *fact* is per-anchor (both publish the same FocusFacts shape).
    "focus_detail_cell",
    "focus_effort_opener",
];

/// The push functions the manifest deliberately does not count: the
/// pre-framework window functions the adapter already drove (the
/// D15 hand-off's view refocus lives in the chunk plan's census
/// note, not this manifest).
const INVOKE_EXEMPT: &[&str] = &[
    "invoke_refocus_view",
    // The status panel's failure posture (OP3 W-3): the typed
    // dropped-reply render serves two deliberate callers — the
    // dispatch's full/closed lane and the drain's closed arm — so the
    // exactly-once count does not apply; both occurrences are still
    // `window.invoke_` pushes, so the no-unlisted totality side
    // counts them toward the manifest's coverage arithmetic.
    "invoke_status_failure_shown",
];

fn invoke_counts() -> Vec<(String, usize)> {
    INVOKE_INVENTORY
        .iter()
        .map(|name| {
            let needle = format!("invoke_{name}");
            (
                (*name).to_owned(),
                word_bounded_matches(ADAPTER_SOURCE, &needle),
            )
        })
        .collect()
}

#[test]
fn every_listed_push_function_is_invoked_exactly_once() {
    assert_exactly_once(invoke_counts(), "push-function invocations");
}

#[test]
fn no_unlisted_push_functions_exist() {
    let exempt = INVOKE_EXEMPT
        .iter()
        .map(|name| ADAPTER_SOURCE.matches(name).count())
        .sum::<usize>();
    let total = ADAPTER_SOURCE.matches("window.invoke_").count() - exempt;
    assert_eq!(
        total,
        INVOKE_INVENTORY.len(),
        "adapter.rs invokes {total} push functions but the inventory lists {} — an added push must be listed; a dropped one already failed the totality side",
        INVOKE_INVENTORY.len()
    );
}
