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
    RemoteBoard, StateCommand, SyncCommand, SyncReport, SystemEvent, TaskRepository,
    persistence::RepositoryError,
};
use taskboard_state::{DayRollConfig, ExecuteError, spawn_state_engine};

use crate::common;

use crate::common::{FixedClock, T0, seed_bound_board_state};

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

    fn read_settings(&self) -> taskboard_domain::SettingsSnapshot {
        self.inner.read_settings()
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
        Self::start_with(seed_bound_board_state("kiosk")).await
    }

    /// A rig over a caller-supplied persisted seed (the multi-board
    /// scoped-undo scenarios; a fresh `CountingIds` engine would collide
    /// with fixture ids minted the same way).
    async fn start_with(seed: PersistedState) -> Self {
        let repo = Arc::new(InMemoryRepository::with_state(seed));
        let clock = Arc::new(FixedClock::from_secs(T0));
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
            taskboard_domain::SettingsSnapshot::default(),
            DayRollConfig::disabled(),
            None,
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
        self.clock.advance_secs(60);
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
    let clock = Arc::new(FixedClock::from_secs(T0));
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
        taskboard_domain::SettingsSnapshot::default(),
        DayRollConfig::disabled(),
        None,
    )
    .await
    .expect("engine boots");
    clock.advance_secs(60);
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
        snapshot: Box::new(taskboard_domain::RemoteBoardSnapshot::Deck(
            taskboard_domain::DeckBoardSnapshot {
                board: RemoteBoard {
                    id: taskboard_domain::RemoteBoardId::deck(42),
                    title: "kiosk".into(),
                    color: "0000ff".into(),
                    archived: false,
                    deleted_at: None,
                    last_modified: common::ts(T0 + 120),
                },
                stacks: vec![],
                tasks: vec![],
                labels: vec![],
            },
        )),
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

/// Fact 16's DR-6/6 verification, as superseded by R-7 (2026-11-21,
/// habit-family UX plan): `LogHabit` is journal-invertible now — the
/// undo goes through (no op retraction to mirror; the inverse adjust
/// enqueues its own flush trigger). The surviving pin is that the
/// flush trigger still rides the journal entry generically (the
/// pushed-boundary machinery stays op-id-aware of it) while `Undo`
/// inverts the entry through the global undo line.
#[tokio::test]
async fn a_habit_log_journals_its_flush_trigger_and_inverts_through_undo() {
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
    // The R-7 undo goes through the global line. The log is unpushed,
    // so the D-U2 pre-push retraction applies: its flush trigger is
    // failed (the server never knew) and the inverse adjust's fresh
    // enqueues drop — the outbox ends empty, the inverse's journal
    // entry stands.
    rig.run(StateCommand::Undo).await.expect("accepted");
    let after = rig.repo.snapshot();
    assert!(
        !after
            .outbox
            .iter()
            .any(|pending| pending.op_id == entry.ops[0]),
        "the unpushed log's trigger is retracted, never pushed orphaned"
    );
    assert!(
        after.journal.entries.len() > 1,
        "the inverse adjust journaled its own entry"
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
        kind: taskboard_domain::tracking::TimeEntryKind::Work,
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
        kind: taskboard_domain::tracking::TimeEntryKind::Work,
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
        kind: None,
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
        kind: taskboard_domain::tracking::TimeEntryKind::Work,
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
        snapshot: Box::new(taskboard_domain::RemoteBoardSnapshot::Deck(
            taskboard_domain::DeckBoardSnapshot {
                board: RemoteBoard {
                    id: taskboard_domain::RemoteBoardId::deck(42),
                    title: "kiosk".into(),
                    color: "0000ff".into(),
                    archived: false,
                    deleted_at: None,
                    last_modified: common::ts(T0 + 120),
                },
                stacks: vec![],
                tasks: vec![],
                labels: vec![],
            },
        )),
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
        !persisted.outbox.iter().any(|p| p.op_id == op_id),
        "no retraction: the completed op stays completed"
    );
    let app = rig.state();
    assert!(
        app.tracking.entries.is_empty(),
        "the undo deleted the logged entry regardless of the boundary"
    );
    assert_eq!(persisted.tracking.entries, app.tracking.entries);
}

