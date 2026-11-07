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

const ADAPTER_SOURCE: &str = include_str!("../src/adapter.rs");

/// The 175 installed window callbacks in `adapter.rs` as of the
/// focus-settings-editor chunk (plan §2.1 census; the #275 close
/// comment counted 110 at `5d8c0751` — the seam surface grows ~30
/// installs per GUI
/// round, so this list changes on purpose when a new seam lands, and
/// both test sides force the list to stay honest).
const SEAM_INVENTORY: &[&str] = &[
    "banner_mute_clicked",
    "banner_row_activated",
    "banner_snooze_clicked",
    "board_add_task_activated",
    "board_bar_segment_activated",
    "board_body_activated",
    "board_body_context_requested",
    "board_card_drag_armed",
    "board_card_drag_finished",
    "board_card_dropped",
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
    "board_task_clicked",
    "board_task_context_requested",
    "board_task_done_toggled",
    "capture_accepted",
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
    "walk_scrim_clicked",
    "walk_tomorrow_clicked",
    "focus_switcher_invoked",
    "habits_convert_armed",
    "habits_convert_cancelled",
    "habits_convert_confirmed",
    "habits_plus_clicked",
    "habits_reorder_asked",
    "habits_repair_clicked",
    "intercept_key",
    "menu_dismiss_requested",
    "menu_entry_invoked",
    "menu_level_back",
    "menu_submenu_entry_invoked",
    "notification_cancel_clicked",
    "notification_dismiss_clicked",
    "notification_done_clicked",
    "notification_mute_clicked",
    "notification_snooze_clicked",
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
    "palette_row_activated",
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
    "task_date_quick_typed",
    "task_date_time_accepted",
    "task_date_time_edited",
    "task_date_time_picked",
    "task_description_committed",
    "task_description_edited",
    "task_description_edit_opened",
    "task_done_toggled",
    "task_due_edit_opened",
    "task_recur_convert_asked",
    "task_recur_convert_cancelled",
    "task_recur_convert_confirmed",
    "task_recur_pause_accepted",
    "task_recur_pause_asked",
    "task_recur_pause_cancelled",
    "task_recur_resume_clicked",
    "task_rule_jump_to_due_clicked",
    "task_rule_remove_clicked",
    "task_rule_unmute_clicked",
    "task_scheduled_edit_opened",
    "task_title_committed",
    "task_title_edit_activated",
    "task_title_edited",
    "task_track_entries_clicked",
    "task_track_start_clicked",
    "task_track_stop_clicked",
    "text_flow_focus_lost",
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

fn install_counts() -> Vec<(String, usize)> {
    SEAM_INVENTORY
        .iter()
        .map(|name| {
            // A seam is spelled `window.on_<name>(` when wired
            // explicitly and `window.on_<name>,` at an
            // `install_seam!` invocation; a longer name extending
            // this one must not count.
            let needle = format!("window.on_{name}");
            let count = ADAPTER_SOURCE
                .match_indices(&needle)
                .filter(|(at, _)| {
                    ADAPTER_SOURCE[at + needle.len()..]
                        .chars()
                        .next()
                        .is_none_or(|next| !matches!(next, 'a'..='z' | '0'..='9' | '_'))
                })
                .count();
            ((*name).to_owned(), count)
        })
        .collect()
}

#[test]
fn every_listed_seam_is_installed_exactly_once() {
    let deviations: Vec<String> = install_counts()
        .into_iter()
        .filter_map(|(name, count)| (count != 1).then(|| format!("{name}: {count} installs")))
        .collect();
    assert!(
        deviations.is_empty(),
        "seam installs deviate from exactly-once:\n{}",
        deviations.join("\n")
    );
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
