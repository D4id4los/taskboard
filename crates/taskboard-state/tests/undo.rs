// SPDX-License-Identifier: MIT OR Apache-2.0
//! The engine-level undo suite (ADR 0019, plan §14.7): end-to-end
//! through `EngineHandle::execute`, the R-U9 post-pass at report
//! ingestion, and the storage-failure arm. Routing only — the semantics
//! stay in the domain suite; `tokio::time::pause`/fixed clocks, typed
//! outcomes, no sleeps.

use std::sync::Arc;

use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    AppState, CommandOutcome, LocalOp, NotifierMessage, PersistedState, PushOutcome, PushResult,
    RemoteBoard, RemoteBoardSnapshot, StateCommand, SyncCommand, SyncReport, SystemEvent,
    TaskRepository, persistence::RepositoryError,
};
use taskboard_state::{DayRollConfig, ExecuteError, ReminderEngineConfig, spawn_state_engine};

mod common;

use common::{FixedClock, T0, seed_bound_board_state};

/// A repo whose `apply` always fails (the storage-failure arm's seam):
/// a handwritten fake, `InMemoryRepository`'s shape with a tripwire.
#[derive(Debug, Default)]
struct FailingRepository {
    inner: InMemoryRepository,
    fail_applies: std::sync::atomic::AtomicBool,
}

impl FailingRepository {
    fn trip(&self, failing: bool) {
        self.fail_applies
            .store(failing, std::sync::atomic::Ordering::SeqCst);
    }
}

impl TaskRepository for FailingRepository {
    fn load(&self) -> taskboard_domain::BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        self.inner.load()
    }

    fn apply(
        &self,
        actions: Vec<taskboard_domain::PersistenceAction>,
    ) -> taskboard_domain::BoxFuture<'_, Result<(), RepositoryError>> {
        let failing = self.fail_applies.load(std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move {
            if failing {
                return Err(RepositoryError::Unavailable);
            }
            TaskRepository::apply(&self.inner, actions).await
        })
    }
}

impl taskboard_domain::SyncStateReader for FailingRepository {
    fn read_state(
        &self,
    ) -> taskboard_domain::BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        self.inner.read_state()
    }
}

/// The running engine over a bound board seed (the undo line's e2e
/// harness). Fixed-clock engine; the logical clock steps per command.
struct Rig {
    handle: taskboard_state::EngineHandle,
    repo: Arc<InMemoryRepository>,
    report_tx: tokio::sync::mpsc::Sender<SyncReport>,
    clock: Arc<FixedClock>,
    _system_tx: tokio::sync::broadcast::Sender<SystemEvent>,
}

