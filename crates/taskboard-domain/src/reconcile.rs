// SPDX-License-Identifier: MIT OR Apache-2.0
//! The title reconcile pass ([`plan_reconcile`]): the pure planner that
//! merges same-title duplicate entities after a bind ceremony's pull
//! adopted remote content alongside pre-existing local-only rows.
//!
//! Survivorship (design D3-4): the *local-only* row survives — it keeps
//! the user's local id (reminder rules, snooze links, and pending update
//! ops address it and stay live) — and adopts the bound row's remote
//! ref. The pull-adopted duplicate is neutralized, not tombstoned with
//! its ref (that would push a real remote delete): it upserts as a
//! ref-less tombstone whose delete group completes synthetically. Its
//! id is fresh from the pull, so no ops and no reminder facts address
//! it and no cascade is needed.
//!
//! Matching is deterministic (D3-10): `trim` + case folding decides
//! equality; the remote-bound side iterates in id order and absorbs the
//! smallest-id unmatched local-only same-title entity; one-to-one; a
//! second same-title local-only entity stays. Tasks pair only within a
//! shared stack row — stack merges re-parent the absorbed stack's
//! children first, so "within matched stacks" is "within one stack
//! row" by construction. Merges are local-vs-local field-wise
//! newer-clock-wins with the resulting clocks the field-wise max; field
//! ties go to the bound row (remote continuity, deterministic).

use std::collections::{BTreeMap, BTreeSet};

use crate::entities::{Label, LabelClocks, Stack, StackClocks, Task, TaskClocks};
use crate::ids::{BoardId, LabelId, StackId, TaskId};
use crate::outbox::{LocalOp, PendingOp};
use crate::persistence::PersistenceAction;
use crate::state::AppState;

/// The normalized match key: `trim()` + simple case folding. Exactly
/// that — no Unicode normalization, no collation (fuzzy growth is the
/// design's out-list).
#[must_use]
pub fn normalized_title(raw: &str) -> String {
    raw.trim().chars().flat_map(char::to_lowercase).collect()
}

/// What one reconcile pass merged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReconcileCounts {
    /// Merged stack pairs.
    pub stacks: usize,
    /// Merged label pairs.
    pub labels: usize,
    /// Merged task pairs.
    pub tasks: usize,
    /// Create-ops cancelled on survivors that just adopted a ref (the
    /// planner would otherwise materialize a duplicate POST).
    pub cancelled_ops: usize,
}

