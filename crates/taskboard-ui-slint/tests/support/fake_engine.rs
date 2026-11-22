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
    /// The rule rows the settle removed, newest last — the undo
    /// line's continuity source (UF1's journeys: `Undo` after a rule
    /// removal restores the row; fixture continuity, not an oracle).
    removed_rules: Vec<taskboard_domain::ReminderRule>,
    /// The rule ids the settle minted for an add, newest last — the
    /// undo line's other continuity face (`Undo` after an add deletes
    /// the added rule).
    added_rules: Vec<taskboard_domain::ReminderRuleId>,
}

impl FakeEngine {
    /// The current state snapshot (the journeys' read seam — the
    /// settle's application is observable through it).
    pub fn snapshot(&self) -> arc_swap::Guard<Arc<AppState>> {
        self.state.load()
    }
}

/// The compact-move pair's fake-side resolution (US-14, #1031): the
/// same stack-adjacent slot the real engine plans — one step toward
/// the named edge through the shared floor-midpoint arithmetic; the
/// boundary is the fake's no-op (the typed refusal is the real
/// engine's).
fn apply_compact_move(state: &mut AppState, id: taskboard_domain::TaskId, up: bool) {
    let Some(subject) = state.tasks.get(&id).cloned() else {
        return;
    };
    let mut rows: Vec<&taskboard_domain::entities::Task> = state
        .tasks
        .values()
        .filter(|t| t.is_live() && t.stack == subject.stack)
        .collect();
    rows.sort_by_key(|t| (t.order, t.id));
    let Some(position) = rows.iter().position(|t| t.id == id) else {
        return;
    };
    let prev = position
        .checked_sub(1)
        .and_then(|i| rows.get(i))
        .map(|t| t.order);
    let next = rows.get(position + 1).map(|t| t.order);
    let order = if up {
        prev.map(|prev| {
            taskboard_domain::command::floor_midpoint_order(Some(prev), Some(subject.order))
        })
    } else {
        next.map(|next| {
            taskboard_domain::command::floor_midpoint_order(Some(subject.order), Some(next))
        })
    };
    if let (Some(order), Some(task)) = (order, state.tasks.get_mut(&id)) {
        task.order = order;
    }
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
                removed_rules: Vec::new(),
                added_rules: Vec::new(),
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
                            kind: taskboard_domain::entities::TaskKind::Standard,

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
                                kind: taskboard_domain::BoardClocks::epoch(),

                                archived: taskboard_domain::BoardClocks::epoch(),
                                deleted: taskboard_domain::BoardClocks::epoch(),
                            },
                            authors: taskboard_domain::entities::TaskAuthors::default(),
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
                        // #1024: the ratings/series/kind masks fold too
                        // (fixture continuity with the engine's
                        // `UpdateTask` application in
                        // `taskboard-domain/src/command.rs` — the
                        // oracle; each double-option field unwraps the
                        // outer layer only, `Some(None)` clears).
                        if let Some(effort) = changes.effort {
                            task.effort = effort;
                        }
                        if let Some(importance) = changes.importance {
                            task.importance = importance;
                        }
                        if let Some(urgency) = changes.urgency {
                            task.urgency = urgency;
                        }
                        if let Some(recur) = changes.recur {
                            task.recur = recur;
                        }
                        if let Some(kind) = changes.kind {
                            task.kind = kind.unwrap_or_default();
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
                // The break-rule verbs (T-14, #797): create inserts
                // the roaming row; delete is terminal-by-removal (the
                // `CancelAdhocAlarm` law).
                StateCommand::CreateBreakRule {
                    board: Some(board),
                    label,
                    threshold,
                    insistence,
                } => {
                    let id = taskboard_domain::BreakRuleId::from_uuid(uuid::Uuid::now_v7());
                    state.reminders.break_rules.insert(
                        id,
                        taskboard_domain::BreakRule {
                            id,
                            board,
                            label,
                            threshold,
                            insistence,
                            created_at: taskboard_domain::BoardClocks::epoch(),
                            by: None,
                        },
                    );
                }
                StateCommand::DeleteBreakRule { id } => {
                    state.reminders.break_rules.remove(&id);
                }
                // The labels overlay's delete (D-6, #881): terminal-by
                // -removal (the `DeleteBreakRule` law) — the roster's
                // subsequent reads see the refreshed model, nothing
                // else. Fixture continuity, not an oracle (the file's
                // contract comment).
                StateCommand::DeleteLabel { id } => {
                    state.labels.remove(&id);
                }
                // The journal verbs (T-15, #798): the edit applies the
                // exact change mask (the planner's contract — the
                // engine re-mints the ratings); the delete is the
                // in-place tombstone (the row survives, the ledger
                // renders live rows only).
                StateCommand::UpdateReflection {
                    id,
                    text,
                    mood,
                    energy,
                    ref flags_add,
                    ref flags_remove,
                } => {
                    if let Some(entry) = state.reflection.entries.get_mut(&id) {
                        if let Some(new_text) = text {
                            entry.text = new_text;
                        }
                        if let Some(mood) = mood {
                            entry.attributes.mood = mood.map(|n| {
                                taskboard_domain::reflection::Rating::try_new(n)
                                    .expect("the fixture rating is 1..=5")
                            });
                        }
                        if let Some(energy) = energy {
                            entry.attributes.energy = energy.map(|n| {
                                taskboard_domain::reflection::Rating::try_new(n)
                                    .expect("the fixture rating is 1..=5")
                            });
                        }
                        for flag in flags_add {
                            entry.attributes.flags.insert(*flag);
                        }
                        for flag in flags_remove {
                            entry.attributes.flags.remove(flag);
                        }
                    }
                }
                StateCommand::DeleteReflection { id } => {
                    if let Some(entry) = state.reflection.entries.get_mut(&id) {
                        entry.deleted = true;
                    }
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
                // The Today compact-move pair (US-14, #1031): the
                // fake resolves the same slot arithmetic the real
                // engine plans — one up-step/down-step per command.
                StateCommand::TaskMoveUp { id } => {
                    apply_compact_move(&mut state, id, true);
                }
                StateCommand::TaskMoveDown { id } => {
                    apply_compact_move(&mut state, id, false);
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
                // The archived-boards roster's verb (UX R-1 D-1): the
                // continuity half of the unarchive journey — the board
                // rejoins the live set on the settle's refresh.
                StateCommand::SetBoardArchived { board, archived } => {
                    if let Some(board) = state.boards.get_mut(&board) {
                        board.archived = archived;
                    }
                }
                // The rule cursor journeys' continuity half (J3 US-6,
                // round 9 chunk 12): the removal rejoins the roster on
                // the settle's refresh — the cursor's prune reads it.
                // UF1: the row is kept (newest last) so the undo line's
                // continuity face can restore it verbatim.
                StateCommand::RemoveReminderRule { rule } => {
                    if let Some(row) = state.reminders.rules.get(&rule) {
                        self.removed_rules.push(row.clone());
                    }
                    state.reminders.rules.remove(&rule);
                }
                // The UF1 journeys' continuity half (the `RemoveReminderRule`
                // precedent): the add mints a rule row so the next journey
                // step sees it in the presented roster.
                StateCommand::AddReminderRule {
                    task, offset, at, ..
                } => {
                    let id = taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::now_v7());
                    let kind = match at {
                        None => taskboard_domain::ReminderKind::DueOffset(offset),
                        Some(taskboard_domain::AdhocWhen::At(at)) => {
                            taskboard_domain::ReminderKind::At(
                                taskboard_domain::AtInstant::whole_second(at)
                                    .expect("whole second"),
                            )
                        }
                        Some(taskboard_domain::AdhocWhen::In(dur)) => {
                            taskboard_domain::ReminderKind::At(
                                taskboard_domain::AtInstant::whole_second(chrono::Utc::now() + dur)
                                    .expect("whole second"),
                            )
                        }
                    };
                    let now = chrono::Utc::now();
                    let row = taskboard_domain::ReminderRule {
                        insistence: taskboard_domain::Insistence::Normal,
                        id,
                        task,
                        kind,
                        created_at: now,
                        updated_at: now,
                        by: None,
                    };
                    self.added_rules.push(id);
                    state.reminders.rules.insert(id, row);
                }
                // The UF1 undo-line continuity (fixture continuity,
                // never an oracle — the state engine's fold suite owns
                // undo semantics): a rule removal restores its row, an
                // add deletes the minted rule; neither touches the
                // delivery records.
                StateCommand::Undo => {
                    if let Some(row) = self.removed_rules.pop() {
                        state.reminders.rules.entry(row.id).or_insert(row);
                    } else if let Some(added) = self.added_rules.pop() {
                        state.reminders.rules.remove(&added);
                    }
                }
                // The tracking verbs' continuity (round 9 chunk 11,
                // J3 US-4/US-5): StartTracking inserts the open
                // entry (the running timer), StopTracking stamps the
                // newest open entry's end — the smart toggle's
                // state-correct second press must see the first
                // press's entry to route the stop half.
                StateCommand::StartTracking { task, kind } => {
                    let now = chrono::Utc::now();
                    let title = state
                        .tasks
                        .get(&task)
                        .map(|task| task.title.clone())
                        .unwrap_or_default();
                    let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
                    state.tracking.entries.insert(
                        id,
                        taskboard_domain::TimeEntry {
                            id,
                            task,
                            task_title: title,
                            start: now,
                            end: None,
                            created_at: now,
                            updated_at: now,
                            kind,
                            by: None,
                        },
                    );
                }
                StateCommand::StopTracking => {
                    let now = chrono::Utc::now();
                    let newest = state
                        .tracking
                        .newest_open()
                        .map(|entry| entry.id)
                        .expect("an open entry to close");
                    if let Some(entry) = state.tracking.entries.get_mut(&newest) {
                        entry.end = Some(now);
                        entry.updated_at = now;
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
