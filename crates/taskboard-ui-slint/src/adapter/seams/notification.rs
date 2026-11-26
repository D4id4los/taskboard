// SPDX-License-Identifier: AGPL-3.0-only

use std::cell::RefCell;
use std::rc::Rc;

use super::Adapter;
use crate::AppWindow;
use crate::actions::ActionId;
use crate::adapter::install_seam;
use slint::ComponentHandle as _;

/// The notification row at `index` (D2: the index resolves against the
/// refresh pass's row table).
impl Adapter {
    /// The notification walk's step (US-6/W-6, D-N1): the first
    /// stroke arms at row 0; a step clamps at both ends (no wrap).
    /// The selection is the surface's pushed highlight (the walk's
    /// render).
    pub(in crate::adapter) fn notification_walk(&mut self, window: &AppWindow, direction: i32) {
        let len = self.tables.notifications.len();
        if len == 0 {
            return;
        }
        let cursor = match self.notification_cursor {
            Some(cursor) => {
                let next =
                    i64::from(u32::try_from(cursor).unwrap_or(u32::MAX)) + i64::from(direction);
                let max = i64::from(u32::try_from(len - 1).unwrap_or(u32::MAX));
                usize::try_from(next.clamp(0, max)).unwrap_or(0)
            }
            None => 0,
        };
        self.notification_cursor = Some(cursor);
        self.sync_view_context(window);
        window.set_notification_selected(i32::try_from(cursor).unwrap_or(0));
        self.finish_mutation(window);
    }

    /// The verb dispatch's shared body (US-6/W-6, D-N1): arms at row
    /// 0 when disengaged, resolves the selected row, and runs exactly
    /// the seam body the row's button callback runs.
    pub(in crate::adapter) fn apply_notification_verb(
        &mut self,
        window: &AppWindow,
        verb: crate::router::NotificationVerb,
    ) {
        use crate::router::NotificationVerb as V;
        let cursor = *self.notification_cursor.get_or_insert(0);
        let cursor = i32::try_from(cursor).unwrap_or(i32::MAX);
        let Some(row) = notification_row(self, cursor) else {
            return;
        };
        match verb {
            V::Dismiss => self.notification_dismiss(&row),
            V::Done => self.notification_done(&row),
            V::Snooze => self.notification_snooze(window, &row),
            V::Mute => self.notification_mute(window, &row),
            V::Cancel => self.notification_cancel(window, &row),
            V::TrackStart => self.notification_track_start(window, &row),
            V::Open => self.notification_open(window, &row),
        }
        self.finish_mutation(window);
    }

    /// The dismiss seam's body (the row's button callback's dispatch).
    /// `&self` — the `bare_ref` seam install borrows non-mut.
    fn notification_dismiss(&self, row: &crate::presentation::NotificationRow) {
        self.send(taskboard_domain::StateCommand::DismissReminder {
            reminder: row.reminder,
            occurrence: row.occurrence,
        });
    }

    /// The done seam's body: the row's task completes; an alarm row
    /// has no task (the button's absence is the presentation's job —
    /// the stray degrades to a no-op).
    fn notification_done(&self, row: &crate::presentation::NotificationRow) {
        let Some(task) = row.task else { return };
        self.send(taskboard_domain::StateCommand::SetTaskDone {
            id: task,
            done: true,
        });
    }

    /// The snooze seam's body: the routed verb over the one-shot
    /// target (the row's anchor mirrors the ref).
    fn notification_snooze(
        &mut self,
        window: &AppWindow,
        row: &crate::presentation::NotificationRow,
    ) {
        self.reminder_target = Some(notification_target(row));
        self.route(window, ActionId::ReminderSnooze);
    }

    /// The mute seam's body (the snooze twin's routed verb).
    fn notification_mute(
        &mut self,
        window: &AppWindow,
        row: &crate::presentation::NotificationRow,
    ) {
        self.reminder_target = Some(notification_target(row));
        self.route(window, ActionId::ReminderMute);
    }