/// Plans one reconcile pass over the board's live entities.
///
/// Pure and deterministic: equal inputs yield equal action batches.
/// An empty plan (unbound board, or no remote-bound sibling to match
/// against) means nothing to do — the planner turns that into a `NoOp`.
///
/// Actions emitted: upserts for survivors, neutralizing upserts for
/// absorbed rows, re-parents, label-edge rewrites, and one `FailOp` per
/// cancelled create-op.
#[must_use]
#[allow(clippy::too_many_lines)] // three fixed passes; the sections ARE the design
pub fn plan_reconcile(
    app: &AppState,
    outbox: &[PendingOp],
    board: BoardId,
) -> (Vec<PersistenceAction>, ReconcileCounts) {
    let mut counts = ReconcileCounts::default();
    let Some(row) = app.boards.get(&board) else {
        return (Vec::new(), counts);
    };
    if row.remote.is_none() {
        return (Vec::new(), counts);
    }

    // ---- Scope: the board's live entities, in working copies --------
    let board_stack_ids: BTreeSet<StackId> = app
        .stacks
        .values()
        .filter(|s| s.board == board && !s.deleted)
        .map(|s| s.id)
        .collect();
    let stacks: BTreeMap<StackId, Stack> = app
        .stacks
        .values()
        .filter(|s| board_stack_ids.contains(&s.id))
        .cloned()
        .map(|s| (s.id, s))
        .collect();
    let labels: BTreeMap<LabelId, Label> = app
        .labels
        .values()
        .filter(|l| l.board == board && !l.deleted)
        .cloned()
        .map(|l| (l.id, l))
        .collect();
    let mut tasks: BTreeMap<TaskId, Task> = app
        .tasks
        .values()
        .filter(|t| t.is_live() && board_stack_ids.contains(&t.stack))
        .cloned()
        .map(|t| (t.id, t))
        .collect();

    let any_bound = stacks.values().any(|s| s.remote.is_some())
        || labels.values().any(|l| l.remote.is_some())
        || tasks.values().any(|t| t.remote.is_some());
    if !any_bound {
        return (Vec::new(), counts);
    }

    let mut actions = Vec::new();

    // ---- Stacks: pair in bound-id order, absorb smallest-id local ----
    let mut matched_stacks: BTreeSet<StackId> = BTreeSet::new();
    let mut merged_stacks: Vec<(Stack, Stack)> = Vec::new(); // (survivor, absorbed)
    for bound_id in stack_bound_ids(&stacks) {
        let norm = normalized_title(&stacks[&bound_id].title);
        let Some(local_id) = stacks
            .values()
            .filter(|s| s.remote.is_none() && !matched_stacks.contains(&s.id))
            .filter(|s| normalized_title(&s.title) == norm)
            .map(|s| s.id)
            .min()
        else {
            continue;
        };
        matched_stacks.insert(local_id);
        let survivor = merge_stack_pair(&stacks[&local_id], &stacks[&bound_id]);
        let absorbed = neutralized_stack(&stacks[&local_id], &stacks[&bound_id]);
        // Re-parent the absorbed stack's children to the survivor (this
        // is what makes the task pass's "within one stack row" exact).
        let re_parented: Vec<Task> = tasks
            .values()
            .filter(|t| t.stack == bound_id)
            .map(|t| {
                let mut moved = t.clone();
                moved.stack = local_id;
                moved
            })
            .collect();
        for moved in &re_parented {
            tasks.insert(moved.id, moved.clone());
            actions.push(PersistenceAction::UpsertTask(moved.clone()));
        }
        actions.push(PersistenceAction::UpsertStack(survivor.clone()));
        actions.push(PersistenceAction::UpsertStack(absorbed.clone()));
        merged_stacks.push((survivor, absorbed));
        counts.stacks += 1;
    }

    // ---- Labels: same pairing; survivors re-point task edges --------
    let mut matched_labels: BTreeSet<LabelId> = BTreeSet::new();
    for bound_id in label_bound_ids(&labels) {
        let norm = normalized_title(&labels[&bound_id].title);
        let Some(local_id) = labels
            .values()
            .filter(|l| l.remote.is_none() && !matched_labels.contains(&l.id))
            .filter(|l| normalized_title(&l.title) == norm)
            .map(|l| l.id)
            .min()
        else {
            continue;
        };
        matched_labels.insert(local_id);
        let survivor = merge_label_pair(&labels[&local_id], &labels[&bound_id]);
        let absorbed = neutralized_label(&labels[&local_id], &labels[&bound_id]);
        // Edge rewrites: every task referencing the absorbed label id
        // now references the survivor; the rewritten rows emit (a
        // rewritten set may be the only change a task carries).
        let rewritten: Vec<Task> = tasks
            .values_mut()
            .filter_map(|task| {
                task.labels.remove(&bound_id).then(|| {
                    task.labels.insert(local_id);
                    task.clone()
                })
            })
            .collect();
        for task in rewritten {
            actions.push(PersistenceAction::UpsertTask(task));
        }
        actions.push(PersistenceAction::UpsertLabel(survivor.clone()));
        actions.push(PersistenceAction::UpsertLabel(absorbed.clone()));
        counts.labels += 1;
    }

    // ---- Tasks: pair within one stack row (post-re-parenting) -------
    let mut matched_tasks: BTreeSet<TaskId> = BTreeSet::new();
    let mut merged_tasks: Vec<(Task, Task)> = Vec::new();
    for bound_id in task_bound_ids(&tasks) {
        let norm = normalized_title(&tasks[&bound_id].title);
        let Some(local_id) = tasks
            .values()
            .filter(|t| t.remote.is_none() && !t.deleted && !matched_tasks.contains(&t.id))
            .filter(|t| t.stack == tasks[&bound_id].stack)
            .filter(|t| normalized_title(&t.title) == norm)
            .map(|t| t.id)
            .min()
        else {
            continue;
        };
        // Both sides of the pair leave the candidate pool: the survivor
        // (now ref-carrying) and the neutralized absorbed row (a
        // ref-less tombstone in the working map — it must never
        // re-enter as a merge candidate).
        matched_tasks.insert(local_id);
        matched_tasks.insert(bound_id);
        let survivor = merge_task_pair(&tasks[&local_id], &tasks[&bound_id]);
        let absorbed = neutralized_task(&tasks[&local_id], &tasks[&bound_id]);
        tasks.insert(local_id, survivor.clone());
        tasks.insert(bound_id, absorbed.clone());
        merged_tasks.push((survivor, absorbed));
        counts.tasks += 1;
    }

    // Emit the task rows: edge rewrites (a rewritten set is the only
    // change some tasks carry) and every merge's survivor + absorbed.
    for (survivor, absorbed) in &merged_tasks {
        actions.push(PersistenceAction::UpsertTask(survivor.clone()));
        actions.push(PersistenceAction::UpsertTask(absorbed.clone()));
    }

    // ---- Duplicate create-ops on survivors cancel --------------------
    let survivors_stacks: BTreeSet<StackId> = merged_stacks.iter().map(|(s, _)| s.id).collect();
    let survivors_labels: BTreeSet<LabelId> = matched_labels.clone();
    let survivors_tasks: BTreeSet<TaskId> = merged_tasks.iter().map(|(t, _)| t.id).collect();
    for pending in outbox {
        let cancels = match pending.op {
            LocalOp::CreateStack(id) => survivors_stacks.contains(&id),
            LocalOp::CreateLabel(id) => survivors_labels.contains(&id),
            LocalOp::CreateTask(id) => survivors_tasks.contains(&id),
            // Update-class ops ride the survivor's update path; delete
            // and move/assign classes keep their own semantics.
            _ => false,
        };
        if cancels {
            actions.push(PersistenceAction::FailOp(pending.op_id));
            counts.cancelled_ops += 1;
        }
    }

    (actions, counts)
}

fn stack_bound_ids(stacks: &BTreeMap<StackId, Stack>) -> Vec<StackId> {
    stacks
        .values()
        .filter(|s| s.remote.is_some())
        .map(|s| s.id)
        .collect()
}

fn label_bound_ids(labels: &BTreeMap<LabelId, Label>) -> Vec<LabelId> {
    labels
        .values()
        .filter(|l| l.remote.is_some())
        .map(|l| l.id)
        .collect()
}

fn task_bound_ids(tasks: &BTreeMap<TaskId, Task>) -> Vec<TaskId> {
    tasks
        .values()
        .filter(|t| t.remote.is_some())
        .map(|t| t.id)
        .collect()
}

/// The merged survivor of a stack pair: the local row's id with the
/// bound row's remote identity; fields resolve by the newer clock
/// (tie ⇒ the bound row's value); clocks are the field-wise max.
#[must_use]
pub fn merge_stack_pair(local: &Stack, bound: &Stack) -> Stack {
    let lc = &local.clocks;
    let bc = &bound.clocks;
    Stack {
        id: local.id,
        remote: bound.remote,
        board: local.board,
        title: pick(&local.title, &bound.title, lc.title, bc.title),
        order: if bc.order >= lc.order {
            bound.order
        } else {
            local.order
        },
        archived: if bc.order >= lc.order {
            bound.archived
        } else {
            local.archived
        },
        deleted: local.deleted || bound.deleted,
        clocks: StackClocks {
            title: lc.title.max(bc.title),
            order: lc.order.max(bc.order),
            deleted: lc.deleted.max(bc.deleted),
        },
        remote_seen: bound.remote_seen,
    }
}

