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
    /// The chronological undo ledger (oldest first): every
    /// continuity-bearing mutating command pushes exactly one face;
    /// `Undo` pops the newest and applies its inverse — one level
    /// per command, across families (#1219). Fixture continuity,
    /// not an oracle.
    ledger: Vec<UndoFace>,
}

/// One continuity-bearing mutation's inverse (the ledger's entry,
/// #1219): the face an `Undo` pops, in the order the mutations
/// landed.
enum UndoFace {
    /// A removed rule row, restored verbatim (UF1's journeys).
    RemovedRule(taskboard_domain::ReminderRule),
    /// An add's minted id, deleted on the undo.
    AddedRule(taskboard_domain::ReminderRuleId),
    /// A restored entity, re-tombstoned (the #1187 law; the domain
    /// fold suite owns the undo semantics).
    RestoredEntity(RestoredEntity),
    /// A cleared batch, restored verbatim (W-7: one undo step
    /// restores all n).
    ClearedBatch(Vec<taskboard_domain::ReminderRule>),
    /// A habit reorder's pre-move section order (#1198 chunk A1 D1;
    /// the domain's inverse suite owns undo semantics).
    ReorderedSections(Vec<(taskboard_domain::HabitId, i64)>),
}

/// One restored entity's continuity fact (the fake's undo face).
enum RestoredEntity {
    Label(taskboard_domain::LabelId),
    Habit(taskboard_domain::HabitId),
}

impl FakeEngine {
    /// The current state snapshot (the journeys' read seam — the
    /// settle's application is observable through it).
    pub fn snapshot(&self) -> arc_swap::Guard<Arc<AppState>> {
        self.state.load()
    }
}

/// The compact-move pair's fake-side resolution (US-14, #1031; B3
/// 2026-11-24 #1247): the same gap-index strokes the real engine
/// plans through `task_reorder_plan` — up targets the row above's gap
/// (`position - 1`), down the gap after the lower neighbour
/// (`position + 2`); the boundary is the fake's no-op (the typed
/// refusal is the real engine's).
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
    let target = if up {
        position.checked_sub(1)
    } else {
        (position + 2 <= rows.len()).then_some(position + 2)
    };
    if let Some(target) = target {
        apply_task_reorder(state, id, target);
    }
}

/// The same-stack task reorder's fake-side resolution (B3, 2026-11-24
/// #1247 — the engine's `task_reorder_plan` mirrored verbatim): the
/// canonical `(order, id)` row source, the clamped gap, the in-place
/// no-op and the dense renumber — fake ≡ engine by construction.
fn apply_task_reorder(state: &mut AppState, id: taskboard_domain::TaskId, target: usize) {
    let Some(subject) = state.tasks.get(&id).cloned() else {
        return;
    };
    let mut rows: Vec<taskboard_domain::entities::Task> = state
        .tasks
        .values()
        .filter(|t| t.is_live() && t.stack == subject.stack)
        .cloned()
        .collect();
    rows.sort_by_key(|t| (t.order, t.id));
    let Some(position) = rows.iter().position(|t| t.id == id) else {
        return;
    };
    let target = target.min(rows.len());
    if target == position || target == position + 1 {
        return;
    }
    let moved = rows.remove(position);
    let insert_at = if target <= position {
        target
    } else {
        target - 1
    };
    rows.insert(insert_at, moved);
    for (index, mut row) in rows.into_iter().enumerate() {
        row.order = i64::try_from(index).expect("stack length fits i64");
        if let Some(stored) = state.tasks.get_mut(&row.id) {
            *stored = row;
        }
    }
}

