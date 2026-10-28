// SPDX-License-Identifier: MIT OR Apache-2.0
//! Manifest assertion 4 (#244, D-F3's mechanics): every `TASK_FIELDS`
//! row's sync lane routes through `plan_command` exactly as the manifest
//! says. The manifest is the single enumeration source — a fifteenth
//! field without routing fails this table; a routed slot without a
//! manifest row cannot be expressed.
//!
//! The table drives `plan_command` directly (the pure layer the engine's
//! execute path delegates to): the engine adds transport and bookkeeping,
//! never routing (chunk 1's engine tests pin that split).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{TimeZone, Utc};
use taskboard_domain::entities::{FieldFamily, TASK_FIELDS};
use taskboard_domain::persistence::{PersistedState, PersistenceAction, apply_actions};
use taskboard_domain::state::{AppState, SyncStatus};
use taskboard_domain::test_support::CountingIds;
use taskboard_domain::{
    CommandOutcome, DeckFeatures, EntityTables, LocalOp, StackId, StateCommand, SyncStates,
    TaskChanges, TaskId, plan_command,
};

mod common;

fn ts(secs: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

const BOARD: u128 = 800;
const STACK: u128 = 900;
const TASK: u128 = 1;

fn board_id() -> taskboard_domain::BoardId {
    taskboard_domain::BoardId::from(uuid::Uuid::from_u128(BOARD))
}

fn stack_id() -> StackId {
    StackId::from(uuid::Uuid::from_u128(STACK))
}

fn task_id() -> TaskId {
    TaskId::from(uuid::Uuid::from_u128(TASK))
}

/// One board (row carrying `features`), one stack, one live task — the
/// minimal state every `UpdateTask` routing needs.
fn app_with(features: DeckFeatures) -> AppState {
    let mut app = AppState::default();
    let mut board = taskboard_domain::Board {
        id: board_id(),
        remote: None,
        title: "board".into(),
        color: taskboard_domain::Color::new("0000ff"),
        archived: false,
        deleted: false,
        clocks: {
            let e = taskboard_domain::BoardClocks::epoch();
            taskboard_domain::BoardClocks {
                title: e,
                color: e,
                archived: e,
                deleted: e,
            }
        },
        remote_seen: None,
    };
    board.id = board_id();
    app.boards.insert(board.id, board);
    let stack = taskboard_domain::Stack {
        id: stack_id(),
        remote: None,
        board: board_id(),
        title: "stack".into(),
        order: 0,
        archived: false,
        deleted: false,
        clocks: taskboard_domain::StackClocks {
            title: ts(0),
            order: ts(0),
            deleted: ts(0),
        },
        remote_seen: None,
    };
    app.stacks.insert(stack.id, stack);
    let task = taskboard_domain::Task {
        id: task_id(),
        remote: None,
        title: "task".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack: stack_id(),
        order: 0,
        label_memberships: BTreeMap::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,

        archived: false,
        deleted: false,
        clocks: taskboard_domain::TaskClocks {
            title: ts(0),
            description: ts(0),
            duedate: ts(0),
            scheduled: ts(0),
            done: ts(0),
            position: ts(0),
            effort: ts(0),
            importance: ts(0),
            urgency: ts(0),
            recur: ts(0),

            archived: ts(0),
            deleted: ts(0),
        },
        remote_seen: None,
        remote_baseline: None,
    };
    app.tasks.insert(task.id, task);
    app.sync = SyncStates::with_row(
        board_id(),
        SyncStatus {
            features,
            ..SyncStatus::default()
        },
    );
    app
}

/// The single-slot `TaskChanges` changeset for a manifest row's field
/// (`None` for rows `EditTask` cannot carry).
fn changes_for(field: &str) -> Option<TaskChanges> {
    let mut changes = TaskChanges::default();
    match field {
        "title" => changes.title = Some("renamed".into()),
        "description" => changes.description = Some("described".into()),
        "duedate" => changes.duedate = Some(Some(ts(86_400))),
        "scheduled" => changes.scheduled = Some(Some(ts(86_400))),
        "archived" => changes.archived = Some(true),
        "effort" => changes.effort = Some(Some(taskboard_domain::entities::Effort::new(900))),
        "importance" => {
            changes.importance = Some(Some(taskboard_domain::entities::Importance::High));
        }
        "urgency" => changes.urgency = Some(Some(taskboard_domain::entities::Urgency::Low)),
        "recur" => {
            let rule = taskboard_domain::recur::RecurRule {
                series: taskboard_domain::SeriesId::from_uuid(uuid::Uuid::from_u128(7)),
                anchor: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                pattern: taskboard_domain::recur::RecurPattern::Daily {
                    every: taskboard_domain::recur::EveryDays::new(2).unwrap(),
                },
                bound: taskboard_domain::recur::RecurBound::None,
                continued_from: None,
            };
            changes.recur = Some(Some(rule));
        }
        // `done` rides SetTaskDone, `position` MoveTask, `labels` the
        // assign/unassign verbs, `deleted` DeleteTask (the native
        // routables chunk 1's engine tests already pin).
        "done" | "position" | "labels" | "deleted" => return None,
        other => panic!("unknown manifest field `{other}` — extend the table"),
    }
    Some(changes)
}

/// The injected day calendar of the routing tests: UTC midnight — the
/// plain dates of the fixture instants are the window-dates too.
fn test_day() -> taskboard_domain::DayCalendar {
    taskboard_domain::DayCalendar {
        day_start_time: chrono::NaiveTime::MIN,
        offset: chrono::FixedOffset::east_opt(0).unwrap(),
    }
}

/// The routed op of a single-slot `UpdateTask` through `plan_command`.
fn routed_op(app: &AppState, field: &str) -> LocalOp {
    let changes = changes_for(field).unwrap_or_else(|| panic!("`{field}` has no EditTask slot"));
    let plan = plan_command(
        app,
        &[],
        &StateCommand::UpdateTask {
            id: task_id(),
            changes,
        },
        &CountingIds::new(),
        &taskboard_domain::ReminderDefaults::default(),
        test_day(),
        ts(100),
    )
    .unwrap();
    assert_eq!(plan.outcome, CommandOutcome::Applied);
    let enqueued: Vec<LocalOp> = plan
        .actions
        .iter()
        .filter_map(|action| match action {
            PersistenceAction::EnqueueOp(op) => Some(op.op),
            _ => None,
        })
        .collect();
    assert_eq!(
        enqueued.len(),
        1,
        "a single-slot edit routes exactly one op (got {enqueued:?})"
    );
    enqueued[0]
}

#[test]
fn every_manifest_row_routes_through_its_declared_lane() {
    // The AppData rows: exactly `UpdateAppData`, never a native write.
    for (clock, fields, family) in TASK_FIELDS {
        if !matches!(family, FieldFamily::AppData) {
            continue;
        }
        assert_eq!(fields.len(), 1, "app rows are single-field");
        assert_eq!(*clock, fields[0], "app rows name their own clock");
        let op = routed_op(&app_with(DeckFeatures::CONSERVATIVE), fields[0]);
        assert!(
            matches!(op, LocalOp::UpdateAppData(id) if id == task_id()),
            "manifest row `{clock}` (AppData) must route UpdateAppData, got {op:?}"
        );
    }

    // The native rows `EditTask` carries: exactly `UpdateTask` on a
    // capability-true board (the honest native arm for every row).
    for (clock, _fields, family) in TASK_FIELDS {
        if !matches!(family, FieldFamily::Native) || changes_for(clock).is_none() {
            continue;
        }
        let op = routed_op(&app_with(DeckFeatures::NATIVE), clock);
        assert!(
            matches!(op, LocalOp::UpdateTask(id) if id == task_id()),
            "manifest row `{clock}` (Native) must route UpdateTask, got {op:?}"
        );
        // The other native slots never enqueue the document lane.
        assert!(
            !matches!(op, LocalOp::UpdateAppData(_)),
            "native row `{clock}` must never touch the app-data lane"
        );
    }

    // The `scheduled` row runs three times (D-V2's lane rule): native
    // while the flag is true, document lane while false, and the
    // unbound-board default (CONSERVATIVE) rides the document lane.
    let native = routed_op(&app_with(DeckFeatures::NATIVE), "scheduled");
    assert!(matches!(native, LocalOp::UpdateTask(id) if id == task_id()));
    let conservative = routed_op(&app_with(DeckFeatures::CONSERVATIVE), "scheduled");
    assert!(matches!(conservative, LocalOp::UpdateAppData(id) if id == task_id()));
    // The unbound default: no board row at all.
    let mut app = app_with(DeckFeatures::NATIVE);
    app.sync = SyncStates::new();
    let plan = plan_command(
        &app,
        &[],
        &StateCommand::UpdateTask {
            id: task_id(),
            changes: TaskChanges {
                scheduled: Some(Some(ts(86_400))),
                ..TaskChanges::default()
            },
        },
        &CountingIds::new(),
        &taskboard_domain::ReminderDefaults::default(),
        test_day(),
        ts(100),
    )
    .unwrap();
    assert!(plan.actions.iter().any(|action| matches!(
        action,
        PersistenceAction::EnqueueOp(op) if matches!(op.op, LocalOp::UpdateAppData(id) if id == task_id())
    )));

    // Mixed changeset (one native + one app slot) ⇒ both ops — the
    // D-F3 contract sentence, pinned.
    let plan = plan_command(
        &app_with(DeckFeatures::NATIVE),
        &[],
        &StateCommand::UpdateTask {
            id: task_id(),
            changes: TaskChanges {
                title: Some("renamed".into()),
                effort: Some(Some(taskboard_domain::entities::Effort::new(5))),
                ..TaskChanges::default()
            },
        },
        &CountingIds::new(),
        &taskboard_domain::ReminderDefaults::default(),
        test_day(),
        ts(100),
    )
    .unwrap();
    let enqueued: Vec<LocalOp> = plan
        .actions
        .iter()
        .filter_map(|action| match action {
            PersistenceAction::EnqueueOp(op) => Some(op.op),
            _ => None,
        })
        .collect();
    assert_eq!(enqueued.len(), 2, "mixed changeset enqueues both ops");
    assert!(
        enqueued
            .iter()
            .any(|op| matches!(op, LocalOp::UpdateTask(id) if *id == task_id()))
    );
    assert!(
        enqueued
            .iter()
            .any(|op| matches!(op, LocalOp::UpdateAppData(id) if *id == task_id()))
    );
}

/// The complement: every `EditTask`-carriable slot is covered by a
/// manifest row (a routed slot without a manifest row cannot be
/// expressed). Deriving the routable set from `TaskChanges`'s serde
/// shape keeps the table two-sided.
#[test]
fn every_taskchanges_slot_is_manifest_covered() {
    use taskboard_domain::TaskChanges;
    // The exhaustive literal IS the enumeration: a new `TaskChanges`
    // slot breaks this construction (no `..Default` escape), so the
    // table cannot silently miss one.
    let _all = TaskChanges {
        title: Some(String::new()),
        description: Some(String::new()),
        duedate: Some(None),
        scheduled: Some(None),
        effort: Some(None),
        importance: Some(None),
        urgency: Some(None),
        recur: None,

        archived: Some(true),
    };
    let slots: Vec<&str> = vec![
        "title",
        "description",
        "duedate",
        "scheduled",
        "effort",
        "importance",
        "urgency",
        "archived",
    ];
    let manifest_fields: BTreeSet<&str> = TASK_FIELDS
        .iter()
        .flat_map(|(_, fields, _)| fields.iter())
        .copied()
        .collect();
    for slot in slots {
        assert!(
            manifest_fields.contains(slot),
            "TaskChanges slot `{slot}` has no TASK_FIELDS row"
        );
        assert!(
            changes_for(slot).is_some(),
            "slot `{slot}` lost its single-slot routing entry"
        );
    }
    // Partition sanity: the non-`UpdateTask` slots (done/position/deleted)
    // are manifest rows too, but ride their own verbs (the composite
    // position row names `stack` + `order`). Labels are no longer a
    // manifest row: since the OR-set amendment (#129) the memberships
    // carry per-entry state, not a `TaskClocks` slot, and they still
    // never ride `UpdateTask` (their own Assign/Unassign verbs).
    for (field, manifest_names) in [
        ("done", vec!["done"]),
        ("position", vec!["stack", "order"]),
        ("deleted", vec!["deleted"]),
    ] {
        for name in manifest_names {
            assert!(manifest_fields.contains(name), "`{name}` missing");
        }
        assert!(changes_for(field).is_none());
    }
    assert!(
        !manifest_fields.contains("labels"),
        "labels left the clock manifest with the whole-set clock"
    );
    assert!(
        changes_for("labels").is_none(),
        "labels never ride UpdateTask"
    );
    // The planner's bucket accept the routed ops (partition property's
    // precondition): an UpdateAppData op buckets into no native group.
    let op = taskboard_domain::PendingOp {
        op_id: taskboard_domain::OpId(uuid::Uuid::from_u128(1)),
        op: LocalOp::UpdateAppData(task_id()),
        queued_at: ts(0),
    };
    let groups = taskboard_domain::plan_pushes(
        &[op],
        EntityTables {
            tasks: &app_with(DeckFeatures::CONSERVATIVE).tasks,
            stacks: &app_with(DeckFeatures::CONSERVATIVE).stacks,
            labels: &std::collections::BTreeMap::default(),
            boards: &std::collections::BTreeMap::default(),
        },
    );
    // The op buckets into exactly one group — the board's document
    // flush, never a native wire group (commit 6's coalescing).
    assert_eq!(groups.len(), 1);
    assert!(
        matches!(groups[0].target, taskboard_domain::PushTarget::Document(_)),
        "a document-lane op must never misroute as a native group"
    );
}

/// The engine's execute path routes identically (one spot check: the
/// routing decision is `plan_command`'s, the engine adds transport
/// only) — observed through the repository's persisted outbox.
#[tokio::test]
async fn the_engine_execute_path_routes_like_the_planner() {
    use std::sync::Arc;
    use taskboard_domain::persistence::TaskRepository;
    use taskboard_domain::test_support::InMemoryRepository;
    use taskboard_state::{ReminderEngineConfig, spawn_state_engine};

    let mut state = PersistedState::default();
    apply_actions(
        &mut state,
        &[
            PersistenceAction::UpsertBoard(
                app_with(DeckFeatures::CONSERVATIVE)
                    .boards
                    .get(&board_id())
                    .unwrap()
                    .clone(),
            ),
            PersistenceAction::UpsertStack(
                app_with(DeckFeatures::CONSERVATIVE)
                    .stacks
                    .get(&stack_id())
                    .unwrap()
                    .clone(),
            ),
            PersistenceAction::UpsertTask(
                app_with(DeckFeatures::CONSERVATIVE)
                    .tasks
                    .get(&task_id())
                    .unwrap()
                    .clone(),
            ),
            PersistenceAction::UpsertSyncStatus(
                board_id(),
                SyncStatus {
                    features: DeckFeatures::CONSERVATIVE,
                    ..SyncStatus::default()
                },
            ),
        ],
    );
    let repo = Arc::new(InMemoryRepository::with_state(state));
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<taskboard_domain::SyncCommand>(8);
    let (report_tx, report_rx) = tokio::sync::mpsc::channel::<taskboard_domain::SyncReport>(8);
    let (system_tx, system_rx) =
        tokio::sync::broadcast::channel::<taskboard_domain::SystemEvent>(8);
    let (notifier, _notifications) =
        tokio::sync::mpsc::channel::<taskboard_domain::NotifierMessage>(8);
    let (handle, join) = spawn_state_engine(
        repo.clone(),
        Arc::new(CountingIds::new()),
        common::FixedClock::new(0),
        sync_out,
        report_rx,
        system_tx,
        system_rx,
        notifier,
        ReminderEngineConfig::disabled(),
        taskboard_state::DayRollConfig::disabled(),
    )
    .await
    .expect("engine boots");
    handle
        .execute(StateCommand::UpdateTask {
            id: task_id(),
            changes: TaskChanges {
                effort: Some(Some(taskboard_domain::entities::Effort::new(900))),
                ..TaskChanges::default()
            },
        })
        .await
        .expect("accepted");
    let stored = repo.load().await.expect("load");
    assert!(stored.outbox.iter().any(|op| matches!(
        op.op,
        LocalOp::UpdateAppData(id) if id == task_id()
    )));
    drop(handle); // closes the command inbox…
    drop(report_tx); // …and the reports source: the loop can exit
    let _ = join.await;
}