#[tokio::test]
async fn a_delete_stack_undo_flips_the_published_state_and_the_persisted_journal() {
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
    let outcome = rig
        .run(StateCommand::DeleteStack { id: stack })
        .await
        .expect("accepted");
    assert!(matches!(outcome, CommandOutcome::Applied));
    let app = rig.state();
    assert!(app.stacks[&stack].deleted);
    assert!(app.tasks[&task].deleted);
    // Undo: the composite restore flips the published state...
    let outcome = rig.run(StateCommand::Undo).await.expect("accepted");
    assert!(matches!(outcome, CommandOutcome::Undone { .. }));
    let app = rig.state();
    assert!(!app.stacks[&stack].deleted, "the stack lives again");
    assert!(!app.tasks[&task].deleted, "the cascade member lives again");
    // ...and the persisted journal agrees (memory ≡ disk), the head
    // moved off the delete entry.
    let persisted = rig.repo.snapshot();
    assert!(!persisted.stacks[&stack].deleted, "memory ≡ disk");
    assert!(!persisted.tasks[&task].deleted, "memory ≡ disk");
    let delete_entry = persisted
        .journal
        .entries
        .iter()
        .find(|entry| matches!(entry.command, StateCommand::DeleteStack { .. }))
        .expect("the delete is journaled");
    assert_eq!(
        persisted.journal.head, delete_entry.parent,
        "the head sits on the delete entry's parent"
    );
    assert_eq!(
        app.undo,
        taskboard_domain::UndoView::derive(&persisted.journal),
        "the published undo projection derives from the persisted line"
    );
    // Redo re-plans the forward intent: the delete re-cascades.
    rig.run(StateCommand::Redo).await.expect("accepted");
    let app = rig.state();
    assert!(app.stacks[&stack].deleted);
    assert!(app.tasks[&task].deleted);
}

// ---- the per-board undo scope (#355) ---------------------------------------

#[tokio::test]
async fn engine_scoped_undo_refuses_when_the_head_is_out_of_scope_and_the_published_state_is_unchanged()
 {
    let mut rig = {
        let mut seed = seed_bound_board_state("kiosk");
        // A second board on a distinct id: `board()` mints from a fresh
        // counter (id 1, the seed's own), so burn one and mint the next.
        let ids = taskboard_domain::test_support::CountingIds::new();
        let _ = taskboard_domain::idgen::IdGenerator::new_board_id(&ids);
        let mut second = seed.boards.values().next().cloned().expect("seed board");
        second.id = taskboard_domain::idgen::IdGenerator::new_board_id(&ids);
        second.title = "second".into();
        seed.boards.insert(second.id, second);
        Rig::start_with(seed).await
    };
    let (board_a, board_b) = {
        let mut keys = rig.state().boards.keys().copied().collect::<Vec<_>>();
        keys.sort();
        (
            keys.first().copied().expect("board a"),
            keys.last().copied().expect("board b"),
        )
    };
    // Board B gets the line's head: a stack, a task.
    let stack_b = expect_created_stack(
        rig.run(StateCommand::CreateStack {
            board: board_b,
            title: "stack".into(),
            order: 1,
        })
        .await
        .expect("accepted"),
    );
    expect_created_task(
        rig.run(StateCommand::CreateTask {
            title: "t".into(),
            stack: stack_b,
            order: 1,
        })
        .await
        .expect("accepted"),
    );
    // The scoped undo on board A asks a frontier that belongs to B.
    let persisted_before = rig.repo.snapshot();
    let err = rig
        .run(StateCommand::UndoOnBoard { board: board_a })
        .await
        .expect_err("typed refusal");
    assert!(matches!(
        err,
        ExecuteError::Rejected(taskboard_domain::CommandError::NothingToUndoForBoard { board })
            if board == board_a
    ));
    // The published state and the persisted journal are unchanged: the
    // head was checked, never skipped past.
    assert_eq!(rig.repo.snapshot(), persisted_before);
    let app = rig.state();
    assert!(app.undo.can_undo, "the global line still has a head");
}