/// The habit reorder's fake-side resolution (#1198 chunk A1 D1): the
/// same canonical row source, clamped gap, in-place no-op and dense
/// renumber the real engine plans — one step of fixture continuity.
fn apply_habit_reorder(
    state: &mut AppState,
    habit: taskboard_domain::HabitId,
    target: usize,
    ledger: &mut Vec<UndoFace>,
) {
    let Some(subject) = state.habits.habits.get(&habit).cloned() else {
        return;
    };
    let mut rows: Vec<taskboard_domain::habit::Habit> = state
        .habits
        .habits
        .values()
        .filter(|h| h.is_live() && h.board == subject.board)
        .cloned()
        .collect();
    rows.sort_by_key(|h| (h.order, h.id));
    let Some(position) = rows.iter().position(|h| h.id == habit) else {
        return;
    };
    let target = target.min(rows.len());
    if target == position || target == position + 1 {
        return;
    }
    ledger.push(UndoFace::ReorderedSections(
        rows.iter().map(|h| (h.id, h.order)).collect(),
    ));
    let moved = rows.remove(position);
    let insert_at = if target <= position {
        target
    } else {
        target - 1
    };
    rows.insert(insert_at, moved);
    let renumbered: Vec<(taskboard_domain::HabitId, i64)> = rows
        .into_iter()
        .enumerate()
        .map(|(index, mut row)| {
            row.order = i64::try_from(index).expect("section length fits i64");
            (row.id, row.order)
        })
        .collect();
    for (id, order) in &renumbered {
        if let Some(row) = state.habits.habits.get_mut(id) {
            row.order = *order;
            row.clocks.order = chrono::Utc::now();
        }
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
                ledger: Vec::new(),
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
                // The same-stack reorder's continuity half (B3,
                // 2026-11-24 #1247): the engine's `task_reorder_plan`
                // mirrored — the canonical row source, the clamped
                // gap, the dense renumber.
                StateCommand::TaskReorder { id, target } => {
                    apply_task_reorder(&mut state, id, target);
                }
                // The habit reorder's continuity half (#1198 chunk
                // A1 D1): the engine's resolution mirrored (the
                // canonical row source, the clamped gap, the dense
                // renumber) and the pre-move section kept for the
                // `Undo` face.
                StateCommand::HabitReorder { habit, target } => {
                    apply_habit_reorder(&mut state, habit, target, &mut self.ledger);
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
                        self.ledger.push(UndoFace::RemovedRule(row.clone()));
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
                    self.ledger.push(UndoFace::AddedRule(id));
                    state.reminders.rules.insert(id, row);
                }
                // The UF1 undo-line continuity (fixture continuity,
                // never an oracle — the state engine's fold suite owns
                // undo semantics): a rule removal restores its row, an
                // add deletes the minted rule; neither touches the
                // delivery records.
                StateCommand::Undo => {
                    // The ledger's pop (the #1219 model — one level
                    // per Undo, the newest face's inverse only;
                    // fixture continuity, never an oracle — the
                    // state engine's fold suite owns undo
                    // semantics).
                    match self.ledger.pop() {
                        // The clear-all's undo face (W-7): one undo
                        // step restores the whole cleared batch
                        // verbatim.
                        Some(UndoFace::ClearedBatch(batch)) => {
                            for rule in batch {
                                state.reminders.rules.insert(rule.id, rule);
                            }
                        }
                        // The restore journeys' undo face (W-2/W-3):
                        // a restored row re-tombstones (the #1187
                        // law).
                        Some(UndoFace::RestoredEntity(RestoredEntity::Label(id))) => {
                            if let Some(label) = state.labels.get_mut(&id) {
                                label.deleted = true;
                            }
                        }
                        Some(UndoFace::RestoredEntity(RestoredEntity::Habit(id))) => {
                            if let Some(habit) = state.habits.habits.get_mut(&id) {
                                habit.deleted = true;
                            }
                        }
                        Some(UndoFace::RemovedRule(row)) => {
                            state.reminders.rules.entry(row.id).or_insert(row);
                        }
                        Some(UndoFace::AddedRule(added)) => {
                            state.reminders.rules.remove(&added);
                        }
                        Some(UndoFace::ReorderedSections(section)) => {
                            for (id, order) in section {
                                if let Some(row) = state.habits.habits.get_mut(&id) {
                                    row.order = order;
                                }
                            }
                        }
                        None => {}
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
                // The restore verbs' continuity (the undo-family
                // journeys, W-2/W-3): the tombstone flips live — the
                // roster's re-derivation rides the next refresh; the
                // row is kept so the `Undo` face can re-tombstone.
                // The clear-all's continuity (the undo-family
                // journeys, W-4): the settle clears the subject's
                // rules — the presented roster's next read sees the
                // empty list (fixture continuity, not an oracle).
                StateCommand::ClearReminderRules { task } => {
                    let cleared: Vec<_> = state
                        .reminders
                        .rules
                        .values()
                        .filter(|rule| rule.task == task)
                        .cloned()
                        .collect();
                    for rule in &cleared {
                        state.reminders.rules.remove(&rule.id);
                    }
                    self.ledger.push(UndoFace::ClearedBatch(cleared));
                }
                StateCommand::RestoreLabel { id } => {
                    if let Some(label) = state.labels.get_mut(&id) {
                        label.deleted = false;
                        self.ledger
                            .push(UndoFace::RestoredEntity(RestoredEntity::Label(id)));
                    }
                }
                StateCommand::RestoreHabit { id } => {
                    if let Some(habit) = state.habits.habits.get_mut(&id) {
                        habit.deleted = false;
                        self.ledger
                            .push(UndoFace::RestoredEntity(RestoredEntity::Habit(id)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::driver::DriverOptions;
    use crate::support::driver::JourneyDriver as _;
    use crate::support::fixture;

    use taskboard_ui_slint::UiSettings;

    /// The ledger self-tests' mount (the driver self-tests' shape,
    /// once): the fake's channel over the shared `ArcSwap`, the UI
    /// mounted on the journey driver so `settle`'s refresh lands. The
    /// assertions address the fake's state snapshot directly — the
    /// ledger's continuity, not the engine's undo semantics.
    fn settle_journey<R>(
        state: AppState,
        job: impl FnOnce(&mut FakeEngine, &tokio::sync::mpsc::Sender<StateCommand>, &UiHandle) -> R
        + Send
        + 'static,
    ) -> R
    where
        R: Send + 'static,
    {
        crate::support::driver::run(
            DriverOptions {
                headless_renderer: crate::HEADLESS_RENDERER,
            },
            move |driver| {
                let state = Arc::new(ArcSwap::from_pointee(state));
                let (tx, mut engine) = FakeEngine::channel(Arc::clone(&state));
                let (requests_tx, _requests_rx) = mpsc::channel(16);
                let (_system_tx, system_rx) = tokio::sync::broadcast::channel(4);
                let (_signals_tx, signals_rx) = tokio::sync::broadcast::channel(4);
                let (_htx, health) = tokio::sync::watch::channel(
                    taskboard_ui_slint::presentation::DeliveryHealth::default(),
                );
                let handle = taskboard_ui_slint::mount(
                    state,
                    signals_rx,
                    system_rx,
                    tx.clone(),
                    requests_tx,
                    UiSettings::default(),
                    health,
                    None,
                )
                .expect("mount succeeds on the testing backend");
                driver.bind(&handle);
                driver.advance_time(0);
                job(&mut engine, &tx, &handle)
            },
        )
    }

    /// Sends `command` over the fake's boundary and applies it (the
    /// settle is the fake's application step).
    fn send_and_settle(
        tx: &tokio::sync::mpsc::Sender<StateCommand>,
        engine: &mut FakeEngine,
        ui: &UiHandle,
        command: StateCommand,
    ) {
        tx.blocking_send(command).expect("the boundary is open");
        engine.settle(ui);
    }

    /// The rules roster's typed ids (the ledger assertions' oracle
    /// input — membership, never rendering).
    fn rule_ids(state: &AppState) -> std::collections::BTreeSet<taskboard_domain::ReminderRuleId> {
        state.reminders.rules.keys().copied().collect()
    }

    /// The #1219 regression pin: `Undo` pops exactly one face per
    /// command. Remove → add → undo deletes only the added rule; the
    /// second undo restores the removed row. The old multi-family pop
    /// reverted both operations in one pass, so this interleaved
    /// sequence is the discriminating shape.
    #[test]
    fn undo_pops_one_face_per_command_across_families() {
        let (fx, mut state) = fixture::journey_state();
        let removed = fixture::add_armed_rule(&mut state, fx.task0, 0xA10);
        settle_journey(state, move |engine, tx, ui| {
            let baseline = rule_ids(&engine.snapshot());
            send_and_settle(
                tx,
                engine,
                ui,
                StateCommand::RemoveReminderRule { rule: removed },
            );
            send_and_settle(
                tx,
                engine,
                ui,
                StateCommand::AddReminderRule {
                    task: fx.task0,
                    offset: taskboard_domain::ReminderOffset::at_due(),
                    at: None,
                    insistence: taskboard_domain::Insistence::Normal,
                },
            );
            let after_ops = rule_ids(&engine.snapshot());
            assert!(
                !after_ops.contains(&removed),
                "the removal removed the seeded rule"
            );
            let mut added: Vec<_> = after_ops.difference(&baseline).copied().collect();
            assert_eq!(added.len(), 1, "the add minted exactly one rule");
            let added = added.pop().expect("one minted rule");

            send_and_settle(tx, engine, ui, StateCommand::Undo);
            let after_first_undo = rule_ids(&engine.snapshot());
            assert!(
                !after_first_undo.contains(&added),
                "the first undo deletes only the added rule"
            );
            assert!(
                !after_first_undo.contains(&removed),
                "the first undo leaves the removal in place"
            );

            send_and_settle(tx, engine, ui, StateCommand::Undo);
            assert_eq!(
                rule_ids(&engine.snapshot()),
                baseline,
                "the second undo restores the removed row verbatim"
            );
        });
    }

    /// An `Undo` past the emptied ledger is a typed no-op — the state
    /// snapshot is bit-identical (nothing restored, nothing removed).
    #[test]
    fn undo_past_the_empty_ledger_is_a_noop() {
        let (fx, mut state) = fixture::journey_state();
        let seeded = fixture::add_armed_rule(&mut state, fx.task0, 0xA20);
        settle_journey(state, move |engine, tx, ui| {
            send_and_settle(
                tx,
                engine,
                ui,
                StateCommand::RemoveReminderRule { rule: seeded },
            );
            send_and_settle(tx, engine, ui, StateCommand::Undo);
            let restored = rule_ids(&engine.snapshot());
            assert!(restored.contains(&seeded), "the one undo restored");

            send_and_settle(tx, engine, ui, StateCommand::Undo);
            send_and_settle(tx, engine, ui, StateCommand::Undo);
            assert_eq!(
                rule_ids(&engine.snapshot()),
                restored,
                "the ledger-empty undos changed nothing"
            );
        });
    }

    /// The W-7 face: one `Undo` after `ClearReminderRules` restores
    /// the whole cleared batch — all n rows, as a single popped face.
    #[test]
    fn undo_after_a_clear_restores_the_whole_batch() {
        let (fx, mut state) = fixture::journey_state();
        let first = fixture::add_armed_rule(&mut state, fx.task0, 0xA30);
        let second = fixture::add_armed_rule(&mut state, fx.task0, 0xA31);
        settle_journey(state, move |engine, tx, ui| {
            send_and_settle(
                tx,
                engine,
                ui,
                StateCommand::ClearReminderRules { task: fx.task0 },
            );
            let after_clear = rule_ids(&engine.snapshot());
            assert!(!after_clear.contains(&first) && !after_clear.contains(&second));

            send_and_settle(tx, engine, ui, StateCommand::Undo);
            let restored = rule_ids(&engine.snapshot());
            assert!(
                restored.contains(&first) && restored.contains(&second),
                "the single undo restored both cleared rows"
            );
        });
    }
}
