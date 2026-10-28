// SPDX-License-Identifier: AGPL-3.0-only

//! The forwarder-boundary fake (design §8.1, chunk decision D4): a
//! test fake stands in for the engine; AGENTS §5 fakes-over-mocks.
//! Owns the receiver half of the UI command channel — exactly where
//! `spawn_ui_command_forwarder` would sit in the app — plus the
//! `ArcSwap` it feeds.
//!
//! CONTRACT: `settle`'s application is *fixture continuity, not an
//! oracle* — the simplest structural change that makes the next
//! journey step's precondition true. It is deliberately not tested:
//! the state engine's own `insta` suite owns command semantics.

use std::sync::Arc;

use arc_swap::ArcSwap;
use taskboard_domain::{AppState, StateCommand};
use taskboard_ui_slint::UiHandle;
use tokio::sync::mpsc;

pub struct FakeEngine {
    commands: mpsc::Receiver<StateCommand>,
    state: Arc<ArcSwap<AppState>>,
    received: Vec<StateCommand>,
    /// The prefix of `received` already folded into the state
    /// snapshot (assertions drain the channel first; `settle`
    /// applies from the log, not the channel).
    applied: usize,
}

impl FakeEngine {
    /// Opens the boundary channel over `state` (the same `ArcSwap`
    /// [`taskboard_ui_slint::mount`] reads).
    #[must_use]
    pub fn channel(state: Arc<ArcSwap<AppState>>) -> (mpsc::Sender<StateCommand>, Self) {
        let (tx, rx) = mpsc::channel(64);
        (
            tx,
            Self {
                commands: rx,
                state,
                received: Vec::new(),
                applied: 0,
            },
        )
    }

    /// Drains every command that reached the boundary (typed capture),
    /// appending to the received log. The drained prefix is the
    /// journeys' assertion input: exact `StateCommand` variants — the
    /// routing-only rule.
    pub fn drain(&mut self) -> Vec<StateCommand> {
        let drained: Vec<_> = std::iter::from_fn(|| self.commands.try_recv().ok()).collect();
        self.received.extend(drained.iter().cloned());
        drained
    }

    /// Applies drained commands to the state snapshot for journey
    /// continuity (insert/create, mark done, move, delete, retitle),
    /// stores, and triggers the UI's deterministic refresh.
    #[allow(clippy::too_many_lines)] // one wiring match, kept flat
    pub fn settle(&mut self, ui: &UiHandle) {
        self.drain();
        let unapplied = &self.received[self.applied..];
        if unapplied.is_empty() {
            return;
        }
        self.applied = self.received.len();
        let mut state = self.state.load_full().as_ref().clone();
        for command in unapplied.iter().cloned() {
            match command {
                StateCommand::CreateTask {
                    title,
                    stack,
                    order,
                } => {
                    let id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
                    state.tasks.insert(
                        id,
                        taskboard_domain::Task {
                            id,
                            remote: None,
                            title,
                            description: String::new(),
                            duedate: None,
                            scheduled: None,
                            done: None,
                            stack,
                            order,
                            label_memberships: std::collections::BTreeMap::new(),
                            effort: None,
                            importance: None,
                            urgency: None,
                            recur: None,

                            archived: false,
                            deleted: false,
                            clocks: taskboard_domain::TaskClocks {
                                title: taskboard_domain::BoardClocks::epoch(),
                                description: taskboard_domain::BoardClocks::epoch(),
                                duedate: taskboard_domain::BoardClocks::epoch(),
                                scheduled: taskboard_domain::BoardClocks::epoch(),
                                done: taskboard_domain::BoardClocks::epoch(),
                                position: taskboard_domain::BoardClocks::epoch(),
                                effort: taskboard_domain::BoardClocks::epoch(),
                                importance: taskboard_domain::BoardClocks::epoch(),
                                urgency: taskboard_domain::BoardClocks::epoch(),
                                recur: taskboard_domain::BoardClocks::epoch(),

                                archived: taskboard_domain::BoardClocks::epoch(),
                                deleted: taskboard_domain::BoardClocks::epoch(),
                            },
                            remote_seen: None,
                            remote_baseline: None,
                        },
                    );
                }
                StateCommand::UpdateTask { id, changes } => {
                    if let Some(task) = state.tasks.get_mut(&id) {
                        if let Some(title) = changes.title {
                            task.title = title;
                        }
                        if let Some(duedate) = changes.duedate {
                            task.duedate = duedate;
                        }
                        if let Some(description) = changes.description {
                            task.description = description;
                        }
                        if let Some(scheduled) = changes.scheduled {
                            task.scheduled = scheduled;
                        }
                        if let Some(archived) = changes.archived {
                            task.archived = archived;
                        }
                    }
                }
                StateCommand::SetTaskDone { id, done } => {
                    if let Some(task) = state.tasks.get_mut(&id) {
                        task.done = done.then(chrono::Utc::now);
                    }
                }
                // The timers verbs (DR-3/3, D-P13): the planner's
                // `In` arm mirrors the engine's stamp (now + dur,
                // `At` verbatim); cancel is terminal-by-deletion.
                StateCommand::CreateAdhocAlarm {
                    label,
                    when,
                    insistence: taskboard_domain::Insistence::Normal,
                } => {
                    let id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
                    let fire_at = match when {
                        taskboard_domain::AdhocWhen::At(at) => at,
                        taskboard_domain::AdhocWhen::In(dur) => chrono::Utc::now() + dur,
                    };
                    state.reminders.alarms.insert(
                        id,
                        taskboard_domain::AdhocAlarm {
                            insistence: taskboard_domain::Insistence::Normal,
                            id,
                            label,
                            fire_at,
                            created_at: chrono::Utc::now(),
                        },
                    );
                }
                StateCommand::CancelAdhocAlarm { alarm } => {
                    state.reminders.alarms.remove(&alarm);
                }
                StateCommand::DismissReminder {
                    reminder,
                    occurrence,
                } => {
                    state
                        .reminders
                        .dismissals
                        .insert(taskboard_domain::ReminderDismissal {
                            reminder,
                            occurrence,
                            dismissed_at: chrono::Utc::now(),
                        });
                }
                StateCommand::MoveTask { id, stack, order } => {
                    if let Some(task) = state.tasks.get_mut(&id) {
                        task.stack = stack;
                        task.order = order;
                    }
                }
                StateCommand::MoveStack { id, order } => {
                    if let Some(stack) = state.stacks.get_mut(&id) {
                        stack.order = order;
                    }
                }
                StateCommand::DeleteTask { id } => {
                    if let Some(task) = state.tasks.get_mut(&id) {
                        task.deleted = true;
                    }
                }
                // The journey session exercises only the task verbs
                // and the stack reorder above; anything else is
                // continuity-relevant never — log-and-keep (the
                // received log still records it).
                other => tracing::debug!(?other, "fake engine: unapplied command"),
            }
        }
        self.state.store(Arc::new(state));
        ui.refresh();
    }

    /// Replaces the whole state snapshot and triggers the UI's
    /// deterministic refresh (the D11 drag journey's "a concurrent
    /// engine change lands mid-gesture" step).
    pub fn replace_state(&mut self, state: AppState, ui: &UiHandle) {
        self.state.store(Arc::new(state));
        ui.refresh();
    }
}
