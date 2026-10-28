// SPDX-License-Identifier: MIT OR Apache-2.0
//! P1–P4 round-trip proptests and T5–T8 targeted storage semantics tests
//! (plan §10 catalogue).

mod common;

use std::collections::{BTreeMap, BTreeSet};

use chrono::{TimeZone, Utc};
use proptest::prelude::*;
use taskboard_domain::entities::{
    Board, Color, Label, LabelClocks, Stack, StackClocks, Task, TaskClocks,
};
use taskboard_domain::ids::{
    BoardId, LabelId, RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef,
    RemoteStackId, RemoteStackRef, StackId, TaskId,
};
use taskboard_domain::outbox::{LocalOp, OpId, PendingOp};
use taskboard_domain::persistence::{
    PersistedState, PersistenceAction, RepositoryError, SyncValidators, TaskRepository,
    ValidatorKey,
};
use taskboard_domain::state::{SyncErrorKind, SyncPhase, SyncStates};
use taskboard_storage_sqlite::open_memory;

use common::actions_from_state;

fn ts(secs: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

fn task_clocks(secs: i64) -> TaskClocks {
    let t = ts(secs);
    TaskClocks {
        title: t,
        description: t,
        duedate: t,
        scheduled: t,
        done: t,
        position: t,
        effort: t,
        importance: t,
        urgency: t,
        recur: t,

        archived: t,
        deleted: t,
    }
}

fn stack_clocks(secs: i64) -> StackClocks {
    let t = ts(secs);
    StackClocks {
        title: t,
        order: t,
        deleted: t,
    }
}

fn label_clocks(secs: i64) -> LabelClocks {
    let t = ts(secs);
    LabelClocks {
        title: t,
        color: t,
        deleted: t,
    }
}

fn board(id: u128) -> Board {
    Board {
        id: BoardId::from(uuid::Uuid::from_u128(id)),
        remote: Some(taskboard_domain::RemoteBoardBinding {
            connector: taskboard_domain::ConnectorId::from("nextcloud"),
            board: RemoteBoardId(77),
        }),
        title: "board".into(),
        color: Color::new("ff0000"),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::BoardClocks {
            title: ts(100),
            color: ts(100),
            archived: ts(100),
            deleted: ts(100),
        },
        remote_seen: Some(ts(100)),
    }
}

fn stack(id: u128, board: &Board) -> Stack {
    Stack {
        id: StackId::from(uuid::Uuid::from_u128(id)),
        remote: Some(RemoteStackRef {
            board: RemoteBoardId(77),
            stack: RemoteStackId(7),
        }),
        board: board.id,
        title: "stack".into(),
        order: 3,
        archived: false,
        deleted: false,
        clocks: stack_clocks(1),
        remote_seen: None,
    }
}

fn label(id: u128, board: &Board) -> Label {
    Label {
        id: LabelId::from(uuid::Uuid::from_u128(id)),
        remote: Some(RemoteLabelRef {
            board: RemoteBoardId(77),
            label: RemoteLabelId(9),
        }),
        board: board.id,
        title: "label".into(),
        color: Color::new("00ff00"),
        deleted: false,
        clocks: label_clocks(2),
        remote_seen: Some(ts(101)),
    }
}

fn task(
    id: u128,
    stack: &Stack,
    labels: BTreeMap<LabelId, taskboard_domain::entities::MembershipState>,
) -> Task {
    Task {
        id: TaskId::from(uuid::Uuid::from_u128(id)),
        remote: Some(RemoteCardRef {
            board: RemoteBoardId(77),
            stack: RemoteStackId(7),
            card: RemoteCardId(8),
        }),
        title: "task".into(),
        description: "desc".into(),
        duedate: Some(ts(200)),
        scheduled: None,
        done: None,
        stack: stack.id,
        order: 1,
        label_memberships: labels,
        effort: None,
        importance: None,
        urgency: None,
        recur: None,

        archived: false,
        deleted: false,
        clocks: task_clocks(5),
        remote_seen: Some(ts(102)),
        remote_baseline: None,
    }
}

// P1 — full-state round trip through one apply batch: everything comes
// back identical except `sync.pending_ops`, which is re-derived from the
// outbox depth.
proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(256))]

    #[test]
    fn persisted_state_roundtrips_through_sqlite(
        state in taskboard_domain::test_support::persisted_state_strategy(),
    ) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(async move {
                let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud")).await.expect("open");
                repo.apply(actions_from_state(&state)).await.expect("apply");
                let loaded = repo.load().await.expect("load");

                prop_assert_eq!(loaded.boards, state.boards);
                prop_assert_eq!(loaded.stacks, state.stacks);
                prop_assert_eq!(loaded.labels, state.labels);
                prop_assert_eq!(loaded.tasks, state.tasks);
                let expected_pending = state.outbox.len();
                prop_assert_eq!(loaded.outbox, state.outbox);
                prop_assert_eq!(loaded.validators, state.validators);
                prop_assert_eq!(loaded.reminders, state.reminders);
                prop_assert_eq!(loaded.tracking, state.tracking);
                prop_assert_eq!(loaded.sync.aggregate().phase, state.sync.aggregate().phase);
                prop_assert_eq!(loaded.sync.aggregate().last_success, state.sync.aggregate().last_success);
                prop_assert_eq!(
                    loaded.sync.aggregate().pending_ops as usize,
                    expected_pending,
                    "pending_ops must be derived from the outbox depth"
                );
                Ok::<(), proptest::test_runner::TestCaseError>(())
            })
            .expect("proptest case");
    }
}

/// P2 — outbox order is the enqueue order (`op_seq`), independent of
/// `queued_at`; CompleteOp/FailOp remove by id without disturbing the rest.
#[tokio::test]
async fn outbox_preserves_enqueue_order_across_removals() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    // The ops' task must exist: the per-board pending attribution runs
    // through the entity graph.
    let task_id = TaskId::from(uuid::Uuid::from_u128(1));
    let seed_board = board(91);
    let seed_stack = stack(90, &seed_board);
    let seed_task = task(1, &seed_stack, BTreeMap::default());
    repo.apply(vec![
        PersistenceAction::UpsertBoard(seed_board),
        PersistenceAction::UpsertStack(seed_stack),
        PersistenceAction::UpsertTask(seed_task),
    ])
    .await
    .expect("seed entities");
    let ids: Vec<OpId> = (0..5).map(|i| OpId(uuid::Uuid::from_u128(i))).collect();
    let batch: Vec<PersistenceAction> = ids
        .iter()
        .map(|op_id| {
            PersistenceAction::EnqueueOp(PendingOp {
                op_id: *op_id,
                op: LocalOp::UpdateTask(task_id),
                queued_at: ts(42), // identical seconds: collisions in practice
            })
        })
        .collect();
    repo.apply(batch).await.expect("enqueue");

    let loaded = repo.load().await.expect("load");
    let order: Vec<OpId> = loaded.outbox.iter().map(|o| o.op_id).collect();
    assert_eq!(order, ids, "queue order must be enqueue order");

    repo.apply(vec![PersistenceAction::CompleteOp(ids[2])])
        .await
        .expect("complete");
    repo.apply(vec![PersistenceAction::FailOp(ids[0])])
        .await
        .expect("fail");
    let loaded = repo.load().await.expect("load");
    let order: Vec<OpId> = loaded.outbox.iter().map(|o| o.op_id).collect();
    assert_eq!(order, vec![ids[1], ids[3], ids[4]]);
    assert_eq!(loaded.sync.aggregate().pending_ops, 3);
}