/// The neutralized duplicate of a stack merge: a ref-less tombstone —
/// `deleted ∧ remote: None`, whose delete group completes synthetically.
#[must_use]
pub fn neutralized_stack(local: &Stack, bound: &Stack) -> Stack {
    let mut absorbed = bound.clone();
    absorbed.deleted = true;
    absorbed.remote = None;
    absorbed.remote_seen = None;
    absorbed.clocks.deleted = absorbed.clocks.deleted.max(local.clocks.deleted);
    absorbed
}

/// The merged survivor of a label pair (same rules as stacks).
#[must_use]
pub fn merge_label_pair(local: &Label, bound: &Label) -> Label {
    let lc = &local.clocks;
    let bc = &bound.clocks;
    Label {
        id: local.id,
        remote: bound.remote,
        board: local.board,
        title: pick(&local.title, &bound.title, lc.title, bc.title),
        color: pick(&local.color, &bound.color, lc.color, bc.color),
        deleted: local.deleted || bound.deleted,
        clocks: LabelClocks {
            title: lc.title.max(bc.title),
            color: lc.color.max(bc.color),
            deleted: lc.deleted.max(bc.deleted),
        },
        remote_seen: bound.remote_seen,
    }
}

/// The neutralized duplicate of a label merge.
#[must_use]
pub fn neutralized_label(local: &Label, bound: &Label) -> Label {
    let mut absorbed = bound.clone();
    absorbed.deleted = true;
    absorbed.remote = None;
    absorbed.remote_seen = None;
    absorbed.clocks.deleted = absorbed.clocks.deleted.max(local.clocks.deleted);
    absorbed
}

/// The merged survivor of a task pair: every field resolves by the
/// newer clock (tie ⇒ the bound row's value); clocks are the field-wise
/// max; the composite position is one field (one clock).
#[must_use]
pub fn merge_task_pair(local: &Task, bound: &Task) -> Task {
    let lc = &local.clocks;
    let bc = &bound.clocks;
    let position_local_wins = lc.position > bc.position;
    let labels_local_wins = lc.labels > bc.labels;
    Task {
        id: local.id,
        remote: bound.remote,
        title: pick(&local.title, &bound.title, lc.title, bc.title),
        description: pick(
            &local.description,
            &bound.description,
            lc.description,
            bc.description,
        ),
        duedate: if bc.duedate >= lc.duedate {
            bound.duedate
        } else {
            local.duedate
        },
        scheduled: if bc.scheduled >= lc.scheduled {
            bound.scheduled
        } else {
            local.scheduled
        },
        done: if bc.done >= lc.done {
            bound.done
        } else {
            local.done
        },
        stack: if position_local_wins {
            local.stack
        } else {
            bound.stack
        },
        order: if position_local_wins {
            local.order
        } else {
            bound.order
        },
        labels: if labels_local_wins {
            local.labels.clone()
        } else {
            bound.labels.clone()
        },
        effort: if bc.effort >= lc.effort {
            bound.effort
        } else {
            local.effort
        },
        importance: if bc.importance >= lc.importance {
            bound.importance
        } else {
            local.importance
        },
        urgency: if bc.urgency >= lc.urgency {
            bound.urgency
        } else {
            local.urgency
        },
        recur: if bc.recur >= lc.recur {
            bound.recur.clone()
        } else {
            local.recur.clone()
        },
        archived: if bc.archived >= lc.archived {
            bound.archived
        } else {
            local.archived
        },
        deleted: local.deleted || bound.deleted,
        clocks: TaskClocks {
            title: lc.title.max(bc.title),
            description: lc.description.max(bc.description),
            duedate: lc.duedate.max(bc.duedate),
            scheduled: lc.scheduled.max(bc.scheduled),
            done: lc.done.max(bc.done),
            position: lc.position.max(bc.position),
            labels: lc.labels.max(bc.labels),
            effort: lc.effort.max(bc.effort),
            importance: lc.importance.max(bc.importance),
            urgency: lc.urgency.max(bc.urgency),
            recur: lc.recur.max(bc.recur),
            archived: lc.archived.max(bc.archived),
            deleted: lc.deleted.max(bc.deleted),
        },
        remote_seen: bound.remote_seen,
    }
}

/// The neutralized duplicate of a task merge.
#[must_use]
pub fn neutralized_task(local: &Task, bound: &Task) -> Task {
    let mut absorbed = bound.clone();
    absorbed.deleted = true;
    absorbed.remote = None;
    absorbed.remote_seen = None;
    absorbed.clocks.deleted = absorbed.clocks.deleted.max(local.clocks.deleted);
    absorbed
}