#[tokio::test]
async fn engine_scoped_undo_flips_the_published_state_and_the_persisted_journal() {
    let mut rig = Rig::start().await;
    let board = rig
        .state()
        .boards
        .keys()
        .next()
        .copied()
        .expect("seed board");
    let stack = expect_created_stack(
        rig.run(StateCommand::CreateStack {
            board,
            title: "stack".into(),
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
    // The scoped undo through the engine: published projection and
    // persisted journal agree (the mirror of the global engine test).
    let outcome = rig
        .run(StateCommand::UndoOnBoard { board })
        .await
        .expect("accepted");
    assert!(matches!(outcome, CommandOutcome::Undone { .. }));
    let app = rig.state();
    assert!(app.tasks[&task].deleted, "the create undone");
    let persisted = rig.repo.snapshot();
    assert!(persisted.tasks[&task].deleted, "memory ≡ disk");
    assert_eq!(
        persisted.journal.head,
        app.undo.recent.last().map(|label| label.id),
        "the head sits on the oldest entry"
    );
}

// ---- the restore family (#913 + #900) through the engine -------------------

#[tokio::test]
async fn engine_undo_of_restore_task_flips_the_published_state_and_the_persisted_journal() {
    let mut rig = Rig::start().await;
    let board = rig
        .state()
        .boards
        .keys()
        .next()
        .copied()
        .expect("seed board");
    let stack = expect_created_stack(
        rig.run(StateCommand::CreateStack {
            board,
            title: "stack".into(),
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
    rig.run(StateCommand::DeleteTask { id: task })
        .await
        .expect("accepted");
    // The forward restore verb dispatch (the #913 entry shape); the
    // resurrect payload rides from the published state.
    let resurrect = rig.state().tasks[&task].clone();
    rig.run(StateCommand::RestoreTask {
        id: task,
        resurrect: Box::new(resurrect),
    })
    .await
    .expect("accepted");
    // Undo the restore: the task is tombstoned again through the
    // ordinary DeleteTask — the published projection and the persisted
    // journal agree.
    let outcome = rig.run(StateCommand::Undo).await.expect("accepted");
    assert!(matches!(outcome, CommandOutcome::Undone { .. }));
    let app = rig.state();
    assert!(app.tasks[&task].deleted, "the restore undone");
    let persisted = rig.repo.snapshot();
    assert!(persisted.tasks[&task].deleted, "memory ≡ disk");
    let restore_entry = persisted
        .journal
        .entries
        .iter()
        .find(|entry| matches!(entry.command, StateCommand::RestoreTask { .. }))
        .expect("the restore is journaled");
    assert_eq!(
        persisted.journal.head, restore_entry.parent,
        "the head sits on the restore entry's parent"
    );
    // Redo re-plans the forward intent: the task lives again.
    rig.run(StateCommand::Redo).await.expect("accepted");
    let app = rig.state();
    assert!(!app.tasks[&task].deleted, "redo re-restores");
}

#[tokio::test]
async fn engine_undo_of_delete_stack_resurrects_rules_in_the_persisted_state() {
    let mut rig = Rig::start().await;
    let board = rig
        .state()
        .boards
        .keys()
        .next()
        .copied()
        .expect("seed board");
    let stack = expect_created_stack(
        rig.run(StateCommand::CreateStack {
            board,
            title: "stack".into(),
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
    rig.run(StateCommand::AddReminderRule {
        task,
        offset: taskboard_domain::reminders::ReminderOffset::before(chrono::TimeDelta::minutes(
            -30,
        ))
        .expect("valid"),
        at: None,
        insistence: taskboard_domain::reminders::Insistence::Normal,
    })
    .await
    .expect("accepted");
    let rule_id = *rig.state().reminders.rules.keys().next().expect("rule row");
    let rule_before = rig.state().reminders.rules[&rule_id].clone();
    rig.run(StateCommand::DeleteStack { id: stack })
        .await
        .expect("accepted");
    assert!(
        rig.state().reminders.rules.is_empty(),
        "the cascade removed the rule"
    );
    // Undo: the composite restore resurrects the task AND, action-level,
    // the cascaded rule — reaching the persisted state (memory ≡ disk).
    rig.run(StateCommand::Undo).await.expect("accepted");
    let app = rig.state();
    assert!(!app.tasks[&task].deleted, "the task lives again");
    assert_eq!(
        app.reminders.rules.get(&rule_id),
        Some(&rule_before),
        "the rule came back verbatim"
    );
    let persisted = rig.repo.snapshot();
    assert_eq!(
        persisted.reminders.rules.get(&rule_id),
        Some(&rule_before),
        "the action-level resurrect reached the persisted state"
    );
}

/// US-1/S4 (HF1): a skip pops via the global undo line — the journal
/// entry carries the prior row shape (or none) as its pre-image, the
/// undo plans the mirror verb, and the published state shows the mark
/// cleared with the count preserved (memory ≡ disk).
#[tokio::test(start_paused = true)]
async fn us1_s4_undo_clears_the_skip_mark_through_the_global_line() {
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
                taskboard_domain::NonZeroCount::new(2).unwrap(),
            ),
        })
        .await
        .expect("accepted");
    let habit = match outcome {
        CommandOutcome::HabitCreated(id) => id,
        other => panic!("expected HabitCreated, got {other:?}"),
    };
    // Log once (the pre-image row shape exists) then skip.
    rig.run(StateCommand::LogHabit { habit })
        .await
        .expect("accepted");
    rig.run(StateCommand::SkipHabit { habit })
        .await
        .expect("accepted");
    let app = rig.state();
    let key = taskboard_domain::HabitLogKey {
        habit,
        window: taskboard_domain::WindowStart::of_period(
            taskboard_domain::PeriodKind::Day,
            chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch date"),
        ),
    };
    assert!(
        app.habits.log[&key].skip,
        "the mark is on the published row"
    );
    drop(app);
    // The global undo pops the skip (the LogHabit below it stays).
    let outcome = rig.run(StateCommand::Undo).await.expect("accepted");
    assert!(matches!(outcome, CommandOutcome::Undone { .. }));
    let app = rig.state();
    assert!(!app.habits.log[&key].skip, "the skip undo cleared the mark");
    assert_eq!(app.habits.log[&key].count, 1, "the count survived");
    drop(app);
    let persisted = rig.repo.snapshot();
    assert!(
        !persisted.habits.log[&key].skip,
        "memory ≡ disk: the cleared mark persisted"
    );
    // Redo re-applies the skip intent (the forward re-plan re-mints the
    // row clock — the documented #986 bump), so the guard law "any later
    // write to the window refuses" applies to a second undo after the
    // redo (the AdjustHabitLog precedent verbatim); the
    // undo → redo pair is the pinned journey here.
    rig.run(StateCommand::Redo).await.expect("accepted");
    let app = rig.state();
    assert!(app.habits.log[&key].skip, "redo re-applied the mark");
    drop(app);
    let persisted = rig.repo.snapshot();
    assert!(persisted.habits.log[&key].skip, "memory ≡ disk after redo");
}

// ---- the habit-log undo widening (R-7, 2026-11-21; UX plan US-3) -------

/// The engine's habit fixture: one live `PerDay` habit on the seed board.
async fn habit_fixture(rig: &mut Rig) -> taskboard_domain::HabitId {
    let board = rig
        .state()
        .boards
        .keys()
        .next()
        .copied()
        .expect("seed board");
    match rig
        .run(StateCommand::CreateHabit {
            board,
            title: "water".into(),
            target: taskboard_domain::HabitTarget::PerDay(
                taskboard_domain::NonZeroCount::new(3).unwrap(),
            ),
        })
        .await
        .expect("accepted")
    {
        CommandOutcome::HabitCreated(id) => id,
        other => panic!("expected HabitCreated, got {other:?}"),
    }
}

/// US-3/AC-1 (R-7 supersession, 2026-11-21): a `LogHabit` as the newest
/// journal entry inverts through the global undo line — the window's
/// count reverts to the pre-image and the receipt names the habit
/// surface (the typed `HabitLogAdjusted` of the inverse adjust).
#[tokio::test]
async fn log_habit_journal_entry_inverts() {
    let mut rig = Rig::start().await;
    let habit = habit_fixture(&mut rig).await;
    let logged = rig
        .run(StateCommand::LogHabit { habit })
        .await
        .expect("accepted");
    let CommandOutcome::HabitLogged { count, window, .. } = logged else {
        panic!("expected HabitLogged, got {logged:?}")
    };
    assert_eq!(count, 1, "one increment");
    // The entry journaled the invertible pre-image (the R-7
    // classification, supplanting the former HabitSurface fence).
    {
        let persisted = rig.repo.snapshot();
        let entry = persisted.journal.entries.last().expect("journaled");
        assert!(
            matches!(entry.facts, taskboard_domain::UndoFacts::PreImages { .. }),
            "LogHabit journals PreImages since R-7"
        );
    }
    // Undo: the count reverts to the pre-image (no row before the log
    // — the honest count-0 restore), receipt names the habit surface
    // (the durable `Undone` entry id; the entry's own label — the
    // UndoView's source — is the habit surface's "Log habit").
    let undone = rig.run(StateCommand::Undo).await.expect("accepted");
    let CommandOutcome::Undone { entry: undone_id } = undone else {
        panic!("expected Undone, got {undone:?}")
    };
    let undone_entry = rig
        .repo
        .snapshot()
        .journal
        .entry(undone_id)
        .expect("the receipt's entry is on the line")
        .clone();
    assert!(
        matches!(undone_entry.command, StateCommand::LogHabit { .. }),
        "the receipt names the habit surface entry: {undone_entry:?}"
    );
    let app = rig.state();
    let key = taskboard_domain::HabitLogKey { habit, window };
    assert_eq!(
        app.habits.log.get(&key).map(|entry| entry.count),
        Some(0),
        "the pre-image absence restores as the count-0 correction row"
    );
}

/// US-3/AC-2: a same-second burst of logs (the #986 bump mint) pops
/// one increment per undo — per-entry invertibility, the bump's
/// monotone clocks distinguishing the rows.
#[tokio::test]
async fn undo_after_burst_pops_one_increment() {
    let mut rig = Rig::start().await;
    let habit = habit_fixture(&mut rig).await;
    // Two logs inside the same wall-clock second: the bump mint stamps
    // the second row strictly past the first.
    rig.clock.advance_secs(0);
    rig.handle
        .execute(StateCommand::LogHabit { habit })
        .await
        .expect("first log accepted");
    rig.handle
        .execute(StateCommand::LogHabit { habit })
        .await
        .expect("second log accepted");
    assert_eq!(
        rig.state()
            .habits
            .log
            .values()
            .map(|entry| u64::from(entry.count))
            .sum::<u64>(),
        2,
        "the burst landed both increments"
    );
    // First undo: exactly one increment reverts (the receipt's entry
    // is the second log).
    let undone = rig.run(StateCommand::Undo).await.expect("accepted");
    let CommandOutcome::Undone { entry: undone_id } = undone else {
        panic!("expected Undone, got {undone:?}")
    };
    {
        let persisted = rig.repo.snapshot();
        let popped = persisted.journal.entry(undone_id).expect("popped entry");
        assert!(
            matches!(popped.command, StateCommand::LogHabit { .. }),
            "one log entry per undo"
        );
        assert_eq!(persisted.journal.head, popped.parent, "one pop");
    }
    assert_eq!(
        rig.state()
            .habits
            .log
            .values()
            .map(|entry| u64::from(entry.count))
            .sum::<u64>(),
        1,
        "one increment reverted"
    );
    // Second undo: the other increment.
    rig.run(StateCommand::Undo).await.expect("accepted");
    assert_eq!(
        rig.state()
            .habits
            .log
            .values()
            .map(|entry| u64::from(entry.count))
            .sum::<u64>(),
        0,
        "the second undo pops the other increment"
    );
}

/// US-3/AC-3 (R-7's fence boundary): an `AdjustHabitLog` onto an
/// absent window as the newest entry keeps the typed
/// `NotUndoable(HabitSurface)` posture — the audited correction valve
/// keeps its fence.
#[tokio::test]
async fn adjust_habit_log_newest_keeps_the_typed_fence() {
    let mut rig = Rig::start().await;
    let habit = habit_fixture(&mut rig).await;
    let window = taskboard_domain::WindowStart::of_period(
        taskboard_domain::PeriodKind::Day,
        chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(),
    );
    rig.run(StateCommand::AdjustHabitLog {
        habit,
        window,
        count: 3,
    })
    .await
    .expect("the backfill itself is legal");
    let err = rig
        .run(StateCommand::Undo)
        .await
        .expect_err("the absent-window adjust stays fenced");
    assert!(
        matches!(
            err,
            ExecuteError::Rejected(taskboard_domain::CommandError::TargetNotUndoable(
                taskboard_domain::NotUndoableReason::HabitSurface
            ))
        ),
        "typed refusal: {err:?}"
    );
}

/// The #1207 never-silent pin (round 5 chunk A2, D-A2-4): an edit
/// session over the reported shape — a start date set, then further
/// edits — undoes as APPLIED (the head moves, the published state
/// flips back) or as a TYPED refusal; "undo did nothing" is the
/// bug class the pin forbids. The matched arms are the contract:
/// `Undone` with a state delta, or the typed `TargetNotUndoable` /
/// scope refusals — any other answer (an `Ok` that changes nothing,
/// an untyped error) fails the pin.
#[tokio::test]
async fn an_edit_session_undo_is_applied_or_typed_refusal_never_silent() {
    let mut rig = Rig::start().await;
    let board = rig
        .state()
        .boards
        .keys()
        .next()
        .copied()
        .expect("seed board");
    let stack = expect_created_stack(
        rig.run(StateCommand::CreateStack {
            board,
            title: "stack".into(),
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
    // The reported edit shape: the start date first, then further
    // edits — each an `UpdateTask` over the journal's PreImages
    // class.
    let start = common::ts(T0);
    rig.run(StateCommand::UpdateTask {
        id: task,
        changes: taskboard_domain::TaskChanges {
            scheduled: Some(Some(start)),
            ..taskboard_domain::TaskChanges::default()
        },
    })
    .await
    .expect("the start-date edit is accepted");
    rig.run(StateCommand::UpdateTask {
        id: task,
        changes: taskboard_domain::TaskChanges {
            title: Some("edited".into()),
            ..taskboard_domain::TaskChanges::default()
        },
    })
    .await
    .expect("the title edit is accepted");
    let undoable_before = rig.state().undo.can_redo;

    // The undo verb: applied or typed refusal, never silent.
    match rig.run(StateCommand::Undo).await {
        Ok(CommandOutcome::Undone { .. }) => {
            let app = rig.state();
            assert!(
                !undoable_before,
                "a fresh line has no redo branch (the cursor's pre-undo read)"
            );
            assert!(
                app.undo.can_redo,
                "an applied undo moved the journal cursor (the redo branch opened)"
            );
            assert_eq!(
                app.tasks[&task].title, "t",
                "the applied undo retracted the last edit (the state moved)"
            );
        }
        Ok(other) => panic!(
            "undo answered {other:?} without moving the state — the silent no-op class (#1207)"
        ),
        Err(ExecuteError::Rejected(
            taskboard_domain::CommandError::TargetNotUndoable(_)
            | taskboard_domain::CommandError::NothingToUndo
            | taskboard_domain::CommandError::NothingToUndoForBoard { .. },
        )) => {
            // The typed refusal: the contract's other half. The head
            // did not move and the refusal is repeatable — a
            // deterministic answer, never a swallowed one.
            let redo_after = rig.state().undo.can_redo;
            assert_eq!(
                redo_after, undoable_before,
                "a refusal never moves the head"
            );
        }
        Err(other) => panic!("undo failed untyped: {other:?}"),
    }
}
