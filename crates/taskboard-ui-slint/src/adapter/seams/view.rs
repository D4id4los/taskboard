// SPDX-License-Identifier: AGPL-3.0-only

use super::{
    Adapter, install_banner_seams, install_board_bar_archived_seam, install_board_bar_seam,
    install_board_color_seams, install_board_context_seam, install_board_seams,
    install_delete_strip_seams, install_drag_seams, install_due_seam, install_export_seam,
    install_focus_seams, install_habit_editor_seams, install_habit_seams,
    install_menu_dismiss_seam, install_notification_seams, install_object_context_seams,
    install_picker_grid_seam, install_recur_editor_seams, install_recur_seams,
    install_review_ledger_seams, install_review_walk_seams, install_settings_back_seam,
    install_settings_seams, install_stack_context_seam, install_stack_create_seams,
    install_stack_done_seam, install_stack_seams, install_stack_width_seams,
    install_status_panel_seam, install_task_back_seam, install_task_seam, install_timer_seams,
    install_today_row_seams, install_toolbar_seam, install_tracking_seams, install_undo_seams,
};
use std::cell::RefCell;
use std::rc::Rc;

use crate::AppWindow;

/// The view dispatch seams (D22): every index-carrying callback
/// resolves against the id tables of the last refresh pass (D2) and
/// lands in the view-state machine or the router — never in ad-hoc
/// logic. Each handler ends in a refresh so the models carry the new
/// state (the adapter applies `view_state` outcomes verbatim, D9).
pub(in crate::adapter) fn install_view_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_toolbar_seam(window, adapter);
    install_status_panel_seam(window, adapter);
    install_menu_dismiss_seam(window, adapter);
    install_undo_seams(window, adapter);
    install_board_bar_seam(window, adapter);
    install_board_color_seams(window, adapter);
    install_board_bar_archived_seam(window, adapter);
    install_delete_strip_seams(window, adapter);
    install_stack_create_seams(window, adapter);
    install_board_seams(window, adapter);
    install_stack_seams(window, adapter);
    install_stack_done_seam(window, adapter);
    install_today_row_seams(window, adapter);
    install_task_seam(window, adapter);
    install_picker_grid_seam(window, adapter);
    install_due_seam(window, adapter);
    install_task_back_seam(window, adapter);
    install_settings_back_seam(window, adapter);
    install_board_context_seam(window, adapter);
    install_stack_context_seam(window, adapter);
    install_object_context_seams(window, adapter);
    install_stack_width_seams(window, adapter);
    install_drag_seams(window, adapter);
    install_banner_seams(window, adapter);
    install_notification_seams(window, adapter);
    install_timer_seams(window, adapter);
    install_tracking_seams(window, adapter);
    install_focus_seams(window, adapter);
    install_review_walk_seams(window, adapter);
    install_review_ledger_seams(window, adapter);
    install_habit_seams(window, adapter);
    install_recur_seams(window, adapter);
    install_recur_editor_seams(window, adapter);
    install_export_seam(window, adapter);
    install_habit_editor_seams(window, adapter);
    install_settings_seams(window, adapter);
}
