// SPDX-License-Identifier: AGPL-3.0-only

//! The seam-install families (chunk E1 of the adapter decomposition,
//! .artifacts/plans/2026-11-25-adapter-decomposition-plan.md §2): one
//! file per `install_*` family, moved verbatim out of the former
//! `adapter.rs`. Mechanical split only — every item keeps its
//! original body; visibility is widened to `pub(super)` exactly where
//! a sibling or the module root calls it.

use super::{
    Adapter, FocusHandover, MenuAnchor, MenuKind, MenuSubject, commit_text, install_banner_seams,
    install_board_bar_archived_seam, install_board_bar_seam, install_board_color_seams,
    install_board_seams, install_delete_strip_seams, install_focus_seams,
    install_notification_seams, install_review_ledger_seams, install_review_walk_seams,
    install_settings_back_seam, install_settings_seams, install_stack_create_seams,
    install_stack_done_seam, install_stack_seams, install_task_seam, install_timer_seams,
    install_tracking_seams, parse_effort_row, select_column, send_card_track_toggle,
    send_toggled_done, settle_pointer_conflict, settle_pointer_conflict_keeping_selection,
};

mod banner;
mod context;
mod drag;
mod due;
mod export;
mod habit;
mod navigation;
mod recur;
mod shell;
mod stack_width;
mod status_panel;
mod today;
mod toolbar;
mod undo;
mod view;

pub(in crate::adapter) use banner::banner_row;
pub(in crate::adapter) use context::{
    install_board_context_seam, install_menu_dismiss_seam, install_object_context_seams,
    install_stack_context_seam, open_stack_context,
};
pub(in crate::adapter) use drag::install_drag_seams;
pub(in crate::adapter) use due::{commit_effort_editor, settle_date_editor};
pub(in crate::adapter) use due::{install_due_seam, install_picker_grid_seam};
pub(in crate::adapter) use export::install_export_seam;
pub(in crate::adapter) use habit::{habit_open_entry, habit_undo_target};
pub(in crate::adapter) use habit::{install_habit_editor_seams, install_habit_seams};
pub(in crate::adapter) use navigation::{install_body_seams, install_task_back_seam};
pub(in crate::adapter) use recur::{install_recur_editor_seams, install_recur_seams};
pub(in crate::adapter) use shell::close_switcher_flyout;
pub(in crate::adapter) use stack_width::install_stack_width_seams;
pub(in crate::adapter) use status_panel::{install_status_panel_seam, status_panel_data};
pub(in crate::adapter) use today::install_today_row_seams;
pub(in crate::adapter) use toolbar::install_toolbar_seam;
pub(in crate::adapter) use undo::install_undo_seams;
pub(in crate::adapter) use view::install_view_seams;
