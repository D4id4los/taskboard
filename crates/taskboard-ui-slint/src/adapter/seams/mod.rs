// SPDX-License-Identifier: AGPL-3.0-only

//! The seam-install families (chunk E1 of the adapter decomposition,
//! .artifacts/plans/2026-11-25-adapter-decomposition-plan.md §2): one
//! file per `install_*` family, moved verbatim out of the former
//! `adapter.rs`. Mechanical split only — every item keeps its
//! original body; visibility is widened to `pub(super)` exactly where
//! a sibling or the module root calls it.

use super::{
    Adapter, install_banner_seams, install_board_bar_archived_seam, install_board_bar_seam,
    install_board_color_seams, install_board_context_seam, install_board_seams,
    install_delete_strip_seams, install_drag_seams, install_due_seam, install_focus_seams,
    install_habit_editor_seams, install_habit_seams, install_menu_dismiss_seam,
    install_notification_seams, install_object_context_seams, install_picker_grid_seam,
    install_review_ledger_seams, install_review_walk_seams, install_settings_back_seam,
    install_settings_seams, install_stack_context_seam, install_stack_create_seams,
    install_stack_done_seam, install_stack_seams, install_task_back_seam, install_task_seam,
    install_timer_seams, install_today_row_seams, install_tracking_seams, install_undo_seams,
    settle_pointer_conflict,
};

mod export;
mod recur;
mod stack_width;
mod status_panel;
mod toolbar;
mod view;

pub(super) use export::install_export_seam;
pub(super) use recur::{install_recur_editor_seams, install_recur_seams};
pub(super) use stack_width::install_stack_width_seams;
pub(super) use status_panel::{install_status_panel_seam, status_panel_data};
pub(super) use toolbar::install_toolbar_seam;
pub(super) use view::install_view_seams;
