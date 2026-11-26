// SPDX-License-Identifier: AGPL-3.0-only

use super::Adapter;
use crate::adapter::install_seam;
use slint::ComponentHandle as _;
use std::cell::RefCell;
use std::rc::Rc;

use crate::AppWindow;
use crate::actions::ActionId;
use crate::router::UiEffect;

pub(in crate::adapter) fn install_toolbar_seam(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_toolbar_menu_clicked, adapter: adapter;
    bind mut (a, window), args (), {        if a.shell.menu.open {
        // The toggle is a full close from any level (round-1 D6).
        a.apply_effect(&window, UiEffect::CloseMenu);
    } else {
        a.route(&window, ActionId::AppMenu);
    }

    });
    // The inbox chip's click routes `view.inbox` (§7.1: every click
    // has an action equivalent; chunk 10 D9).
    install_seam!(window: window.on_toolbar_inbox_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ViewInbox);

    });
    // The view switcher's clicks (round-3 D22): each routes the same
    // registry action the keyboard uses — the leaving-rule settle
    // applies unchanged (a switcher click under a live flow settles
    // it first, the `FocusBoard` precedent).
    install_seam!(window: window.on_toolbar_switcher_board_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::NavFocusBoard);

    });
    install_seam!(window: window.on_toolbar_switcher_today_clicked, adapter: adapter;
    bind mut (a, window), args (), {        a.route(&window, ActionId::ViewToday);

    });
}