/// The rule write clock (chunk 4): an upsert persists `updated_at` and
/// load returns it; a row written before this reader existed (the 0007
/// column default) decodes with the epoch — which loses every clock
/// race, exactly like `BoardClocks::epoch`'s precedent.
#[tokio::test]
async fn reminder_rule_round_trips_the_write_clock_and_epoch_decodes() {
    use taskboard_domain::ids::ReminderRuleId;
    use taskboard_domain::reminders::{ReminderOffset, ReminderRule};

    let mut db = common::FileDb::new().await;
    let repo = db.repo();
    let b = board(11);
    let s = stack(2, &b);
    let t = task(1, &s, BTreeMap::default());
    let rule_id = ReminderRuleId::from(uuid::Uuid::from_u128(20));
    let rule = ReminderRule {
        insistence: taskboard_domain::Insistence::Normal,
        id: rule_id,
        task: t.id,
        offset: ReminderOffset::before(chrono::TimeDelta::try_minutes(-5).unwrap())
            .expect("valid offset"),
        created_at: ts(1_000),
        updated_at: ts(2_000),
    };
    repo.apply(vec![
        PersistenceAction::UpsertBoard(b),
        PersistenceAction::UpsertStack(s),
        PersistenceAction::UpsertTask(t),
        PersistenceAction::UpsertReminderRule(rule.clone()),
    ])
    .await
    .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.reminders.rules.get(&rule_id), Some(&rule));

    // A row written before this reader existed: the DEFAULT epoch value
    // (and only that) is what load can return for it.
    let raw = sqlx::sqlite::SqliteConnectOptions::new().filename(db.path());
    let pool = sqlx::SqlitePool::connect_with(raw).await.expect("raw pool");
    sqlx::query("UPDATE reminder_rules SET updated_at = '0001-01-01T00:00:00+00:00'")
        .execute(&pool)
        .await
        .expect("backdate the row");
    sqlx::query(
        "INSERT INTO reminder_rules (rule_id, task_id, offset_seconds, created_at) \
         VALUES ('{00000000-0000-0000-0000-000000000021}', ?, -60, '1970-01-01T00:00:16+00:00')",
    )
    .bind(uuid::Uuid::from_u128(1).to_string())
    .execute(&pool)
    .await
    .expect("a column-default row");
    pool.close().await;

    let loaded = repo.load().await.expect("load");
    let epoch: chrono::DateTime<Utc> = Utc.timestamp_opt(0, 0).unwrap();
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339("0001-01-01T00:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc),
        loaded.reminders.rules[&rule_id].updated_at,
        "the backdated row decodes with the epoch"
    );
    let defaulted = ReminderRuleId::from(uuid::Uuid::from_u128(0x21));
    assert_eq!(
        loaded.reminders.rules[&defaulted].updated_at, loaded.reminders.rules[&rule_id].updated_at,
        "the column default decodes identically"
    );
    assert_eq!(
        loaded.reminders.rules[&defaulted].created_at,
        ts(16),
        "the other columns decode normally"
    );
    let _ = epoch;
    db.close().await;
}

/// The `offset_seconds` column stores the offset exactly: every
/// whole-second value in the type's domain round-trips identically
/// through write and reload (the double-fire guard — a lossy column
/// would recompute a different occurrence on reload). Exhaustive over
/// the domain's scale points, extending the single −5 min fixture.
#[tokio::test]
async fn rule_offset_roundtrips_identically_through_sqlite() {
    use taskboard_domain::ids::ReminderRuleId;
    use taskboard_domain::reminders::{ReminderOffset, ReminderRule};

    for (i, secs) in [0_i64, -1, -90, -3_600, -86_400, -2_592_000]
        .into_iter()
        .enumerate()
    {
        let mut db = common::FileDb::new().await;
        let repo = db.repo();
        let b = board(11);
        let s = stack(2, &b);
        let t = task(1, &s, BTreeMap::default());
        let rule_id = ReminderRuleId::from(uuid::Uuid::from_u128(20 + i as u128));
        let rule = ReminderRule {
            insistence: taskboard_domain::Insistence::Normal,
            id: rule_id,
            task: t.id,
            offset: ReminderOffset::before(chrono::TimeDelta::try_seconds(secs).unwrap())
                .expect("whole-second"),
            created_at: ts(1_000),
            updated_at: ts(2_000),
        };
        repo.apply(vec![
            PersistenceAction::UpsertBoard(b),
            PersistenceAction::UpsertStack(s),
            PersistenceAction::UpsertTask(t),
            PersistenceAction::UpsertReminderRule(rule.clone()),
        ])
        .await
        .expect("apply");
        let loaded = repo.load().await.expect("load");
        assert_eq!(
            loaded.reminders.rules.get(&rule_id),
            Some(&rule),
            "offset {secs} s round-trips identically"
        );
        db.close().await;
    }
}