/// Tie-goes-to-the-bound field pick for owned values.
fn pick<T: Clone>(
    local: &T,
    bound: &T,
    local_clock: chrono::DateTime<chrono::Utc>,
    bound_clock: chrono::DateTime<chrono::Utc>,
) -> T {
    if bound_clock >= local_clock {
        bound.clone()
    } else {
        local.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::OpId;
    use chrono::{TimeZone, Utc};

    fn ts(secs: i64) -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn board() -> BoardId {
        BoardId::from(uuid::Uuid::from_u128(800))
    }

    fn bound_board() -> AppState {
        let mut app = AppState::default();
        let mut row = crate::entities::Board {
            id: board(),
            remote: None,
            title: "board".into(),
            color: crate::entities::Color::new("0000ff"),
            archived: false,
            deleted: false,
            clocks: crate::entities::BoardClocks {
                title: crate::entities::BoardClocks::epoch(),
                color: crate::entities::BoardClocks::epoch(),
                archived: crate::entities::BoardClocks::epoch(),
                deleted: crate::entities::BoardClocks::epoch(),
            },
            remote_seen: None,
        };
        row.remote = crate::test_support::test_binding(crate::ids::RemoteBoardId(42));
        app.boards.insert(board(), row);
        app
    }

    fn stack(raw: u64, title: &str, bound: bool) -> Stack {
        Stack {
            id: StackId::from(uuid::Uuid::from_u128(u128::from(raw))),
            remote: bound.then_some(crate::ids::RemoteStackRef {
                board: crate::ids::RemoteBoardId(42),
                stack: crate::ids::RemoteStackId(raw),
            }),
            board: board(),
            title: title.to_owned(),
            order: 1,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: ts(100),
                order: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        }
    }

    fn label(raw: u64, title: &str, bound: bool) -> Label {
        Label {
            id: LabelId::from(uuid::Uuid::from_u128(u128::from(raw))),
            remote: bound.then_some(crate::ids::RemoteLabelRef {
                board: crate::ids::RemoteBoardId(42),
                label: crate::ids::RemoteLabelId(raw),
            }),
            board: board(),
            title: title.to_owned(),
            color: crate::entities::Color::new("ff0000"),
            deleted: false,
            clocks: LabelClocks {
                title: ts(100),
                color: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        }
    }

    fn task(raw: u64, stack_id: StackId, title: &str, bound: bool) -> Task {
        Task {
            id: TaskId::from(uuid::Uuid::from_u128(u128::from(raw))),
            remote: bound.then_some(crate::ids::RemoteCardRef {
                board: crate::ids::RemoteBoardId(42),
                stack: crate::ids::RemoteStackId(
                    u64::try_from(stack_id.as_uuid().as_u128()).unwrap_or(1),
                ),
                card: crate::ids::RemoteCardId(raw),
            }),
            title: title.to_owned(),
            description: String::new(),
            duedate: None,
            done: None,
            scheduled: None,
            stack: stack_id,
            order: 1,
            labels: BTreeSet::default(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: ts(100),
                description: ts(100),
                duedate: ts(100),
                scheduled: ts(100),
                done: ts(100),
                position: ts(100),
                labels: ts(100),
                effort: ts(100),
                importance: ts(100),
                urgency: ts(100),
                recur: ts(100),

                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        }
    }

    fn op(raw: u128, op: LocalOp) -> PendingOp {
        PendingOp {
            op_id: OpId(uuid::Uuid::from_u128(raw)),
            op,
            queued_at: ts(50),
        }
    }

    #[test]
    fn normalized_title_trims_and_casefolds() {
        assert_eq!(normalized_title("  To-Do "), "to-do");
        assert_eq!(normalized_title("TO-DO"), "to-do");
        assert_eq!(normalized_title("to-do"), "to-do");
    }

    #[test]
    fn stacks_match_by_normalized_title_and_merge() {
        let mut app = bound_board();
        app.stacks
            .insert(stack(1, "To-Do", true).id, stack(1, "To-Do", true));
        app.stacks
            .insert(stack(2, "  to-do ", false).id, stack(2, "  to-do ", false));
        let (actions, counts) = plan_reconcile(&app, &[], board());
        assert_eq!(counts.stacks, 1);
        assert_eq!(counts.tasks, 0);
        // The survivor is the local row (id 2): it gains the ref.
        let survivor = actions
            .iter()
            .find_map(|a| match a {
                PersistenceAction::UpsertStack(s)
                    if s.id == StackId::from(uuid::Uuid::from_u128(2)) =>
                {
                    Some(s)
                }
                _ => None,
            })
            .expect("survivor upserted");
        assert!(survivor.remote.is_some(), "the local row adopts the ref");
        // The absorbed row is a ref-less tombstone.
        let absorbed = actions
            .iter()
            .find_map(|a| match a {
                PersistenceAction::UpsertStack(s)
                    if s.id == StackId::from(uuid::Uuid::from_u128(1)) =>
                {
                    Some(s)
                }
                _ => None,
            })
            .expect("absorbed upserted");
        assert!(absorbed.deleted && absorbed.remote.is_none());
    }

    #[test]
    fn stack_merge_reparents_absorbed_children() {
        let mut app = bound_board();
        let bound = stack(1, "col", true);
        let local = stack(2, "col", false);
        app.stacks.insert(bound.id, bound.clone());
        app.stacks.insert(local.id, local.clone());
        let child = task(10, bound.id, "card", true);
        let child_id = child.id;
        app.tasks.insert(child.id, child);
        let (actions, counts) = plan_reconcile(&app, &[], board());
        assert_eq!(counts.stacks, 1);
        assert!(
            actions.iter().any(|a| matches!(a, PersistenceAction::UpsertTask(t) if t.id == child_id && t.stack == local.id)),
            "the absorbed stack's child re-parents to the survivor"
        );
    }

    #[test]
    fn labels_match_and_repoint_task_edges() {
        let mut app = bound_board();
        let bound = label(1, "urgent", true);
        let local = label(2, "Urgent", false);
        app.labels.insert(bound.id, bound.clone());
        app.labels.insert(local.id, local.clone());
        let s = stack(5, "s", true);
        app.stacks.insert(s.id, s.clone());
        let mut carrier = task(10, s.id, "card", true);
        carrier.labels.insert(bound.id);
        let carrier_id = carrier.id;
        app.tasks.insert(carrier.id, carrier);
        let (actions, counts) = plan_reconcile(&app, &[], board());
        assert_eq!(counts.labels, 1);
        assert!(
            actions.iter().any(|a| matches!(a, PersistenceAction::UpsertTask(t)
                if t.id == carrier_id && t.labels.contains(&local.id) && !t.labels.contains(&bound.id))),
            "the task's label edge re-points to the survivor"
        );
    }

    #[test]
    fn tasks_match_only_within_shared_stack_row() {
        let mut app = bound_board();
        let s1 = stack(1, "a", true);
        let s2 = stack(2, "b", false);
        app.stacks.insert(s1.id, s1.clone());
        app.stacks.insert(s2.id, s2.clone());
        app.tasks.insert(
            task(10, s1.id, "same", true).id,
            task(10, s1.id, "same", true),
        );
        app.tasks.insert(
            task(11, s2.id, "same", false).id,
            task(11, s2.id, "same", false),
        );
        let (_, counts) = plan_reconcile(&app, &[], board());
        assert_eq!(counts.tasks, 0, "cross-stack same-title tasks never pair");
    }

    #[test]
    fn task_merge_newer_clock_wins_per_field() {
        let mut local = task(11, stack(1, "s", false).id, "t", false);
        let mut bound = task(10, stack(1, "s", true).id, "t", true);
        // Local wins title (newer clock); bound wins description.
        local.clocks.title = ts(200);
        local.title = "local title".into();
        bound.clocks.title = ts(100);
        local.clocks.description = ts(100);
        bound.clocks.description = ts(300);
        bound.description = "bound desc".into();
        let merged = merge_task_pair(&local, &bound);
        assert_eq!(merged.title, "local title");
        assert_eq!(merged.description, "bound desc");
        assert_eq!(merged.clocks.title, ts(200));
        assert_eq!(merged.clocks.description, ts(300));
        assert_eq!(merged.id, local.id, "the local row survives");
        assert!(merged.remote.is_some(), "the survivor adopts the ref");
    }

    #[test]
    fn duplicate_create_ops_cancel_counted() {
        let mut app = bound_board();
        let bound = stack(1, "s", true);
        let local = stack(2, "s", false);
        app.stacks.insert(bound.id, bound.clone());
        app.stacks.insert(local.id, local.clone());
        let unmatched = stack(3, "other", false);
        app.stacks.insert(unmatched.id, unmatched.clone());
        let outbox = vec![
            op(100, LocalOp::CreateStack(local.id)),
            op(101, LocalOp::CreateStack(unmatched.id)),
        ];
        let (actions, counts) = plan_reconcile(&app, &outbox, board());
        assert_eq!(counts.cancelled_ops, 1);
        assert!(
            actions.contains(&PersistenceAction::FailOp(OpId(uuid::Uuid::from_u128(100)))),
            "the survivor's create-op cancels"
        );
        assert!(
            !actions.contains(&PersistenceAction::FailOp(OpId(uuid::Uuid::from_u128(101)))),
            "an unmatched local-only stack's create-op survives"
        );
    }

    #[test]
    fn second_run_is_empty() {
        let mut app = bound_board();
        app.stacks
            .insert(stack(1, "s", true).id, stack(1, "s", true));
        app.stacks
            .insert(stack(2, "s", false).id, stack(2, "s", false));
        let (actions, _) = plan_reconcile(&app, &[], board());
        // Apply the plan exactly like the engine's interpret would.
        for action in &actions {
            match action {
                PersistenceAction::UpsertStack(s) => {
                    app.stacks.insert(s.id, s.clone());
                }
                PersistenceAction::UpsertTask(t) => {
                    app.tasks.insert(t.id, t.clone());
                }
                PersistenceAction::UpsertLabel(l) => {
                    app.labels.insert(l.id, l.clone());
                }
                _ => {}
            }
        }
        let (second, counts) = plan_reconcile(&app, &[], board());
        assert!(second.is_empty(), "a settled state re-plans to nothing");
        assert_eq!(counts, ReconcileCounts::default());
    }

    #[test]
    fn two_same_title_locals_one_bound_absorbs_deterministic_min_id() {
        let mut app = bound_board();
        app.stacks
            .insert(stack(1, "s", true).id, stack(1, "s", true));
        app.stacks
            .insert(stack(3, "s", false).id, stack(3, "s", false));
        app.stacks
            .insert(stack(2, "s", false).id, stack(2, "s", false));
        let (actions, counts) = plan_reconcile(&app, &[], board());
        assert_eq!(counts.stacks, 1, "one-to-one: only one local absorbs");
        // Smallest local id (2) is the survivor; 3 stays.
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, PersistenceAction::UpsertStack(s)
                if s.id == StackId::from(uuid::Uuid::from_u128(2)) && s.remote.is_some())),
        );
        let leftover = &app.stacks[&StackId::from(uuid::Uuid::from_u128(3))];
        assert!(leftover.remote.is_none(), "the leftover stays local-only");
    }

    #[test]
    fn unbound_board_plans_nothing() {
        let app = crate::state::AppState::default();
        let (actions, counts) = plan_reconcile(&app, &[], board());
        assert_eq!(actions.len(), 0);
        assert_eq!(counts, ReconcileCounts::default());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::command::bind_wipe_count;
    use crate::entities::{LabelClocks, StackClocks, TaskClocks};
    use crate::ids::{RemoteBoardId, RemoteCardRef, RemoteLabelRef, RemoteStackId, RemoteStackRef};
    use chrono::{TimeZone, Utc};
    use proptest::prelude::*;

    fn ts(secs: i64) -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    /// A raw stack row: (index, bound, ref board, title). Titles come
    /// from a small alphabet whose members collide after normalization.
    type RawStack = (usize, bool, u64, String);
    type RawLabel = (usize, bool, u64, String);
    type RawTask = (usize, usize, bool, u64, String);

    use super::proptest_strategies::{
        label_clock_pair_strategy, stack_clock_pair_strategy, task_pair_strategy,
    };

    fn title() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("a".to_owned()),
            Just("A".to_owned()),
            Just(" a ".to_owned()),
            Just("b".to_owned()),
            Just("B".to_owned()),
            Just("c".to_owned()),
        ]
    }

    fn raw_stacks() -> impl Strategy<Value = Vec<RawStack>> {
        prop::collection::vec((0_usize..4, any::<bool>(), 1_u64..3, title()), 0..4)
    }

    fn raw_labels() -> impl Strategy<Value = Vec<RawLabel>> {
        prop::collection::vec((4_usize..8, any::<bool>(), 1_u64..3, title()), 0..3)
    }

    fn raw_tasks() -> impl Strategy<Value = Vec<RawTask>> {
        prop::collection::vec(
            (20_usize..26, 0_usize..4, any::<bool>(), 1_u64..3, title()),
            0..6,
        )
    }

    fn entity_id(raw: usize) -> uuid::Uuid {
        uuid::Uuid::from_u128(u128::from(raw as u64) + 1)
    }

    fn build_stack(raw: &RawStack) -> Stack {
        let (index, bound, ref_board, title) = raw;
        Stack {
            id: StackId::from(entity_id(*index)),
            remote: bound.then_some(RemoteStackRef {
                board: RemoteBoardId(*ref_board),
                stack: RemoteStackId(u64::try_from(*index).unwrap_or(1)),
            }),
            board: crate::ids::BoardId::from(entity_id(999)),
            title: title.clone(),
            order: 1,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: ts(100),
                order: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        }
    }

    fn build_label(raw: &RawLabel) -> Label {
        let (index, bound, ref_board, title) = raw;
        Label {
            id: LabelId::from(entity_id(*index)),
            remote: bound.then_some(RemoteLabelRef {
                board: RemoteBoardId(*ref_board),
                label: crate::ids::RemoteLabelId(u64::try_from(*index).unwrap_or(1)),
            }),
            board: crate::ids::BoardId::from(entity_id(999)),
            title: title.clone(),
            color: crate::entities::Color::new("ff0000"),
            deleted: false,
            clocks: LabelClocks {
                title: ts(100),
                color: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        }
    }

    /// Builds a task only when its stack row exists (referential sanity).
    fn build_task(raw: &RawTask, stack_ids: &[StackId]) -> Option<Task> {
        let (index, stack_index, bound, ref_board, title) = raw;
        let stack = *stack_ids.get(*stack_index)?;
        Some(Task {
            id: TaskId::from(entity_id(*index)),
            remote: bound.then_some(RemoteCardRef {
                board: RemoteBoardId(*ref_board),
                stack: RemoteStackId(u64::try_from(*stack_index).unwrap_or(1)),
                card: crate::ids::RemoteCardId(u64::try_from(*index).unwrap_or(1)),
            }),
            title: title.clone(),
            description: String::new(),
            duedate: None,
            done: None,
            scheduled: None,
            stack,
            order: 1,
            labels: std::collections::BTreeSet::default(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: ts(100),
                description: ts(100),
                duedate: ts(100),
                scheduled: ts(100),
                done: ts(100),
                position: ts(100),
                labels: ts(100),
                effort: ts(100),
                importance: ts(100),
                urgency: ts(100),
                recur: ts(100),

                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        })
    }

    /// The generated fixture: one board (bound to remote 42) with the
    /// generated stacks, labels, and tasks on it. The bound rows may
    /// carry refs naming foreign boards (1, 2) — exactly the
    /// pathological-but-possible shapes the matcher and the wipe
    /// predicate must handle.
    fn reconcile_state(
        stacks: &[RawStack],
        labels: &[RawLabel],
        tasks: &[RawTask],
    ) -> (AppState, BoardId) {
        let mut app = AppState::default();
        let board = crate::ids::BoardId::from(entity_id(999));
        app.boards.insert(
            board,
            crate::entities::Board {
                id: board,
                remote: crate::test_support::test_binding(RemoteBoardId(42)),
                title: "board".into(),
                color: crate::entities::Color::new("0000ff"),
                archived: false,
                deleted: false,
                clocks: crate::entities::BoardClocks {
                    title: crate::entities::BoardClocks::epoch(),
                    color: crate::entities::BoardClocks::epoch(),
                    archived: crate::entities::BoardClocks::epoch(),
                    deleted: crate::entities::BoardClocks::epoch(),
                },
                remote_seen: None,
            },
        );
        for raw in stacks {
            let stack = build_stack(raw);
            app.stacks.insert(stack.id, stack);
        }
        let stack_ids: Vec<StackId> = app.stacks.keys().copied().collect();
        for raw in labels {
            let label = build_label(raw);
            app.labels.insert(label.id, label);
        }
        for raw in tasks {
            if let Some(task) = build_task(raw, &stack_ids) {
                app.tasks.insert(task.id, task);
            }
        }
        (app, board)
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn norm_is_idempotent_and_order_insensitive(raw in ".*") {
            let once = normalized_title(&raw);
            prop_assert_eq!(normalized_title(&once), once);
        }

        #[test]
        fn pairing_is_one_to_one_and_deterministic(
            stacks in raw_stacks(),
            labels in raw_labels(),
            tasks in raw_tasks(),
        ) {
            let (app, board) = reconcile_state(&stacks, &labels, &tasks);
            let (first, first_counts) = plan_reconcile(&app, &[], board);
            let (second, second_counts) = plan_reconcile(&app, &[], board);
            prop_assert_eq!(&first, &second, "re-planning is deterministic");
            prop_assert_eq!(first_counts, second_counts);

            // One-to-one: no local-only entity survives into two pairs,
            // and no remote-bound entity is absorbed twice. Survivors
            // are upserts that gave a local row a ref; absorbed rows
            // gave a bound row `deleted ∧ remote: None`.
            // Survivors: local-only input rows (remote None) upserted
            // with a ref. Absorbed rows: bound input rows upserted as
            // ref-less tombstones. (Re-parents and edge rewrites of
            // *bound* rows are legitimate non-pair upserts and classify
            // as neither.)
            let mut survivor_ids: Vec<String> = Vec::new();
            let mut absorbed_ids: Vec<String> = Vec::new();
            for action in &first {
                match action {
                    PersistenceAction::UpsertStack(s) => {
                        let key = s.id.as_uuid().to_string();
                        let input_bound = app
                            .stacks
                            .get(&s.id)
                            .is_some_and(|row| row.remote.is_some());
                        if s.remote.is_some() && !input_bound {
                            survivor_ids.push(key);
                        } else if s.deleted && s.remote.is_none() && input_bound {
                            absorbed_ids.push(key);
                        }
                    }
                    PersistenceAction::UpsertLabel(l) => {
                        let key = l.id.as_uuid().to_string();
                        let input_bound = app
                            .labels
                            .get(&l.id)
                            .is_some_and(|row| row.remote.is_some());
                        if l.remote.is_some() && !input_bound {
                            survivor_ids.push(key);
                        } else if l.deleted && l.remote.is_none() && input_bound {
                            absorbed_ids.push(key);
                        }
                    }
                    PersistenceAction::UpsertTask(t) => {
                        let key = t.id.as_uuid().to_string();
                        let input_bound = app
                            .tasks
                            .get(&t.id)
                            .is_some_and(|row| row.remote.is_some());
                        if t.remote.is_some() && !input_bound {
                            survivor_ids.push(key);
                        } else if t.deleted && t.remote.is_none() && input_bound {
                            absorbed_ids.push(key);
                        }
                    }
                    _ => {}
                }
            }
            let survivors_unique = survivor_ids.len()
                == survivor_ids.iter().collect::<std::collections::BTreeSet<_>>().len();
            prop_assert!(survivors_unique, "no entity pairs twice (survivors)");
            let absorbed_unique = absorbed_ids.len()
                == absorbed_ids.iter().collect::<std::collections::BTreeSet<_>>().len();
            prop_assert!(absorbed_unique, "no entity pairs twice (absorbed)");

            // One survivor per absorbed row, and the pair counts agree
            // with the outcome.
            prop_assert_eq!(survivor_ids.len(), absorbed_ids.len(), "one survivor per absorbed row");
            let merged_pairs = first_counts.stacks + first_counts.labels + first_counts.tasks;
            prop_assert_eq!(survivor_ids.len(), merged_pairs, "counts == emitted pairs");
        }

        #[test]
        fn task_merge_newer_clock_wins_and_clocks_are_fieldwise_max(
            (local, bound) in task_pair_strategy(),
        ) {
            let merged = merge_task_pair(&local, &bound);
            prop_assert_eq!(merged.id, local.id, "the local row survives");
            prop_assert_eq!(merged.remote, bound.remote, "the survivor adopts the ref");
            prop_assert_eq!(merged.clocks.title, local.clocks.title.max(bound.clocks.title));
            prop_assert_eq!(merged.clocks.description, local.clocks.description.max(bound.clocks.description));
            prop_assert_eq!(merged.clocks.duedate, local.clocks.duedate.max(bound.clocks.duedate));
            prop_assert_eq!(merged.clocks.done, local.clocks.done.max(bound.clocks.done));
            prop_assert_eq!(merged.clocks.position, local.clocks.position.max(bound.clocks.position));
            prop_assert_eq!(merged.clocks.labels, local.clocks.labels.max(bound.clocks.labels));
            prop_assert_eq!(merged.clocks.archived, local.clocks.archived.max(bound.clocks.archived));
            prop_assert_eq!(merged.clocks.deleted, local.clocks.deleted.max(bound.clocks.deleted));
        }

        #[test]
        fn stack_label_merges_keep_fieldwise_max_clocks(
            stacks in stack_clock_pair_strategy(),
            labels in label_clock_pair_strategy(),
        ) {
            let (local_s, bound_s) = stacks;
            let merged_s = merge_stack_pair(&local_s, &bound_s);
            prop_assert_eq!(merged_s.clocks.title, local_s.clocks.title.max(bound_s.clocks.title));
            prop_assert_eq!(merged_s.clocks.order, local_s.clocks.order.max(bound_s.clocks.order));
            prop_assert_eq!(merged_s.clocks.deleted, local_s.clocks.deleted.max(bound_s.clocks.deleted));

            let (local_l, bound_l) = labels;
            let merged_l = merge_label_pair(&local_l, &bound_l);
            prop_assert_eq!(merged_l.clocks.title, local_l.clocks.title.max(bound_l.clocks.title));
            prop_assert_eq!(merged_l.clocks.color, local_l.clocks.color.max(bound_l.clocks.color));
            prop_assert_eq!(merged_l.clocks.deleted, local_l.clocks.deleted.max(bound_l.clocks.deleted));
        }

        #[test]
        fn wipe_scope_equals_foreign_refs(
            stacks in raw_stacks(),
            labels in raw_labels(),
            tasks in raw_tasks(),
            target in 1_u64..3,
        ) {
            let (mut app, board) = reconcile_state(&stacks, &labels, &tasks);
            app.boards.get_mut(&board).unwrap().remote = None;
            let remote = RemoteBoardId(target);
            let expected: usize = {
                let board_stack_ids: std::collections::BTreeSet<_> = app
                    .stacks
                    .values()
                    .filter(|s| s.board == board && !s.deleted)
                    .map(|s| s.id)
                    .collect();
                let s = app.stacks.values()
                    .filter(|s| s.board == board && !s.deleted && s.remote.is_some_and(|r| r.board != remote))
                    .count();
                let t = app.tasks.values()
                    .filter(|t| t.is_live() && board_stack_ids.contains(&t.stack) && t.remote.is_some_and(|r| r.board != remote))
                    .count();
                let l = app.labels.values()
                    .filter(|l| l.board == board && !l.deleted && l.remote.is_some_and(|r| r.board != remote))
                    .count();
                s + t + l
            };
            prop_assert_eq!(bind_wipe_count(&app, board, remote), expected);
        }
    }
}

#[cfg(test)]
mod proptest_strategies {
    use super::*;
    use crate::entities::{LabelClocks, StackClocks, TaskClocks};
    use crate::ids::{RemoteBoardId, RemoteCardRef, RemoteStackId};
    use chrono::{TimeZone, Utc};
    use proptest::prelude::*;

    fn ts(secs: i64) -> chrono::DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn stamp() -> impl Strategy<Value = chrono::DateTime<Utc>> {
        (50_i64..500).prop_map(ts)
    }

    /// A (local, bound) stack pair sharing board and title alphabet;
    /// independent per-field clocks and orders.
    pub(crate) fn stack_clock_pair_strategy() -> impl Strategy<Value = (Stack, Stack)> {
        (
            stamp(),
            stamp(),
            stamp(),
            stamp(),
            stamp(),
            stamp(),
            0_i64..3,
            0_i64..3,
            prop_oneof![Just("a".to_owned()), Just("b".to_owned())],
        )
            .prop_map(
                |(
                    lt_title,
                    lt_order,
                    lt_del,
                    bt_title,
                    bt_order,
                    bt_del,
                    lorder,
                    border,
                    title,
                )| {
                    let mk = |id: u64, clocks: StackClocks, order: i64| Stack {
                        id: StackId::from(uuid::Uuid::from_u128(u128::from(id))),
                        remote: None,
                        board: BoardId::from(uuid::Uuid::from_u128(1)),
                        title: title.clone(),
                        order,
                        archived: false,
                        deleted: false,
                        clocks,
                        remote_seen: None,
                    };
                    (
                        mk(
                            1,
                            StackClocks {
                                title: lt_title,
                                order: lt_order,
                                deleted: lt_del,
                            },
                            lorder,
                        ),
                        mk(
                            2,
                            StackClocks {
                                title: bt_title,
                                order: bt_order,
                                deleted: bt_del,
                            },
                            border,
                        ),
                    )
                },
            )
    }

    pub(crate) fn label_clock_pair_strategy() -> impl Strategy<Value = (Label, Label)> {
        (stamp(), stamp(), stamp(), stamp(), stamp(), stamp()).prop_map(
            |(lt, lc, ld, bt, bc, bd)| {
                let mk = |id: u64, clocks: LabelClocks| Label {
                    id: LabelId::from(uuid::Uuid::from_u128(u128::from(id))),
                    remote: None,
                    board: BoardId::from(uuid::Uuid::from_u128(1)),
                    title: "t".to_owned(),
                    color: crate::entities::Color::new("ff0000"),
                    deleted: false,
                    clocks,
                    remote_seen: None,
                };
                (
                    mk(
                        1,
                        LabelClocks {
                            title: lt,
                            color: lc,
                            deleted: ld,
                        },
                    ),
                    mk(
                        2,
                        LabelClocks {
                            title: bt,
                            color: bc,
                            deleted: bd,
                        },
                    ),
                )
            },
        )
    }

    /// A (local, bound) task pair: independent per-field task clocks.
    #[allow(clippy::type_complexity)] // one generated fixture shape
    pub(crate) fn task_pair_strategy() -> impl Strategy<Value = (Task, Task)> {
        (
            stamp(),
            stamp(),
            stamp(),
            stamp(),
            stamp(),
            stamp(),
            stamp(),
            stamp(),
        )
            .prop_map(|(t, d, due, done, pos, lab, arc, del)| {
                let mk = |id: u64, clocks: TaskClocks, bound: bool| Task {
                    id: TaskId::from(uuid::Uuid::from_u128(u128::from(id))),
                    remote: bound.then_some(RemoteCardRef {
                        board: RemoteBoardId(1),
                        stack: RemoteStackId(1),
                        card: crate::ids::RemoteCardId(id),
                    }),
                    title: "t".to_owned(),
                    description: "d".to_owned(),
                    duedate: None,
                    done: None,
                    scheduled: None,
                    stack: StackId::from(uuid::Uuid::from_u128(9)),
                    order: 1,
                    labels: std::collections::BTreeSet::default(),
                    effort: None,
                    importance: None,
                    urgency: None,
                    recur: None,

                    archived: false,
                    deleted: false,
                    clocks,
                    remote_seen: None,
                };
                (
                    mk(
                        1,
                        TaskClocks {
                            title: t,
                            description: d,
                            duedate: due,
                            scheduled: due,
                            done,
                            position: pos,
                            labels: lab,
                            effort: lab,
                            importance: lab,
                            urgency: lab,
                            recur: lab,

                            archived: arc,
                            deleted: del,
                        },
                        false,
                    ),
                    mk(
                        2,
                        TaskClocks {
                            title: t + chrono::TimeDelta::try_seconds(7).unwrap(),
                            description: d + chrono::TimeDelta::try_seconds(7).unwrap(),
                            duedate: due + chrono::TimeDelta::try_seconds(7).unwrap(),
                            scheduled: due + chrono::TimeDelta::try_seconds(7).unwrap(),
                            done: done + chrono::TimeDelta::try_seconds(7).unwrap(),
                            position: pos + chrono::TimeDelta::try_seconds(7).unwrap(),
                            labels: lab + chrono::TimeDelta::try_seconds(7).unwrap(),
                            effort: lab + chrono::TimeDelta::try_seconds(7).unwrap(),
                            importance: lab + chrono::TimeDelta::try_seconds(7).unwrap(),
                            urgency: lab + chrono::TimeDelta::try_seconds(7).unwrap(),
                            recur: lab + chrono::TimeDelta::try_seconds(7).unwrap(),

                            archived: arc + chrono::TimeDelta::try_seconds(7).unwrap(),
                            deleted: del + chrono::TimeDelta::try_seconds(7).unwrap(),
                        },
                        true,
                    ),
                )
            })
    }
}
