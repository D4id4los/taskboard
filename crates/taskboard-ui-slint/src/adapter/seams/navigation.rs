// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle as _;

use crate::AppWindow;
use crate::actions::ActionId;

use super::Adapter;
use super::{open_stack_context, select_column};
use crate::adapter::install_seam;

/// The stack card's bare-surface body seams (r3 D4 / r4 D3): the
/// right-down context arm and the left-click select arm — one home so
/// they cannot drift from the header's pair.
pub(in crate::adapter) fn install_body_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_board_body_context_requested, adapter: adapter;
    bind mut (a, window), args (column, x, y), {        open_stack_context(&mut a, &window, column, x, y);

    });
    install_seam!(window: window.on_board_body_activated, adapter: adapter;
    bind mut (a, window), args (column), {        select_column(&mut a, &window, column);

    });
}
/// The task-detail back button (round-1 D12): routes
/// `nav.focus-board` — the existing `focus_board` outcome, no new
/// view-state semantics.
pub(in crate::adapter) fn install_task_back_seam(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    install_seam!(window: window.on_task_back_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::NavFocusBoard);

    });
}