/// Chunk-7 board-clear pin (storage-side): an alarm row and its
/// `alarm:`-keyed deliveries/snoozes ride the text codec and survive a
/// `ClearBoardContent` — the `'rule:'`/`'default:'` subqueries cannot
/// match `'alarm:'` text.
#[tokio::test]
async fn adhoc_alarms_and_their_facts_survive_a_board_clear() {
    use taskboard_domain::ids::AdhocAlarmId;
    use taskboard_domain::reminders::{ReminderDelivery, ReminderRef, ReminderSnooze};

    let mut db = common::FileDb::new().await;
    let repo = db.repo();
    let b = board(11);
    let s = stack(2, &b);
    let t = task(1, &s, BTreeMap::default());
    let alarm_id = AdhocAlarmId::from(uuid::Uuid::from_u128(30));
    let reminder = ReminderRef::Adhoc(alarm_id);
    repo.apply(vec![
        PersistenceAction::UpsertBoard(b.clone()),
        PersistenceAction::UpsertStack(s),
        PersistenceAction::UpsertTask(t),
        PersistenceAction::UpsertAdhocAlarm(taskboard_domain::reminders::AdhocAlarm {
            insistence: taskboard_domain::Insistence::Normal,
            id: alarm_id,
            label: "steep the tea".into(),
            fire_at: ts(3_000),
            created_at: ts(1_000),
        }),
        PersistenceAction::RecordReminderDelivery(ReminderDelivery {
            reminder,
            occurrence: ts(3_000),
            delivered_at: ts(3_001),
        }),
        PersistenceAction::RecordReminderSnooze(ReminderSnooze {
            reminder,
            occurrence: ts(4_000),
            rescheduled_to: ts(5_000),
        }),
    ])
    .await
    .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.reminders.alarms.len(), 1);

    repo.apply(vec![PersistenceAction::ClearBoardContent(b.id)])
        .await
        .expect("clear");
    let loaded = repo.load().await.expect("load");
    assert!(loaded.tasks.is_empty(), "board content goes");
    let alarm = loaded
        .reminders
        .alarms
        .get(&alarm_id)
        .expect("alarm survives");
    assert_eq!(alarm.label, "steep the tea");
    assert_eq!(alarm.fire_at, ts(3_000));
    assert!(
        loaded.reminders.is_delivered(&reminder, &ts(3_000)),
        "the alarm-keyed delivery survives"
    );
    assert!(
        loaded
            .reminders
            .snoozes
            .contains_key(&taskboard_domain::reminders::ReminderSnoozeKey {
                reminder,
                occurrence: ts(4_000),
            }),
        "the alarm-keyed snooze survives"
    );
    db.close().await;
}

/// P3 — upserting a modified entity overwrites every stored column.
#[tokio::test]
async fn upsert_overwrites_all_columns() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let b = board(11);
    let s = stack(2, &b);
    repo.apply(vec![
        PersistenceAction::UpsertBoard(b.clone()),
        PersistenceAction::UpsertStack(s.clone()),
    ])
    .await
    .expect("apply");

    let mut renamed = s.clone();
    renamed.title = "renamed".into();
    renamed.order = -4;
    renamed.deleted = true;
    renamed.clocks = stack_clocks(99);
    renamed.remote_seen = None;
    repo.apply(vec![PersistenceAction::UpsertStack(renamed.clone())])
        .await
        .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.stacks.get(&s.id), Some(&renamed));
}

/// The recur column round-trips a full rule (and a cleared one decodes
/// back as `None`).
#[tokio::test]
async fn recur_rule_round_trips_through_sqlite() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let b = board(11);
    let s = stack(2, &b);
    repo.apply(vec![
        PersistenceAction::UpsertBoard(b.clone()),
        PersistenceAction::UpsertStack(s.clone()),
    ])
    .await
    .expect("apply");

    let mut recurring = task(3, &s, BTreeMap::new());
    recurring.recur = Some(taskboard_domain::recur::RecurRule {
        series: taskboard_domain::SeriesId::from_uuid(uuid::Uuid::from_u128(0x5e)),
        anchor: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        pattern: taskboard_domain::recur::RecurPattern::Weekly {
            interval: taskboard_domain::recur::EveryWeeks::new(2).unwrap(),
            on: taskboard_domain::recur::WeekdaySet::new([
                chrono::Weekday::Mon,
                chrono::Weekday::Fri,
            ])
            .unwrap(),
        },
        bound: taskboard_domain::recur::RecurBound::Until(
            chrono::NaiveDate::from_ymd_opt(2027, 6, 30).unwrap(),
        ),
        continued_from: Some(taskboard_domain::HabitId::from_uuid(uuid::Uuid::from_u128(
            0xab,
        ))),
    });
    recurring.clocks.recur = ts(900);
    repo.apply(vec![PersistenceAction::UpsertTask(recurring.clone())])
        .await
        .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.tasks.get(&recurring.id), Some(&recurring));

    // Clearing the rule: an ordinary write of the NULL column.
    let mut cleared = recurring.clone();
    cleared.recur = None;
    cleared.clocks.recur = ts(901);
    repo.apply(vec![PersistenceAction::UpsertTask(cleared.clone())])
        .await
        .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.tasks.get(&cleared.id), Some(&cleared));
}

/// P4 — applying the same batch twice is idempotent (no duplicate outbox
/// rows; `op_id` upserts in place).
#[tokio::test]
async fn idempotent_reapply_leaves_same_state() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let b = board(11);
    let op = PendingOp {
        op_id: OpId(uuid::Uuid::from_u128(3)),
        op: LocalOp::CreateTask(TaskId::from(uuid::Uuid::from_u128(1))),
        queued_at: ts(7),
    };
    let batch = vec![
        PersistenceAction::UpsertBoard(b.clone()),
        PersistenceAction::EnqueueOp(op.clone()),
    ];
    repo.apply(batch.clone()).await.expect("apply once");
    repo.apply(batch).await.expect("apply twice");

    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.boards.len(), 1);
    assert_eq!(loaded.outbox.len(), 1);
    assert_eq!(loaded.outbox[0].op_id, op.op_id);
}

/// T2 (DR-4/R) — the ref-delete ops survive a close/open cycle through
/// the board column + JSON payload (the #150 collapse's doomed deletes).
#[tokio::test]
async fn ref_delete_ops_roundtrip_through_persistence() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let board_id = taskboard_domain::BoardId::from(uuid::Uuid::from_u128(2));
    let card_ref = taskboard_domain::RemoteCardRef {
        board: taskboard_domain::RemoteBoardId(42),
        stack: taskboard_domain::RemoteStackId(7),
        card: taskboard_domain::RemoteCardId(517),
    };
    let stack_ref = taskboard_domain::RemoteStackRef {
        board: taskboard_domain::RemoteBoardId(42),
        stack: taskboard_domain::RemoteStackId(8),
    };
    let batch = vec![
        PersistenceAction::EnqueueOp(PendingOp {
            op_id: OpId(uuid::Uuid::from_u128(10)),
            op: LocalOp::DeleteRemoteCard {
                board: board_id,
                card: card_ref,
            },
            queued_at: ts(7),
        }),
        PersistenceAction::EnqueueOp(PendingOp {
            op_id: OpId(uuid::Uuid::from_u128(11)),
            op: LocalOp::DeleteRemoteStack {
                board: board_id,
                stack: stack_ref,
            },
            queued_at: ts(7),
        }),
    ];
    repo.apply(batch).await.expect("enqueue");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.outbox.len(), 2);
    assert!(
        matches!(
            &loaded.outbox[0].op,
            LocalOp::DeleteRemoteCard { card, .. } if *card == card_ref
        ),
        "card ref-delete round-trips"
    );
    assert!(
        matches!(
            &loaded.outbox[1].op,
            LocalOp::DeleteRemoteStack { stack, .. } if *stack == stack_ref
        ),
        "stack ref-delete round-trips"
    );
}

