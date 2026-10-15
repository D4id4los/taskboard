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
            },
        )
    }

    /// Drains every command that reached the boundary (typed capture),
    /// appending to the received log.
    pub fn drain(&mut self) -> Vec<StateCommand> {
        let drained: Vec<_> = std::iter::from_fn(|| self.commands.try_recv().ok()).collect();
        self.received.extend(drained.iter().cloned());
        drained
    }

    /// The commands that reached the boundary so far (assertion input:
    /// exact `StateCommand` variants — the routing-only rule).
    #[must_use]
    pub fn received(&self) -> &[StateCommand] {
        &self.received
    }

    /// Applies drained commands to the state snapshot for journey
    /// continuity (insert/create, mark done, move, delete, retitle),
    /// stores, and triggers the UI's deterministic refresh.
    pub fn settle(&mut self, ui: &UiHandle) {
        let drained = self.drain();
        if drained.is_empty() {
            return;
        }
        let mut state = self.state.load_full().as_ref().clone();
        for command in drained {
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
                            done: None,
                            stack,
                            order,
                            labels: std::collections::BTreeSet::new(),
                            archived: false,
                            deleted: false,
                            clocks: taskboard_domain::TaskClocks {
                                title: taskboard_domain::BoardClocks::epoch(),
                                description: taskboard_domain::BoardClocks::epoch(),
                                duedate: taskboard_domain::BoardClocks::epoch(),
                                done: taskboard_domain::BoardClocks::epoch(),
                                position: taskboard_domain::BoardClocks::epoch(),
                                labels: taskboard_domain::BoardClocks::epoch(),
                                archived: taskboard_domain::BoardClocks::epoch(),
                                deleted: taskboard_domain::BoardClocks::epoch(),
                            },
                            remote_seen: None,
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
                StateCommand::MoveTask { id, stack, order } => {
                    if let Some(task) = state.tasks.get_mut(&id) {
                        task.stack = stack;
                        task.order = order;
                    }
                }
                StateCommand::DeleteTask { id } => {
                    if let Some(task) = state.tasks.get_mut(&id) {
                        task.deleted = true;
                    }
                }
                // The journey session exercises only the task verbs
                // above; anything else is continuity-relevant never —
                // log-and-keep (the received log still records it).
                other => tracing::debug!(?other, "fake engine: unapplied command"),
            }
        }
        self.state.store(Arc::new(state));
        ui.refresh();
    }
}
