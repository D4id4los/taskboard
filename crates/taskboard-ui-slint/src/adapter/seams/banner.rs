// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use super::Adapter;
use crate::AppWindow;
use crate::actions::ActionId;
use crate::adapter::install_seam;
use slint::ComponentHandle as _;

/// The banner row at `index` (D2: the index resolves against the
/// refresh pass's row table).
pub(in crate::adapter) fn banner_row(
    adapter: &Adapter,
    index: i32,
) -> Option<crate::presentation::BannerRow> {
    adapter
        .tables
        .banner
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
}

/// The banner seams (D12): row activation (click or Tab focus — the
/// markup's `changed has-focus` lands on the same seam) sets focus
/// state plus the one-shot reminder target (D4); snooze/mute set the
/// target and route the registry actions.
pub(in crate::adapter) fn install_banner_seams(window: &AppWindow, adapter: &Rc<RefCell<Adapter>>) {
    install_seam!(window: window.on_banner_row_activated, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(banner_row) = banner_row(&a, index) else {
        return;
    };
    a.view_state.set_focused(Some(banner_row.task));
    a.reminder_target = Some(banner_row.target);
    a.refresh_forced(&window);

    });

    install_seam!(window: window.on_banner_snooze_clicked, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(banner_row) = banner_row(&a, index) else {
        return;
    };
    a.reminder_target = Some(banner_row.target);
    a.route(&window, ActionId::ReminderSnooze);

    });

    install_seam!(window: window.on_banner_mute_clicked, adapter: adapter; // #1011 (SB1 US-3): direction-aware — the row folds `state.reminders.muted`, so a muted task's control dispatches the mute-clearing command directly (the #733 twin grammar on the banner surface); the unmuted direction keeps the routed ReminderMute verb.
    bind mut (a, window), args (index), {        let Some(banner_row) = banner_row(&a, index) else {
        return;
    };
    if banner_row.muted {
        a.send(taskboard_domain::StateCommand::SetTaskRemindersMuted {
            task: banner_row.task,
            muted: false,
        });
        a.finish_mutation(&window);
    } else {
        a.reminder_target = Some(banner_row.target);
        a.route(&window, ActionId::ReminderMute);
    }

    });

    // D-6.1 (round 6): the row's double-click open — the today-row
    // activation's shape (focus, then the TaskEdit route). The click
    // that precedes it already focused and armed the one-shot target
    // (both idempotent); TaskEdit → Activate → `RefreshNow` performs
    // the push, so no extra refresh. A stale index stays inert.
    install_seam!(window: window.on_banner_row_open_requested, adapter: adapter;
    bind mut (a, window), args (index), {        let Some(banner_row) = banner_row(&a, index) else {
        return;
    };
    a.view_state.set_focused(Some(banner_row.task));
    a.reminder_target = Some(banner_row.target);
    a.route(&window, ActionId::TaskEdit);

    });
}