/// T5 — upserting a task again must not cascade-delete its `task_labels`
/// rows (the `INSERT OR REPLACE` trap).
#[tokio::test]
async fn task_upsert_keeps_label_set() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let b = board(11);
    let s = stack(2, &b);
    let l = label(12, &b);
    let t = task(
        1,
        &s,
        BTreeMap::from([(
            l.id,
            taskboard_domain::entities::MembershipState::Added { at: ts(102) },
        )]),
    );
    let batch = vec![
        PersistenceAction::UpsertBoard(b.clone()),
        PersistenceAction::UpsertStack(s.clone()),
        PersistenceAction::UpsertLabel(l.clone()),
        PersistenceAction::UpsertTask(t.clone()),
    ];
    repo.apply(batch.clone()).await.expect("apply");

    let mut retitled = t.clone();
    retitled.title = "changed".into();
    repo.apply(vec![PersistenceAction::UpsertTask(retitled.clone())])
        .await
        .expect("apply");

    let loaded = repo.load().await.expect("load");
    let stored = loaded.tasks.get(&t.id).expect("task still present");
    assert_eq!(stored.title, "changed");
    assert_eq!(
        stored.live_labels(),
        BTreeSet::from([l.id]),
        "label set must survive the upsert"
    );

    // T5b — the no-flap storage half (#129): a remembered removal
    // survives a later upsert of the same task.
    let mut with_tombstone = retitled;
    with_tombstone.label_memberships.insert(
        l.id,
        taskboard_domain::entities::MembershipState::Removed { at: ts(103) },
    );
    repo.apply(vec![PersistenceAction::UpsertTask(with_tombstone)])
        .await
        .expect("apply");
    let reloaded = repo.load().await.expect("load");
    let stored = reloaded.tasks.get(&t.id).expect("task still present");
    assert!(!stored.live_labels().contains(&l.id));
    assert_eq!(
        stored.label_memberships.get(&l.id),
        Some(&taskboard_domain::entities::MembershipState::Removed { at: ts(103) }),
        "the tombstone row persists through the delete-then-rewrite"
    );
}

/// T6 — a mid-batch failure rolls back the whole transaction: neither the
/// entity upsert nor the enqueue survives. The failure is injected with a
/// temporary `RAISE(ABORT)` trigger on outbox inserts.
#[tokio::test]
async fn failed_batch_rolls_back_atomically() {
    let mut db = common::FileDb::new().await;
    let b = board(11);
    let s = stack(2, &b);
    db.repo()
        .apply(vec![
            PersistenceAction::UpsertBoard(b.clone()),
            PersistenceAction::UpsertStack(s.clone()),
        ])
        .await
        .expect("setup");

    // Raw second connection to the same file: installs the sabotage.
    let raw = sqlx::sqlite::SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connect");
    sqlx::query("CREATE TRIGGER sabotage BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT, 'sabotage'); END")
        .execute(&raw)
        .await
        .expect("trigger");

    let t = task(1, &s, BTreeMap::new());
    let result = db
        .repo()
        .apply(vec![
            PersistenceAction::UpsertTask(t.clone()),
            PersistenceAction::EnqueueOp(PendingOp {
                op_id: OpId(uuid::Uuid::from_u128(3)),
                op: LocalOp::CreateTask(t.id),
                queued_at: ts(9),
            }),
        ])
        .await;
    assert!(result.is_err(), "the sabotaged batch must fail");

    sqlx::query("DROP TRIGGER sabotage")
        .execute(&raw)
        .await
        .expect("drop trigger");
    raw.close().await;

    let loaded = db.repo().load().await.expect("load after rollback");
    assert!(
        !loaded.tasks.contains_key(&t.id),
        "task upsert must be rolled back"
    );
    assert!(loaded.outbox.is_empty(), "enqueue must be rolled back");
    db.close().await;
}

/// T9 — an `UpdateBoard` op is stored in the outbox `board_id` column with
/// `stack_id` NULL (structural column facts; the cast-era shape would put
/// the board id in `stack_id` and pass every semantic roundtrip).
#[tokio::test]
async fn rename_board_op_lands_in_the_board_column() {
    let board_uuid = uuid::Uuid::from_u128(0xb1);
    let mut db = common::FileDb::new().await;
    let op_id = OpId(uuid::Uuid::from_u128(0xb2));
    db.repo()
        .apply(vec![PersistenceAction::EnqueueOp(PendingOp {
            op_id,
            op: LocalOp::UpdateBoard(BoardId::from(board_uuid)),
            queued_at: ts(42),
        })])
        .await
        .expect("enqueue");

    let raw = sqlx::sqlite::SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connect");
    let (board_id, stack_id): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT board_id, stack_id FROM outbox WHERE op_id = ?")
            .bind(op_id.0.to_string())
            .fetch_one(&raw)
            .await
            .expect("outbox row");
    raw.close().await;
    assert_eq!(board_id.as_deref(), Some(board_uuid.to_string().as_str()));
    assert_eq!(stack_id, None, "the board id must not ride stack_id");

    let loaded = db.repo().load().await.expect("load");
    assert_eq!(
        loaded.outbox.iter().map(|o| o.op).collect::<Vec<_>>(),
        vec![LocalOp::UpdateBoard(BoardId::from(board_uuid))]
    );
    db.close().await;
}