impl Rig {
    async fn start() -> Self {
        let repo = Arc::new(InMemoryRepository::with_state(seed_bound_board_state(
            "kiosk",
        )));
        let clock = FixedClock::new(T0);
        let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
        let (report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
        let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
        let (notifier, _notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
        let (handle, _join) = spawn_state_engine(
            repo.clone(),
            Arc::new(CountingIds::new()),
            clock.clone(),
            sync_out,
            report_rx,
            system_tx.clone(),
            system_rx,
            notifier,
            ReminderEngineConfig::disabled(),
            DayRollConfig::disabled(),
        )
        .await
        .expect("engine boots");
        Self {
            handle,
            repo,
            report_tx,
            clock,
            _system_tx: system_tx,
        }
    }

    async fn run(&mut self, command: StateCommand) -> Result<CommandOutcome, ExecuteError> {
        self.clock.advance(60);
        self.handle.execute(command).await
    }

    fn state(&self) -> Arc<AppState> {
        self.handle.shared_state().load_full()
    }
}

#[tokio::test]
async fn an_undo_flips_the_published_state_and_the_persisted_journal() {
    let mut rig = Rig::start().await;
    let board = rig
        .state()
        .boards
        .keys()
        .next()
        .copied()
        .expect("seed board");
    let outcome = rig
        .run(StateCommand::CreateStack {
            board,
            title: "stack".into(),
            order: 1,
        })
        .await
        .expect("accepted");
    let stack = match outcome {
        CommandOutcome::CreatedStack(id) => id,
        other => panic!("expected CreatedStack, got {other:?}"),
    };
    let outcome = rig
        .run(StateCommand::CreateTask {
            title: "t".into(),
            stack,
            order: 1,
        })
        .await
        .expect("accepted");
    let task = match outcome {
        CommandOutcome::CreatedTask(id) => id,
        other => panic!("expected CreatedTask, got {other:?}"),
    };
    // Undo: the published projection and the persisted journal agree.
    let outcome = rig.run(StateCommand::Undo).await.expect("accepted");
    assert!(matches!(outcome, CommandOutcome::Undone { .. }));
    let app = rig.state();
    assert!(app.tasks[&task].deleted, "the create undone");
    assert!(app.undo.can_undo, "the line still has the stack create");
    assert!(app.undo.can_redo);
    let persisted = rig.repo.snapshot();
    assert!(persisted.tasks[&task].deleted, "memory ≡ disk");
    assert_eq!(
        persisted.journal.head,
        app.undo.recent.last().map(|label| label.id),
        "the head sits on the oldest entry"
    );
    // Redo through the engine. Redo re-plans the *forward intent*
    // (D3): a create re-mints a fresh task — the original id stays a
    // tombstone, a new live task carries the intent.
    rig.run(StateCommand::Redo).await.expect("accepted");
    let app = rig.state();
    assert!(app.tasks[&task].deleted, "the old id stays retired");
    assert_eq!(
        app.tasks.values().filter(|t| !t.deleted).count(),
        1,
        "the re-planned create minted a fresh live task"
    );
}

#[tokio::test]
async fn a_not_undoable_head_refuses_typed_through_the_engine() {
    let mut rig = Rig::start().await;
    let board = rig
        .state()
        .boards
        .keys()
        .next()
        .copied()
        .expect("seed board");
    // The empty line first.
    let err = rig.run(StateCommand::Undo).await.expect_err("typed");
    assert!(matches!(
        err,
        ExecuteError::Rejected(taskboard_domain::CommandError::NothingToUndo)
    ));
    // A board ceremony journals NotUndoable.
    rig.run(StateCommand::RenameBoard {
        board,
        title: "renamed".into(),
    })
    .await
    .expect("accepted");
    let err = rig.run(StateCommand::Undo).await.expect_err("typed");
    assert!(matches!(
        err,
        ExecuteError::Rejected(taskboard_domain::CommandError::TargetNotUndoable(_))
    ));
    // The head never moved: the refusal is repeatable.
    let err = rig.run(StateCommand::Undo).await.expect_err("typed");
    assert!(matches!(
        err,
        ExecuteError::Rejected(taskboard_domain::CommandError::TargetNotUndoable(_))
    ));
}

#[tokio::test]
async fn a_storage_failure_leaves_the_journal_and_head_untouched() {
    let repo = Arc::new(FailingRepository::default());
    repo.trip(true);
    let clock = FixedClock::new(T0);
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (_report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
    let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    let (notifier, _notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
    let (handle, _join) = spawn_state_engine(
        repo.clone(),
        Arc::new(CountingIds::new()),
        clock.clone(),
        sync_out,
        report_rx,
        system_tx,
        system_rx,
        notifier,
        ReminderEngineConfig::disabled(),
        DayRollConfig::disabled(),
    )
    .await
    .expect("engine boots");
    clock.advance(60);
    let err = handle
        .execute(StateCommand::CreateBoard {
            title: "b".into(),
            color: taskboard_domain::Color::new("0000ff"),
        })
        .await
        .expect_err("storage failed");
    assert!(matches!(err, ExecuteError::Storage(_)));
    // Nothing journaled, no head: the failed batch never advanced
    // memory, so the journal cannot have moved either.
    let app = handle.shared_state().load_full();
    assert!(app.boards.is_empty());
    assert!(!app.undo.can_undo);
}

#[tokio::test]
async fn report_ingestion_flips_the_pushed_flag_at_the_boundary() {
    let mut rig = Rig::start().await;
    let outcome = rig
        .run(StateCommand::CreateStack {
            board: rig
                .state()
                .boards
                .keys()
                .next()
                .copied()
                .expect("seed board"),
            title: "stack".into(),
            order: 1,
        })
        .await
        .expect("accepted");
    let stack = match outcome {
        CommandOutcome::CreatedStack(id) => id,
        other => panic!("expected CreatedStack, got {other:?}"),
    };
    // The journal carries the stack create with its pending op.
    let (op_id, entry_id) = {
        let persisted = rig.repo.snapshot();
        let entry = persisted.journal.entries.last().expect("journaled");
        assert!(!entry.pushed);
        (entry.ops[0], entry.id)
    };
    // A cycle completes the push: the R-U9 post-pass must append the
    // flag flip to the applied batch (both arms land here — `Completed`
    // with pushes; `Failed`-with-pushes shares the one call site).
    let report = SyncReport::Completed {
        connector: taskboard_domain::test_support::test_connector(),
        snapshot: Box::new(RemoteBoardSnapshot {
            board: RemoteBoard {
                id: taskboard_domain::RemoteBoardId(42),
                title: "kiosk".into(),
                color: "0000ff".into(),
                archived: false,
                deleted_at: None,
                last_modified: common::ts(T0 + 120),
            },
            stacks: vec![],
            tasks: vec![],
            labels: vec![],
        }),
        validators: common::test_validators(),
        pushes: vec![PushOutcome {
            op: op_id,
            result: PushResult::Applied { echo: None },
        }],
        read_at: common::ts(T0 + 120),
        duration: std::time::Duration::from_millis(5),
        app_data: taskboard_domain::AppDataEvidence::Absent,
        features: taskboard_domain::DeckFeatures::CONSERVATIVE,
    };
    rig.report_tx.send(report).await.expect("report lands");
    // Poll the predicate (deterministic-by-construction: the sequential
    // loop lands the report before the channel send resolves).
    for _ in 0..100 {
        if rig
            .repo
            .snapshot()
            .journal
            .entry(entry_id)
            .is_some_and(|entry| entry.pushed)
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        rig.repo
            .snapshot()
            .journal
            .entry(entry_id)
            .expect("entry")
            .pushed,
        "the completed push flipped the entry at the boundary"
    );
    // The flipped entry now undoes as a new edit (fresh op enqueued).
    let outbox_before = rig.repo.snapshot().outbox.len();
    rig.run(StateCommand::Undo).await.expect("accepted");
    let persisted = rig.repo.snapshot();
    assert_eq!(
        persisted.outbox.len(),
        outbox_before + 1,
        "a pushed undo enqueues, never retracts"
    );
    let _ = LocalOp::CreateStack(stack); // the op family under test
}

/// Fact 16's DR-6/6 verification, against undo/1's landed ruling: the
/// habit surface is deliberately **not undoable**
/// (`NotUndoableReason::HabitSurface`), so there is no op retraction to
/// mirror. The pin is that the flush trigger still rides the journal
/// entry generically (the pushed-boundary machinery stays op-id-aware
/// of it) while `Undo` refuses the surface typed — the stage-1 plan's
/// "undo retracts/keeps the pending flush op exactly as for task ops"
/// premise mis-stated undo/1's catalogue and is recorded as a deviation.
#[tokio::test]
async fn a_habit_log_journals_its_flush_trigger_but_stays_not_undoable() {
    let mut rig = Rig::start().await;
    let board = rig
        .state()
        .boards
        .keys()
        .next()
        .copied()
        .expect("seed board");
    let outcome = rig
        .run(StateCommand::CreateHabit {
            board,
            title: "water".into(),
            target: taskboard_domain::HabitTarget::PerDay(
                taskboard_domain::NonZeroCount::new(3).unwrap(),
            ),
        })
        .await
        .expect("accepted");
    let habit = match outcome {
        CommandOutcome::HabitCreated(id) => id,
        other => panic!("expected HabitCreated, got {other:?}"),
    };
    rig.run(StateCommand::LogHabit { habit })
        .await
        .expect("accepted");
    // The journal entry carries the flush trigger as its op — the
    // pushed-boundary machinery sees it like any op id.
    let entry = {
        let persisted = rig.repo.snapshot();
        let entry = persisted.journal.entries.last().expect("journaled").clone();
        assert_eq!(entry.ops.len(), 1, "one flush trigger per habit write");
        entry
    };
    let persisted = rig.repo.snapshot();
    assert!(
        persisted
            .outbox
            .iter()
            .any(|pending| pending.op_id == entry.ops[0]
                && matches!(pending.op, LocalOp::FlushBoardAppData(_))),
        "the pending trigger is the board flush"
    );
    // The surface is fenced typed, exactly as undo/1 pinned it.
    let err = rig
        .run(StateCommand::Undo)
        .await
        .expect_err("the habit surface is not undoable");
    assert!(
        matches!(
            err,
            ExecuteError::Rejected(taskboard_domain::CommandError::TargetNotUndoable(
                taskboard_domain::NotUndoableReason::HabitSurface
            ))
        ),
        "typed refusal: {err:?}"
    );
    // And the refused undo touched nothing.
    let after = rig.repo.snapshot();
    assert!(
        after
            .outbox
            .iter()
            .any(|pending| pending.op_id == entry.ops[0]),
        "the refused undo retracts nothing"
    );
}

// ---- the undo v2 widening (#555): one engine round-trip per arm ------
// The dispatch acceptance's contract: verb applied → undo restores the
// pre-state → redo re-applies, asserted on the *published* state with
// the persisted journal in agreement. Fine-grained op/watermark
// assertions stay in the domain suite (AGENTS §5, lowest layer).

fn expect_created_stack(outcome: CommandOutcome) -> taskboard_domain::StackId {
    match outcome {
        CommandOutcome::CreatedStack(id) => id,
        other => panic!("expected CreatedStack, got {other:?}"),
    }
}

fn expect_created_task(outcome: CommandOutcome) -> taskboard_domain::TaskId {
    match outcome {
        CommandOutcome::CreatedTask(id) => id,
        other => panic!("expected CreatedTask, got {other:?}"),
    }
}

fn expect_created_habit(outcome: CommandOutcome) -> taskboard_domain::HabitId {
    match outcome {
        CommandOutcome::HabitCreated(id) => id,
        other => panic!("expected HabitCreated, got {other:?}"),
    }
}

fn quota(n: u32) -> taskboard_domain::HabitTarget {
    taskboard_domain::HabitTarget::PerDay(taskboard_domain::NonZeroCount::new(n).unwrap())
}

#[tokio::test]
async fn log_time_round_trips_through_the_engine() {
    let mut rig = Rig::start().await;
    let board = rig.state().boards.keys().next().copied().expect("seed");
    let stack = expect_created_stack(
        rig.run(StateCommand::CreateStack {
            board,
            title: "todo".into(),
            order: 1,
        })
        .await
        .expect("accepted"),
    );
    let task = expect_created_task(
        rig.run(StateCommand::CreateTask {
            title: "t".into(),
            stack,
            order: 1,
        })
        .await
        .expect("accepted"),
    );
    rig.run(StateCommand::LogTime {
        task,
        start: common::ts(T0 - 3_600),
        end: common::ts(T0 - 1_800),
    })
    .await
    .expect("accepted");
    let logged = rig
        .repo
        .snapshot()
        .tracking
        .entries
        .values()
        .next()
        .expect("entry")
        .clone();
    // Undo: the entry is gone from the published projection and disk.
    rig.run(StateCommand::Undo).await.expect("accepted");
    let app = rig.state();
    assert!(!app.tracking.entries.contains_key(&logged.id));
    assert!(
        !rig.repo
            .snapshot()
            .tracking
            .entries
            .contains_key(&logged.id)
    );
    // Redo: re-mints fresh (the old id stays retired), same intent.
    rig.run(StateCommand::Redo).await.expect("accepted");
    let app = rig.state();
    assert!(
        !app.tracking.entries.contains_key(&logged.id),
        "old id retired"
    );
    let fresh = app.tracking.entries.values().next().expect("re-minted");
    assert_eq!(
        (fresh.task, fresh.start, fresh.end),
        (logged.task, logged.start, logged.end)
    );
    assert_eq!(
        rig.repo.snapshot().tracking.entries,
        app.tracking.entries,
        "memory ≡ disk"
    );
}

#[tokio::test]
async fn adjust_time_entry_round_trips_through_the_engine() {
    let mut rig = Rig::start().await;
    let board = rig.state().boards.keys().next().copied().expect("seed");
    let stack = expect_created_stack(
        rig.run(StateCommand::CreateStack {
            board,
            title: "todo".into(),
            order: 1,
        })
        .await
        .expect("accepted"),
    );
    let task = expect_created_task(
        rig.run(StateCommand::CreateTask {
            title: "t".into(),
            stack,
            order: 1,
        })
        .await
        .expect("accepted"),
    );
    rig.run(StateCommand::LogTime {
        task,
        start: common::ts(T0 - 3_600),
        end: common::ts(T0 - 1_800),
    })
    .await
    .expect("accepted");
    let entry_id = *rig
        .repo
        .snapshot()
        .tracking
        .entries
        .keys()
        .next()
        .expect("entry");
    let pre = rig.repo.snapshot().tracking.entries[&entry_id].clone();
    rig.run(StateCommand::AdjustTimeEntry {
        id: entry_id,
        start: Some(common::ts(T0 - 2_400)),
        end: Some(common::ts(T0 - 900)),
    })
    .await
    .expect("accepted");
    // Undo: the pre span restored on the published state and disk.
    rig.run(StateCommand::Undo).await.expect("accepted");
    let app = rig.state();
    assert_eq!(
        (
            app.tracking.entries[&entry_id].start,
            app.tracking.entries[&entry_id].end
        ),
        (pre.start, pre.end)
    );
    assert_eq!(
        rig.repo.snapshot().tracking.entries[&entry_id],
        app.tracking.entries[&entry_id]
    );
    // Redo: the forward mask re-applied.
    rig.run(StateCommand::Redo).await.expect("accepted");
    let app = rig.state();
    assert_eq!(
        (
            app.tracking.entries[&entry_id].start,
            app.tracking.entries[&entry_id].end
        ),
        (common::ts(T0 - 2_400), Some(common::ts(T0 - 900)))
    );
}

#[tokio::test]
async fn create_habit_round_trips_through_the_engine() {
    let mut rig = Rig::start().await;
    let board = rig.state().boards.keys().next().copied().expect("seed");
    // A create ahead of the habit keeps the head non-root after the
    // habit-undo: redo at the line's absolute start refuses by design
    // (the accepted §7 rule — head None ⇒ NothingToRedo).
    let _stack = expect_created_stack(
        rig.run(StateCommand::CreateStack {
            board,
            title: "todo".into(),
            order: 1,
        })
        .await
        .expect("accepted"),
    );
    let habit = expect_created_habit(
        rig.run(StateCommand::CreateHabit {
            board,
            title: "water".into(),
            target: quota(3),
        })
        .await
        .expect("accepted"),
    );
    // Undo: tombstoned on the published projection and disk.
    rig.run(StateCommand::Undo).await.expect("accepted");
    let app = rig.state();
    assert!(app.habits.habits[&habit].deleted);
    assert!(rig.repo.snapshot().habits.habits[&habit].deleted);
    // Redo: re-mints fresh.
    rig.run(StateCommand::Redo).await.expect("accepted");
    let app = rig.state();
    assert!(app.habits.habits[&habit].deleted, "old id retired");
    let fresh = app
        .habits
        .habits
        .values()
        .find(|row| row.is_live())
        .expect("re-minted habit");
    assert_eq!((fresh.title.as_str(), fresh.target), ("water", quota(3)));
    assert_eq!(rig.repo.snapshot().habits, app.habits, "memory ≡ disk");
}

#[tokio::test]
async fn update_habit_round_trips_through_the_engine() {
    let mut rig = Rig::start().await;
    let board = rig.state().boards.keys().next().copied().expect("seed");
    let habit = expect_created_habit(
        rig.run(StateCommand::CreateHabit {
            board,
            title: "water".into(),
            target: quota(3),
        })
        .await
        .expect("accepted"),
    );
    let pre = rig.repo.snapshot().habits.habits[&habit].clone();
    let week =
        taskboard_domain::HabitTarget::PerWeek(taskboard_domain::NonZeroCount::new(2).unwrap());
    rig.run(StateCommand::UpdateHabit {
        habit,
        changes: taskboard_domain::HabitChanges {
            title: Some("water 2".into()),
            target: Some(week),
            ..Default::default()
        },
    })
    .await
    .expect("accepted");
    // Undo: all clocked fields restored.
    rig.run(StateCommand::Undo).await.expect("accepted");
    let app = rig.state();
    let after = &app.habits.habits[&habit];
    assert_eq!(
        (after.title.as_str(), after.target),
        (pre.title.as_str(), pre.target)
    );
    assert_eq!(rig.repo.snapshot().habits.habits[&habit], *after);
    // Redo: the forward changeset re-applied.
    rig.run(StateCommand::Redo).await.expect("accepted");
    let app = rig.state();
    let after = &app.habits.habits[&habit];
    assert_eq!((after.title.as_str(), after.target), ("water 2", week));
}

#[tokio::test]
async fn move_habit_round_trips_through_the_engine() {
    let mut rig = Rig::start().await;
    let board = rig.state().boards.keys().next().copied().expect("seed");
    let habit = expect_created_habit(
        rig.run(StateCommand::CreateHabit {
            board,
            title: "water".into(),
            target: quota(3),
        })
        .await
        .expect("accepted"),
    );
    let pre_order = rig.repo.snapshot().habits.habits[&habit].order;
    rig.run(StateCommand::MoveHabit { habit, order: 42 })
        .await
        .expect("accepted");
    // Undo: the pre-image order restored (never a payload value).
    rig.run(StateCommand::Undo).await.expect("accepted");
    let app = rig.state();
    assert_eq!(app.habits.habits[&habit].order, pre_order);
    assert_eq!(
        rig.repo.snapshot().habits.habits[&habit].order,
        pre_order,
        "memory ≡ disk"
    );
    // Redo.
    rig.run(StateCommand::Redo).await.expect("accepted");
    assert_eq!(rig.state().habits.habits[&habit].order, 42);
}

#[tokio::test]
async fn adjust_habit_log_round_trips_through_the_engine() {
    let mut rig = Rig::start().await;
    let board = rig.state().boards.keys().next().copied().expect("seed");
    let habit = expect_created_habit(
        rig.run(StateCommand::CreateHabit {
            board,
            title: "water".into(),
            target: quota(3),
        })
        .await
        .expect("accepted"),
    );
    // Seed the window row with the (excluded) automatic path.
    rig.run(StateCommand::LogHabit { habit })
        .await
        .expect("accepted");
    let key = *rig
        .repo
        .snapshot()
        .habits
        .log
        .keys()
        .next()
        .expect("log row");
    let pre = rig.repo.snapshot().habits.log[&key];
    rig.run(StateCommand::AdjustHabitLog {
        habit,
        window: key.window,
        count: 5,
    })
    .await
    .expect("accepted");
    // Undo: the pre count restored, the row kept.
    rig.run(StateCommand::Undo).await.expect("accepted");
    let app = rig.state();
    assert_eq!(app.habits.log[&key].count, pre.count);
    assert_eq!(
        rig.repo.snapshot().habits.log[&key].count,
        pre.count,
        "memory ≡ disk"
    );
    // Redo: the audited correction re-applied.
    rig.run(StateCommand::Redo).await.expect("accepted");
    assert_eq!(rig.state().habits.log[&key].count, 5);
}

/// The pushed-boundary case over a widened arm: a completed push flips
/// the `LogTime` entry; its undo enqueues the inverse's op as a new edit
/// instead of retracting (the pushed-edit rule rides the widened arms
/// verbatim — zero new op code).
#[tokio::test]
async fn a_pushed_log_time_undo_enqueues_instead_of_retracting() {
    let mut rig = Rig::start().await;
    let board = rig.state().boards.keys().next().copied().expect("seed");
    let stack = expect_created_stack(
        rig.run(StateCommand::CreateStack {
            board,
            title: "todo".into(),
            order: 1,
        })
        .await
        .expect("accepted"),
    );
    let task = expect_created_task(
        rig.run(StateCommand::CreateTask {
            title: "t".into(),
            stack,
            order: 1,
        })
        .await
        .expect("accepted"),
    );
    rig.run(StateCommand::LogTime {
        task,
        start: common::ts(T0 - 3_600),
        end: common::ts(T0 - 1_800),
    })
    .await
    .expect("accepted");
    let (op_id, entry_id) = {
        let persisted = rig.repo.snapshot();
        let entry = persisted.journal.entries.last().expect("journaled");
        assert!(!entry.pushed);
        (entry.ops[0], entry.id)
    };
    // A cycle completes the push: the entry flips at the boundary (the
    // R-U9 post-pass, simulated through report ingestion like the v1
    // boundary test above).
    let report = SyncReport::Completed {
        connector: taskboard_domain::test_support::test_connector(),
        snapshot: Box::new(RemoteBoardSnapshot {
            board: RemoteBoard {
                id: taskboard_domain::RemoteBoardId(42),
                title: "kiosk".into(),
                color: "0000ff".into(),
                archived: false,
                deleted_at: None,
                last_modified: common::ts(T0 + 120),
            },
            stacks: vec![],
            tasks: vec![],
            labels: vec![],
        }),
        validators: common::test_validators(),
        pushes: vec![PushOutcome {
            op: op_id,
            result: PushResult::Applied { echo: None },
        }],
        read_at: common::ts(T0 + 120),
        duration: std::time::Duration::from_millis(5),
        app_data: taskboard_domain::AppDataEvidence::Absent,
        features: taskboard_domain::DeckFeatures::CONSERVATIVE,
    };
    rig.report_tx.send(report).await.expect("report lands");
    for _ in 0..100 {
        if rig
            .repo
            .snapshot()
            .journal
            .entry(entry_id)
            .is_some_and(|entry| entry.pushed)
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        rig.repo
            .snapshot()
            .journal
            .entry(entry_id)
            .is_some_and(|entry| entry.pushed),
        "the completed push flipped the entry"
    );
    // The pushed undo: no retraction — the inverse's op enqueues.
    let outbox_before = rig.repo.snapshot().outbox.len();
    rig.run(StateCommand::Undo).await.expect("accepted");
    let persisted = rig.repo.snapshot();
    assert_eq!(
        persisted.outbox.len(),
        outbox_before + 1,
        "a pushed undo enqueues the inverse as a new edit"
    );
    assert!(
        !persisted
            .outbox
            .iter()
            .any(|pending| pending.op_id == op_id),
        "no retraction: the completed op stays completed"
    );
    let app = rig.state();
    assert!(
        app.tracking.entries.is_empty(),
        "the undo deleted the logged entry regardless of the boundary"
    );
    assert_eq!(persisted.tracking.entries, app.tracking.entries);
}