    /// The cancel seam's body: the alarm row's off-switch; a task row
    /// degrades to a no-op (the done-seam's guard shape).
    fn notification_cancel(
        &mut self,
        window: &AppWindow,
        row: &crate::presentation::NotificationRow,
    ) {
        let taskboard_domain::ReminderRef::Adhoc(alarm) = row.reminder else {
            return;
        };
        self.timer_target = Some(alarm);
        self.route(window, ActionId::TimerCancel);
    }

    /// The track-start seam's body: exactly one command leaves the UI
    /// (the implicit dismissal lives planner-side).
    fn notification_track_start(
        &mut self,
        window: &AppWindow,
        row: &crate::presentation::NotificationRow,
    ) {
        let Some(task) = row.task else { return };
        self.track_target = Some(task);
        self.route(window, ActionId::TaskTrackStart);
    }

    /// The row-open seam's body: focus, then the `TaskEdit` route.
    fn notification_open(
        &mut self,
        window: &AppWindow,
        row: &crate::presentation::NotificationRow,
    ) {
        let Some(task) = row.task else { return };
        self.view_state.set_focused(Some(task));
        self.route(window, ActionId::TaskEdit);
    }
}
pub(in crate::adapter) fn notification_row(
    adapter: &Adapter,
    index: i32,
) -> Option<crate::presentation::NotificationRow> {
    adapter
        .tables
        .notifications
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .cloned()
}

/// The done checkbox's dispatch (r3 D6): the toggled `SetTaskDone`
/// for the addressed task — the raw-send the notification Done button
/// set (the registry's complete/reopen act on the focused task).
/// The importance picker's slot mapping (UX R-3 D-1, #788): slots
/// 1..=5 are the domain's declaration order; anything else is the
/// stale-index absorb (`None`). Slot 0 (unset) is handled at the
/// seam before these run.
pub(in crate::adapter) fn slot_importance(slot: usize) -> Option<taskboard_domain::Importance> {
    match slot {
        1 => Some(taskboard_domain::Importance::VeryLow),
        2 => Some(taskboard_domain::Importance::Low),
        3 => Some(taskboard_domain::Importance::Normal),
        4 => Some(taskboard_domain::Importance::High),
        5 => Some(taskboard_domain::Importance::VeryHigh),
        _ => None,
    }
}

/// The urgency picker's slot mapping — the importance helper's twin
/// (distinct types by the domain's own ruling, one slot grammar).
pub(in crate::adapter) fn slot_urgency(slot: usize) -> Option<taskboard_domain::Urgency> {
    match slot {
        1 => Some(taskboard_domain::Urgency::VeryLow),
        2 => Some(taskboard_domain::Urgency::Low),
        3 => Some(taskboard_domain::Urgency::Normal),
        4 => Some(taskboard_domain::Urgency::High),
        5 => Some(taskboard_domain::Urgency::VeryHigh),
        _ => None,
    }
}

/// The card play/pause control's routing (UX-R8 chunk-5 D-6, US-4;
/// re-rooted onto the smart toggle's registry id — J3 R-8, round 9
/// chunk 11): the seam only addresses the target (the one-shot
/// `track_target`, exact even off-focus) and routes
/// `TaskTrackToggle` — the state read and the Start/Stop outcome
/// choice live in the router arm (one semantics, one home; the
/// D-TU5 addressing is unchanged). A running-elsewhere subject's
/// Start refusal is the engine's typed `NoOps` (the dispatch-time
/// gating posture).
pub(in crate::adapter) fn send_card_track_toggle(
    adapter: &mut Adapter,
    window: &AppWindow,
    task: taskboard_domain::TaskId,
) {
    adapter.track_target = Some(task);
    adapter.route(window, ActionId::TaskTrackToggle);
}