/// T7 — raw-inserted corrupt rows decode to `Err(Corrupted)`, per class.
#[tokio::test]
async fn corrupt_rows_decode_to_corrupted() {
    async fn corrupt_and_load(sql: &'static str) -> Result<PersistedState, RepositoryError> {
        let mut db = common::FileDb::new().await;
        let raw = sqlx::sqlite::SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
            .await
            .expect("raw connect");
        sqlx::query(sql)
            .execute(&raw)
            .await
            .expect("corrupt insert");
        raw.close().await;
        let outcome = db.repo().load().await;
        db.close().await;
        outcome
    }

    // Unparseable uuid in an id column.
    let result = corrupt_and_load("INSERT INTO boards (id, title, color, archived, deleted) VALUES ('not-a-uuid', 'x', 'x', 0, 0)").await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Unknown op_kind tag.
    let result = corrupt_and_load(
        "INSERT INTO outbox (op_id, op_kind, queued_at) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000001', 'teleport_task', '2026-01-01T00:00:00+00:00')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Missing required id column for the op kind.
    let result = corrupt_and_load(
        "INSERT INTO outbox (op_id, op_kind, queued_at) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000002', 'create_task', '2026-01-01T00:00:00+00:00')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // rename_board without its board column (the cast-era shape — the
    // board id riding stack_id — is now a rejection, not a silent cast).
    let result = corrupt_and_load(
        "INSERT INTO outbox (op_id, op_kind, queued_at) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000008', 'rename_board', '2026-01-01T00:00:00+00:00')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Unknown sync phase tag (on a row whose board exists — the FK).
    let result = corrupt_and_load(
        "INSERT INTO boards (id, title, color, archived, deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000009', 'x', 'x', 0, 0);
         INSERT INTO sync_status (board, phase) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000009', 'warp')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Failed phase without its error kind.
    let result = corrupt_and_load(
        "INSERT INTO boards (id, title, color, archived, deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-00000000000a', 'x', 'x', 0, 0);
         INSERT INTO sync_status (board, phase) VALUES ('0197c0ff-eeee-7ccc-8ddd-00000000000a', 'failed')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Note: a malformed board uuid in the status key cannot be inserted
    // past the FK (the key must match a boards row, whose id decodes),
    // so the key-decode guard is defensive only — no SQL-reachable pin.

    // Partial remote binding (board column set, stack column NULL).
    let result = corrupt_and_load(
        "INSERT INTO boards (id, title, color, archived, deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000003', 'x', 'x', 0, 0);
         INSERT INTO stacks (id, board, remote_board_id, remote_stack_id, title, sort_order, archived, deleted, ck_title, ck_sort_order, ck_deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000004', '0197c0ff-eeee-7ccc-8ddd-000000000003', 5, NULL, 's', 0, 0, 0, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Unparseable clock text in a task's ck_ column.
    let result = corrupt_and_load(
        "INSERT INTO boards (id, title, color, archived, deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000005', 'x', 'x', 0, 0);
         INSERT INTO stacks (id, board, title, sort_order, archived, deleted, ck_title, ck_sort_order, ck_deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000006', '0197c0ff-eeee-7ccc-8ddd-000000000005', 's', 0, 0, 0, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00');
         INSERT INTO tasks (id, stack, title, description, sort_order, archived, deleted, ck_title, ck_description, ck_duedate, ck_done, ck_position, ck_labels, ck_archived, ck_deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-000000000007', '0197c0ff-eeee-7ccc-8ddd-000000000006', 't', '', 0, 0, 0, 'junk', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // Positive reminder-rule offset (the mint rejects it; never a
    // clamped value).
    let result = corrupt_and_load(
        "INSERT INTO boards (id, title, color, archived, deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-00000000000b', 'x', 'x', 0, 0);
         INSERT INTO stacks (id, board, title, sort_order, archived, deleted, ck_title, ck_sort_order, ck_deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-00000000000c', '0197c0ff-eeee-7ccc-8ddd-00000000000b', 's', 0, 0, 0, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00');
         INSERT INTO tasks (id, stack, title, description, sort_order, archived, deleted, ck_title, ck_description, ck_duedate, ck_done, ck_position, ck_labels, ck_archived, ck_deleted) VALUES ('0197c0ff-eeee-7ccc-8ddd-00000000000d', '0197c0ff-eeee-7ccc-8ddd-00000000000c', 't', '', 0, 0, 0, '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00');
         INSERT INTO reminder_rules (rule_id, task_id, offset_seconds, created_at) VALUES ('0197c0ff-eeee-7ccc-8ddd-00000000000e', '0197c0ff-eeee-7ccc-8ddd-00000000000d', 60, '2026-01-01T00:00:00+00:00')",
    )
    .await;
    assert!(matches!(result, Err(RepositoryError::Corrupted)));

    // The singleton row is gone (per-board rebuild; zero rows is the
    // healthy fresh-install shape, so only a *malformed* key corrupts).
}

/// T8 — state survives a close/reopen cycle on a file database (and the
/// second open runs zero pending migrations).
#[tokio::test]
async fn state_survives_reopen() {
    let mut db = common::FileDb::new().await;
    let board_id = board(11).id;
    let mut sync = SyncStates::new();
    sync.set_stored(
        board_id,
        SyncPhase::Failed {
            last_error: SyncErrorKind::Network,
        },
        Some(ts(123)),
    );
    let state = PersistedState {
        boards: BTreeMap::from([(board_id, board(11))]),
        sync,
        ..PersistedState::default()
    };
    db.repo()
        .apply(actions_from_state(&state))
        .await
        .expect("apply");
    let expected = db.repo().load().await.expect("load");
    db.reopen().await;

    let loaded = db.repo().load().await.expect("load");
    assert_eq!(loaded, expected);
    db.close().await;
}

/// Validators for a `stacks:<board>` key round-trip (the key form that the
/// contract harness does not cover).
#[tokio::test]
async fn per_board_validators_roundtrip() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let key = ValidatorKey::Stacks(RemoteBoardId(55));
    let validators = SyncValidators {
        etag: Some("\"etag\"".into()),
        last_modified: Some("Tue, 01 Jan 2026 00:00:00 GMT".into()),
    };
    repo.apply(vec![PersistenceAction::UpsertValidators(
        key,
        validators.clone(),
    )])
    .await
    .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.validators.get(&key), Some(&validators));
}

/// `DeleteSyncStatus` drops exactly the named board's row; the other
/// board's stored phase/last-success survives (the unbind cleanup).
#[tokio::test]
async fn delete_sync_status_roundtrip() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let first = board(71);
    let second = board(72);
    let status = |phase: SyncPhase| taskboard_domain::SyncStatus {
        phase,
        last_success: Some(ts(400)),
        pending_ops: 0, // derived, never stored
        counters: taskboard_domain::SyncCounters::default(),
        ..taskboard_domain::SyncStatus::default()
    };
    repo.apply(vec![
        PersistenceAction::UpsertBoard(first.clone()),
        PersistenceAction::UpsertBoard(second.clone()),
        PersistenceAction::UpsertSyncStatus(first.id, status(SyncPhase::Syncing)),
        PersistenceAction::UpsertSyncStatus(second.id, status(SyncPhase::Idle)),
    ])
    .await
    .expect("seed boards and status rows");

    repo.apply(vec![PersistenceAction::DeleteSyncStatus(first.id)])
        .await
        .expect("delete one row");
    let loaded = repo.load().await.expect("load");
    // The projection re-derives a default row for every board row; the
    // *stored* part (phase, last success) is what the delete dropped.
    let deleted_view = loaded.sync.get(first.id).expect("derived row present");
    assert_eq!(deleted_view.phase, SyncPhase::Idle);
    assert_eq!(deleted_view.last_success, None);
    let survivor = loaded.sync.get(second.id).expect("the other board's row");
    assert_eq!(survivor.phase, SyncPhase::Idle);
    assert_eq!(survivor.last_success, Some(ts(400)));

    // Absent row = no-op (idempotent repeat).
    repo.apply(vec![PersistenceAction::DeleteSyncStatus(first.id)])
        .await
        .expect("repeat delete is a no-op");
}

// S1 — the baseline codec round-trip: a fully-populated record survives
// apply → load byte-identically; `None` round-trips as `None`.
#[tokio::test]
async fn remote_baseline_roundtrips_through_sqlite() {
    let mut db = common::FileDb::new().await;
    let b = board(71);
    let s = stack(81, &b);
    let labels = BTreeMap::from([(
        LabelId::from(uuid::Uuid::from_u128(61)),
        taskboard_domain::entities::MembershipState::Added { at: ts(102) },
    )]);
    let seed_label = label(61, &b);
    let baseline = taskboard_domain::entities::RemoteTaskBaseline {
        title: "remote title".into(),
        description: "remote desc".into(),
        duedate: Some(ts(300)),
        scheduled: Some(ts(310)),
        done: Some(ts(320)),
        stack: RemoteStackId(7),
        order: 4,
        labels: BTreeSet::from([RemoteLabelId(9), RemoteLabelId(11)]),
        archived: true,
    };

    let mut with_baseline = task(90, &s, labels.clone());
    with_baseline.remote_baseline = Some(baseline.clone());
    let none = task(91, &s, labels);
    db.repo()
        .apply(vec![
            PersistenceAction::UpsertBoard(b),
            PersistenceAction::UpsertStack(s),
            PersistenceAction::UpsertLabel(seed_label),
            PersistenceAction::UpsertTask(with_baseline),
            PersistenceAction::UpsertTask(none),
        ])
        .await
        .expect("apply");

    let loaded = db.repo().load().await.expect("load");
    assert_eq!(
        loaded
            .tasks
            .get(&TaskId::from(uuid::Uuid::from_u128(90)))
            .expect("loaded")
            .remote_baseline
            .as_ref()
            .expect("populated record survives"),
        &baseline
    );
    assert_eq!(
        loaded
            .tasks
            .get(&TaskId::from(uuid::Uuid::from_u128(91)))
            .expect("loaded")
            .remote_baseline,
        None
    );
    db.close().await;
}

// S2 — a stored baseline that fails to decode is typed corruption, never
// a silent downgrade to the fallback (D-7: the fallback would blanket-
// clobber on the next merge).
#[tokio::test]
async fn corrupt_baseline_json_is_typed_corruption() {
    let mut db = common::FileDb::new().await;
    let b = board(71);
    let s = stack(81, &b);
    let mut seeded = task(90, &s, BTreeMap::new());
    seeded.remote_baseline = Some(taskboard_domain::entities::RemoteTaskBaseline {
        title: "t".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack: RemoteStackId(7),
        order: 1,
        labels: BTreeSet::new(),
        archived: false,
    });
    db.repo()
        .apply(vec![
            PersistenceAction::UpsertBoard(b),
            PersistenceAction::UpsertStack(s),
            PersistenceAction::UpsertTask(seeded),
        ])
        .await
        .expect("seed");
    db.close().await;

    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connection");
    sqlx::query(r"UPDATE tasks SET remote_baseline = '{not json}' WHERE id = '00000000-0000-0000-0000-00000000005a'")
        .execute(&pool)
        .await
        .expect("raw update");
    pool.close().await;
    db.reopen().await;

    let err = db.repo().load().await.expect_err("load must fail");
    assert_eq!(err, RepositoryError::Corrupted);
    db.close().await;
}

// S12 — the memberships upgrade pin (DR-4/L, #129): raw pre-0012-shape
// task_labels rows (no state column) decode as fresh adds at the epoch
// stamp — the pre-migration live set bit-for-bit — and the loaded task
// merges a pull to the same live labels as a hand-built fresh-add task.
#[tokio::test]
#[allow(clippy::too_many_lines)] // raw DDL + the merge-equivalence leg
async fn pre_membership_rows_decode_as_fresh_adds_at_the_epoch() {
    let mut db = common::FileDb::new().await;
    let b = board(71);
    let s = stack(81, &b);
    let l = label(61, &b);
    db.repo()
        .apply(vec![
            PersistenceAction::UpsertBoard(b),
            PersistenceAction::UpsertStack(s.clone()),
            PersistenceAction::UpsertLabel(l.clone()),
        ])
        .await
        .expect("parents");
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connection");
    // Raw pre-0012-shape INSERT: a task and two task_labels rows with
    // only the (task, label) columns the pre-migration schema had.
    sqlx::query(
        r"INSERT INTO tasks (id, stack, title, description, sort_order, archived, deleted,
                             ck_title, ck_description, ck_duedate, ck_done, ck_position,
                             ck_labels, ck_archived, ck_deleted)
           VALUES ('00000000-0000-0000-0000-0000000000bb', '00000000-0000-0000-0000-000000000051',
                   'pre-memberships', 'd', 1, 0, 0,
                   '2020-01-01T00:00:00+00:00', '2020-01-01T00:00:00+00:00',
                   '2020-01-01T00:00:00+00:00', '2020-01-01T00:00:00+00:00',
                   '2020-01-01T00:00:00+00:00', '2020-01-01T00:00:00+00:00',
                   '2020-01-01T00:00:00+00:00', '2020-01-01T00:00:00+00:00')",
    )
    .execute(&pool)
    .await
    .expect("raw insert");
    sqlx::query(
        r"INSERT INTO task_labels (task, label) VALUES
              ('00000000-0000-0000-0000-0000000000bb', '00000000-0000-0000-0000-00000000003d')",
    )
    .execute(&pool)
    .await
    .expect("raw join insert");
    pool.close().await;

    let loaded = db.repo().load().await.expect("load");
    let upgraded = loaded
        .tasks
        .get(&TaskId::from(uuid::Uuid::from_u128(0xbb)))
        .expect("task loaded")
        .clone();
    let label_id = LabelId::from(uuid::Uuid::from_u128(0x3d));
    assert_eq!(
        upgraded.live_labels(),
        BTreeSet::from([label_id]),
        "the pre-migration live set is preserved bit-for-bit"
    );
    assert_eq!(
        upgraded.label_memberships.get(&label_id),
        Some(&taskboard_domain::entities::MembershipState::Added {
            at: taskboard_domain::entities::BoardClocks::epoch()
        }),
        "backfilled as a fresh add at the epoch stamp"
    );
    db.close().await;

    // Merge equivalence: the upgraded task and a hand-built fresh-add
    // task produce the same live labels from the same pull.
    let remote = taskboard_domain::RemoteTask {
        id: taskboard_domain::RemoteCardRef {
            board: RemoteBoardId(77),
            stack: RemoteStackId(7),
            card: RemoteCardId(8),
        },
        title: "pre-memberships".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack: RemoteStackId(7),
        order: 1,
        labels: BTreeSet::from([taskboard_domain::RemoteLabelId(9)]),
        archived: false,
        last_modified: ts(1_800_000_500),
        created_at: ts(1_800_000_500),
    };
    let mut ctx = taskboard_domain::RemoteIndex::default();
    ctx.stack_by_ref.insert(
        taskboard_domain::RemoteStackRef {
            board: RemoteBoardId(77),
            stack: RemoteStackId(7),
        },
        s.id,
    );
    ctx.label_by_ref.insert(
        taskboard_domain::RemoteLabelRef {
            board: RemoteBoardId(77),
            label: taskboard_domain::RemoteLabelId(9),
        },
        label_id,
    );
    let merged_upgraded = taskboard_domain::merge::merge_task(&upgraded, &remote, &ctx, true);
    let mut hand_built = upgraded.clone();
    hand_built.label_memberships.insert(
        label_id,
        taskboard_domain::entities::MembershipState::Added {
            at: taskboard_domain::entities::BoardClocks::epoch(),
        },
    );
    let merged_hand = taskboard_domain::merge::merge_task(&hand_built, &remote, &ctx, true);
    assert_eq!(
        merged_upgraded.live_labels(),
        merged_hand.live_labels(),
        "the epoch backfill merges like a hand-built fresh add"
    );
}

// S3 — the upgrade pin: a raw pre-0011-shape row decodes to a `None`
// baseline and merges under the fallback exactly like a hand-built
// baseline-less task (same clobber as today on the first cycle).
#[tokio::test]
async fn pre_baseline_row_decodes_to_none_and_merges_under_the_fallback() {
    let mut db = common::FileDb::new().await;
    let b = board(71);
    let s = stack(81, &b);
    db.repo()
        .apply(vec![
            PersistenceAction::UpsertBoard(b),
            PersistenceAction::UpsertStack(s),
        ])
        .await
        .expect("parents");
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db.path().display()))
        .await
        .expect("raw connection");
    // Raw pre-0011-shape INSERT (the schema.rs upgrade-test precedent):
    // the ids are the seeded parents' text forms.
    sqlx::query(
        r"INSERT INTO tasks (id, stack, title, description, sort_order, archived, deleted,
                             ck_title, ck_description, ck_duedate, ck_done, ck_position,
                             ck_labels, ck_archived, ck_deleted)
           VALUES ('00000000-0000-0000-0000-0000000000ba', '00000000-0000-0000-0000-000000000051',
                   'pre-baseline', 'd', 1, 0, 0,
                   '2020-01-01T00:00:00+00:00', '2020-01-01T00:00:00+00:00',
                   '2020-01-01T00:00:00+00:00', '2020-01-01T00:00:00+00:00',
                   '2020-01-01T00:00:00+00:00', '2020-01-01T00:00:00+00:00',
                   '2020-01-01T00:00:00+00:00', '2020-01-01T00:00:00+00:00')",
    )
    .execute(&pool)
    .await
    .expect("raw insert");
    pool.close().await;
    let loaded = db.repo().load().await.expect("load");
    let upgraded = loaded
        .tasks
        .get(&TaskId::from(uuid::Uuid::from_u128(0xba)))
        .expect("task loaded")
        .clone();
    assert_eq!(upgraded.remote_baseline, None);
    db.close().await;

    // The decoded task merges identically to a baseline-less one: the
    // unchanged remote title clobbers on the first (fallback) cycle.
    let mut local = upgraded.clone();
    local.title = "edited locally".into();
    local.clocks.title = ts(1_800_000_400);
    let remote = taskboard_domain::RemoteTask {
        id: taskboard_domain::RemoteCardRef {
            board: RemoteBoardId(77),
            stack: RemoteStackId(7),
            card: RemoteCardId(8),
        },
        title: "pre-baseline".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack: RemoteStackId(7),
        order: 1,
        labels: BTreeSet::new(),
        archived: false,
        last_modified: ts(1_800_000_500),
        created_at: ts(1_800_000_500),
    };
    let ctx = taskboard_domain::RemoteIndex::default();
    let first = taskboard_domain::merge::merge_task(&local, &remote, &ctx, true);
    assert_eq!(first.title, remote.title, "fallback clobbers, bit-for-bit");
    assert_eq!(
        first.remote_baseline,
        Some(taskboard_domain::entities::RemoteTaskBaseline::of(&remote))
    );
    // ...and the second cycle is a fixpoint.
    let second = taskboard_domain::merge::merge_task(&first, &remote, &ctx, true);
    assert_eq!(second, first);
}

// ---- Insistence storage (DR-3/2, #475, migration 0013) ----------------

/// The insistence upgrade pin: raw pre-0013-shape rows in both tables
/// (no `insistence` column) decode as `Normal` — existing rows fold
/// `normal`, the pre-migration behavior bit-for-bit.
#[tokio::test]
async fn pre_insistence_rows_decode_normal() {
    use taskboard_domain::ids::{AdhocAlarmId, ReminderRuleId};
    use taskboard_domain::reminders::Insistence;

    let mut db = common::FileDb::new().await;
    let repo = db.repo();
    let b = board(11);
    let s = stack(2, &b);
    let t = task(1, &s, BTreeMap::default());
    repo.apply(vec![
        PersistenceAction::UpsertBoard(b),
        PersistenceAction::UpsertStack(s),
        PersistenceAction::UpsertTask(t),
    ])
    .await
    .expect("parents");
    let raw = sqlx::sqlite::SqliteConnectOptions::new().filename(db.path());
    let pool = sqlx::SqlitePool::connect_with(raw).await.expect("raw pool");
    sqlx::query(
        "INSERT INTO reminder_rules (rule_id, task_id, offset_seconds, created_at, updated_at) \
         VALUES ('00000000-0000-0000-0000-000000000021', \
                 '00000000-0000-0000-0000-000000000001', -60, \
                 '1970-01-01T00:00:16+00:00', '1970-01-01T00:00:17+00:00')",
    )
    .execute(&pool)
    .await
    .expect("raw rule insert");
    sqlx::query(
        "INSERT INTO adhoc_alarms (alarm_id, label, fire_at, created_at) \
         VALUES ('00000000-0000-0000-0000-00000000002a', 'tea', \
                 '1970-01-01T00:05:00+00:00', '1970-01-01T00:00:16+00:00')",
    )
    .execute(&pool)
    .await
    .expect("raw alarm insert");
    pool.close().await;

    let loaded = db.repo().load().await.expect("load");
    let rule = loaded.reminders.rules[&ReminderRuleId::from(uuid::Uuid::from_u128(0x21))].clone();
    let alarm = loaded.reminders.alarms[&AdhocAlarmId::from(uuid::Uuid::from_u128(0x2a))].clone();
    assert_eq!(rule.insistence, Insistence::Normal, "rules fold normal");
    assert_eq!(alarm.insistence, Insistence::Normal, "alarms fold normal");
    db.close().await;
}

/// An `Alarm` rung survives write/reload identically on both tables
/// (the column stores the domain serde unit name).
#[tokio::test]
async fn insistence_round_trips_through_sqlite() {
    use taskboard_domain::ids::{AdhocAlarmId, ReminderRuleId};
    use taskboard_domain::reminders::{AdhocAlarm, Insistence, ReminderOffset, ReminderRule};

    let mut db = common::FileDb::new().await;
    let repo = db.repo();
    let b = board(11);
    let s = stack(2, &b);
    let t = task(1, &s, BTreeMap::default());
    let rule = ReminderRule {
        id: ReminderRuleId::from(uuid::Uuid::from_u128(20)),
        task: t.id,
        offset: ReminderOffset::before(chrono::TimeDelta::try_minutes(-5).unwrap())
            .expect("valid offset"),
        insistence: Insistence::Alarm,
        created_at: ts(1_000),
        updated_at: ts(2_000),
    };
    let alarm = AdhocAlarm {
        id: AdhocAlarmId::from(uuid::Uuid::from_u128(30)),
        label: "wake".into(),
        fire_at: ts(3_000),
        insistence: Insistence::Alarm,
        created_at: ts(1_000),
    };
    repo.apply(vec![
        PersistenceAction::UpsertBoard(b),
        PersistenceAction::UpsertStack(s),
        PersistenceAction::UpsertTask(t),
        PersistenceAction::UpsertReminderRule(rule.clone()),
        PersistenceAction::UpsertAdhocAlarm(alarm.clone()),
    ])
    .await
    .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.reminders.rules[&rule.id], rule);
    assert_eq!(loaded.reminders.alarms[&alarm.id], alarm);
    db.close().await;
}

/// Unknown insistence text in the db is a typed corruption, never a
/// defaulted decode (the codec's one-representation discipline).
#[tokio::test]
async fn unknown_insistence_text_is_a_typed_corruption() {
    use taskboard_domain::ids::ReminderRuleId;
    use taskboard_domain::persistence::RepositoryError;
    use taskboard_domain::reminders::{ReminderOffset, ReminderRule};

    let mut db = common::FileDb::new().await;
    let repo = db.repo();
    let b = board(11);
    let s = stack(2, &b);
    let t = task(1, &s, BTreeMap::default());
    let rule = ReminderRule {
        id: ReminderRuleId::from(uuid::Uuid::from_u128(20)),
        task: t.id,
        offset: ReminderOffset::before(chrono::TimeDelta::try_minutes(-5).unwrap())
            .expect("valid offset"),
        insistence: taskboard_domain::Insistence::Normal,
        created_at: ts(1_000),
        updated_at: ts(2_000),
    };
    repo.apply(vec![
        PersistenceAction::UpsertBoard(b),
        PersistenceAction::UpsertStack(s),
        PersistenceAction::UpsertTask(t),
        PersistenceAction::UpsertReminderRule(rule),
    ])
    .await
    .expect("apply");
    let raw = sqlx::sqlite::SqliteConnectOptions::new().filename(db.path());
    let pool = sqlx::SqlitePool::connect_with(raw).await.expect("raw pool");
    sqlx::query("UPDATE reminder_rules SET insistence = 'wake_up'")
        .execute(&pool)
        .await
        .expect("corrupting update");
    pool.close().await;

    let err = db.repo().load().await.expect_err("corrupt decode");
    assert!(matches!(err, RepositoryError::Corrupted));
    db.close().await;
}

// ---- Time-entry storage (DR-5/2, #487, migration 0015) -----------------

fn time_entry(id: u128, task: &Task, open: bool) -> taskboard_domain::TimeEntry {
    taskboard_domain::TimeEntry {
        id: taskboard_domain::TimeEntryId::from(uuid::Uuid::from_u128(id)),
        task: task.id,
        task_title: task.title.clone(),
        start: ts(1_000),
        end: open.then_some(ts(2_000)),
        created_at: ts(999),
        updated_at: ts(2_500),
    }
}

/// The load codec (D-T9): a closed and an open (`end` NULL) entry
/// round-trip through apply → load as typed-equal rows.
#[tokio::test]
async fn time_entry_rows_round_trip_including_the_open_row() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let b = board(11);
    let s = stack(2, &b);
    let t = task(1, &s, BTreeMap::default());
    let closed = time_entry(20, &t, true);
    let open = time_entry(21, &t, false);
    repo.apply(vec![
        PersistenceAction::UpsertBoard(b),
        PersistenceAction::UpsertStack(s),
        PersistenceAction::UpsertTask(t),
        PersistenceAction::UpsertTimeEntry(closed.clone()),
        PersistenceAction::UpsertTimeEntry(open.clone()),
    ])
    .await
    .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert_eq!(loaded.tracking.entries.get(&closed.id), Some(&closed));
    assert_eq!(
        loaded.tracking.entries.get(&open.id),
        Some(&open),
        "the open row's end column reads back None"
    );
    repo.close().await;
}

/// The batch contract's last-write-wins pin: upsert and delete for one
/// entry id in a single apply batch lands the delete (the statements run
/// in the fixed partition order, mirroring `apply_actions`' sequence).
#[tokio::test]
async fn a_batch_carrying_upsert_then_delete_for_one_entry_lands_the_delete() {
    let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
        .await
        .expect("open");
    let b = board(11);
    let s = stack(2, &b);
    let t = task(1, &s, BTreeMap::default());
    let entry = time_entry(22, &t, true);
    repo.apply(vec![
        PersistenceAction::UpsertBoard(b),
        PersistenceAction::UpsertStack(s),
        PersistenceAction::UpsertTask(t),
        PersistenceAction::UpsertTimeEntry(entry.clone()),
        PersistenceAction::DeleteTimeEntry(entry.id),
    ])
    .await
    .expect("apply");
    let loaded = repo.load().await.expect("load");
    assert!(
        !loaded.tracking.entries.contains_key(&entry.id),
        "the delete after the upsert wins"
    );
    repo.close().await;
}