pub(in crate::adapter) fn send_toggled_done(adapter: &Adapter, task: taskboard_domain::TaskId) {
    let done_now = adapter
        .state
        .load()
        .tasks
        .get(&task)
        .is_some_and(|task| task.done.is_some());
    adapter.send(taskboard_domain::StateCommand::SetTaskDone {
        id: task,
        done: !done_now,
    });
}
/// The notification surface's seams (R3 chunk C, design D-10): each
/// button dispatches exactly one engine command and nothing else (the
/// thin-client rule).
pub(in crate::adapter) fn install_notification_seams(
    window: &AppWindow,
    adapter: &Rc<RefCell<Adapter>>,
) {
    // Dismiss: the synced dismissal fact (the universal surface's one
    // write command).
    install_seam!(window: window.on_notification_dismiss_clicked, adapter: adapter; [bare_ref]
        bind (a), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.notification_dismiss(&row);
    });

    // Mark Done: the existing done command — the planner emits the
    // dismissal rows for the task's active occurrences (R3.5.5). An
    // alarm row has no task to done (the button's absence is the
    // presentation's job; a stray index degrades to a no-op).
    install_seam!(window: window.on_notification_done_clicked, adapter: adapter; [bare_ref]
        bind (a), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.notification_done(&row);
    });

    // Snooze / Mute: the banner's routed verbs over the one-shot
    // target — the same registry actions, unchanged. The target's
    // anchor mirrors the ref (task rows task-anchored; alarm rows
    // adhoc-anchored — mute has no button there anyway).
    // The snooze choice flyout's pick (H4 D3, US-6/S2): the row's
    // chosen span routes the typed delta (the keyboard verb keeps its
    // fixed 1-hour hop — the grammar gains no chord).
    install_seam!(window: window.on_notification_snooze_span_picked, adapter: adapter;
        bind mut (a, window), args (index, span), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.send(taskboard_domain::StateCommand::SnoozeReminder {
            reminder: row.reminder,
            occurrence: row.occurrence,
            postpone: crate::router::snooze_span(usize::try_from(span).unwrap_or(usize::MAX)),
        });
        a.finish_mutation(&window);
    });

    install_seam!(window: window.on_notification_mute_clicked, adapter: adapter;
        bind mut (a, window), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.notification_mute(&window, &row);
    });

    // Cancel (DR-3/3, D-I7): the alarm row's off-switch — the routed
    // TimerCancel verb over the one-shot target. A stray or task row
    // degrades to a no-op (the done-seam's guard shape); the row
    // disappears by derivation at the next refresh.
    install_seam!(window: window.on_notification_cancel_clicked, adapter: adapter;
        bind mut (a, window), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.notification_cancel(&window, &row);
    });

    // Start Task (DR-5/4, D-TU7): the routed TaskTrackStart over the
    // row's one-shot target — table addressing, not focus (D-TU5).
    // Exactly one command leaves the UI: the implicit dismissal lives
    // planner-side (D-T5's same-batch emission); a stray index or an
    // alarm row (no task behind the anchor table) degrades to a no-op,
    // the done-seam's guard shape. The issue #210 reservation
    // resolves here.
    install_seam!(window: window.on_notification_track_start_clicked, adapter: adapter;
        bind mut (a, window), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.notification_track_start(&window, &row);
    });
    // D-6.1 (round 6): the row's double-click open — focus, then the
    // TaskEdit route (the today-row activation's shape; Activate's
    // `RefreshNow` performs the push). Alarm rows never open: the
    // markup's `enabled: row.can-open` gates, and the seam re-gates on
    // the row's task (belt and braces). The one-shot reminder target
    // is deliberately not armed here — nothing on this path consumes
    // it; every snooze/mute seam arms it itself before routing.
    install_seam!(window: window.on_notification_row_open_requested, adapter: adapter;
        bind mut (a, window), args (index), {
        let Some(row) = notification_row(&a, index) else {
            return;
        };
        a.notification_open(&window, &row);
    });
}

/// The one-shot reminder target a notification row feeds the routed
/// snooze/mute verbs with.
pub(in crate::adapter) fn notification_target(
    row: &crate::presentation::NotificationRow,
) -> crate::presentation::ReminderTarget {
    let anchor = match row.reminder {
        taskboard_domain::ReminderRef::Adhoc(alarm) => {
            taskboard_domain::ReminderAnchor::Alarm(alarm)
        }
        _ => taskboard_domain::ReminderAnchor::Task(
            row.task
                .expect("a non-adhoc notification row carries its task"),
        ),
    };
    crate::presentation::ReminderTarget {
        anchor,
        reminder: row.reminder,
        occurrence: row.occurrence,
    }
}
