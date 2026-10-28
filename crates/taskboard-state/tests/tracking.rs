// SPDX-License-Identifier: MIT OR Apache-2.0
// Environment gate, not a weakened assertion: this suite drives the live
// tokio engine (channels, scheduling), which testing_strategy §10 scopes
// out of Miri. The transition logic it routes to is Miri-covered in the
// domain suites and in `core.rs`.
#![cfg_attr(miri, allow(dead_code))]
//! Engine tier for the tracking verbs (DR-5/1, A-series style): start
//! publishes the open row (memory ≡ disk), stop stamps and publishes,
//! and the crash-resume leg — a fresh engine over the same repository
//! sees the open entry, because the open row is the intent (ADR 0017).
//! Injected clocks, oneshot replies as completion signals, poll with a
//! paused-time deadline — no sleeps beyond the poll interval, no
//! wall-clock assumptions.

use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{CommandOutcome, NotifierMessage, StateCommand, SyncCommand, SystemEvent};
use taskboard_state::{EngineHandle, ReminderEngineConfig, spawn_state_engine};

mod common;

use common::{FixedClock, T0, seed_board_state};

struct Harness {
    handle: EngineHandle,
    repo: Arc<InMemoryRepository>,
    system_tx: tokio::sync::broadcast::Sender<SystemEvent>,
    join: tokio::task::JoinHandle<()>,
}

async fn spawn_over(repo: Arc<InMemoryRepository>, clock: Arc<FixedClock>) -> Harness {
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (_report_tx, report_rx) = tokio::sync::mpsc::channel::<taskboard_domain::SyncReport>(8);
    let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    let (notifier, _notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
    let (handle, join) = spawn_state_engine(
        repo.clone(),
        Arc::new(CountingIds::new()),
        clock.clone(),
        sync_out,
        report_rx,
        system_tx.clone(),
        system_rx,
        notifier,
        ReminderEngineConfig::disabled(),
        taskboard_state::DayRollConfig::disabled(),
    )
    .await
    .expect("engine boots");
    Harness {
        handle,
        repo,
        system_tx,
        join,
    }
}

impl Harness {
    fn board_id(&self) -> taskboard_domain::BoardId {
        *self
            .handle
            .shared_state()
            .load()
            .boards
            .keys()
            .next()
            .expect("the fixture has exactly one board")
    }

    /// The published tracking rows, sorted by id (the open set's
    /// deterministic read).
    fn published_entries(&self) -> Vec<taskboard_domain::TimeEntry> {
        self.handle
            .shared_state()
            .load()
            .tracking
            .entries
            .values()
            .cloned()
            .collect()
    }

    /// Graceful shutdown: the drain starts on `Shutdown`, the join
    /// completes within a paused-time deadline (the engine.rs idiom).
    async fn shutdown(self) {
        self.system_tx
            .send(SystemEvent::Shutdown)
            .expect("broadcast");
        tokio::time::timeout(Duration::from_secs(60), self.join)
            .await
            .expect("engine exits after drain")
            .expect("clean exit");
    }
}

/// A live task to track: stack + task authored through the engine.
async fn seed_task(h: &Harness) -> taskboard_domain::TaskId {
    let CommandOutcome::CreatedStack(stack) = h
        .handle
        .execute(StateCommand::CreateStack {
            board: h.board_id(),
            title: "todo".into(),
            order: 1,
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    let CommandOutcome::CreatedTask(task) = h
        .handle
        .execute(StateCommand::CreateTask {
            title: "tracked work".into(),
            stack,
            order: 1,
        })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    task
}

/// A1: start publishes the open row, and memory equals disk at the
/// first read after the receipt (the receipt implies durability).
#[tokio::test]
async fn t1_start_publishes_the_open_row_memory_equals_disk() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let h = spawn_over(repo, FixedClock::new(T0)).await;
    let task = seed_task(&h).await;

    let outcome = h
        .handle
        .execute(StateCommand::StartTracking { task })
        .await
        .expect("accepted");
    let CommandOutcome::TrackingStarted { entry, closed } = outcome else {
        panic!("wrong receipt");
    };
    assert_eq!(closed, Vec::new());
    assert_eq!(entry.task, task);
    assert_eq!(entry.end, None);
    assert_eq!(entry.start, common::ts(T0));

    // The published snapshot holds exactly the open entry.
    let published = h.published_entries();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].id, entry.id);
    assert_eq!(published[0].end, None);
    // And the disk carries the same row (memory ≡ disk by construction).
    let persisted = h.repo.snapshot();
    assert_eq!(persisted.tracking.entries.get(&entry.id), Some(&entry));
    // Elapsed is derived at read, never stored: no cached duration, no
    // `running` flag — the entry's fields are the whole story.
    assert_eq!(
        published[0].duration(common::ts(T0 + 60)),
        chrono::TimeDelta::try_seconds(60).unwrap()
    );
    h.shutdown().await;
}

/// A2: stop stamps every open entry at the engine's (advanced) now and
/// publishes; the persisted rows agree.
#[tokio::test]
async fn t2_stop_stamps_and_publishes() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = FixedClock::new(T0);
    let h = spawn_over(repo, clock.clone()).await;
    let task = seed_task(&h).await;
    h.handle
        .execute(StateCommand::StartTracking { task })
        .await
        .expect("accepted");

    clock.advance(1_800);
    let outcome = h
        .handle
        .execute(StateCommand::StopTracking)
        .await
        .expect("accepted");
    let CommandOutcome::TrackingStopped { entries } = outcome else {
        panic!("wrong receipt");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].end, Some(common::ts(T0 + 1_800)));
    assert_eq!(entries[0].updated_at, common::ts(T0 + 1_800));

    let published = h.published_entries();
    assert_eq!(published[0], entries[0], "published rows are post-stamp");
    let persisted = h.repo.snapshot();
    assert_eq!(
        persisted.tracking.entries.get(&entries[0].id),
        Some(&entries[0]),
        "the stamped row is durable"
    );

    // Nothing open: the repeat is the accepted no-op, nothing persisted.
    let before = h.repo.snapshot();
    let repeat = h.handle.execute(StateCommand::StopTracking).await;
    match repeat {
        Ok(CommandOutcome::NoOp) => {}
        other => panic!("expected NoOp, got {other:?}"),
    }
    assert_eq!(h.repo.snapshot(), before, "the no-op persisted nothing");
    h.shutdown().await;
}

/// A3: the crash-resume — one shared repository; engine 1 starts
/// tracking and shuts down; engine 2 boots over the same repo, hydrates
/// the open entry with its original start, and its stop closes it. The
/// intent survived process death because the open row IS the intent
/// (ADR 0017).
#[tokio::test]
async fn t3_fresh_engine_over_the_same_repo_sees_the_running_entry() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = FixedClock::new(T0);
    let engine1 = spawn_over(repo.clone(), clock.clone()).await;
    let task = seed_task(&engine1).await;
    let CommandOutcome::TrackingStarted { entry, .. } = engine1
        .handle
        .execute(StateCommand::StartTracking { task })
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    engine1.shutdown().await;

    // A "fresh process": a new engine over the same repository.
    clock.advance(3_600);
    let engine2 = spawn_over(repo.clone(), clock.clone()).await;
    let published = engine2.published_entries();
    assert_eq!(published.len(), 1, "the open entry hydrated");
    assert_eq!(published[0].id, entry.id, "same entry identity");
    assert_eq!(published[0].start, common::ts(T0), "the start survived");
    assert_eq!(published[0].end, None, "still running");
    // The timer "resumed" because it never stopped: elapsed derives
    // from the original start across the restart.
    assert_eq!(
        published[0].duration(common::ts(T0 + 3_600)),
        chrono::TimeDelta::try_seconds(3_600).unwrap()
    );

    let CommandOutcome::TrackingStopped { entries } = engine2
        .handle
        .execute(StateCommand::StopTracking)
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, entry.id);
    assert_eq!(entries[0].end, Some(common::ts(T0 + 3_600)));
    let persisted = engine2.repo.snapshot();
    assert_eq!(
        persisted
            .tracking
            .entries
            .get(&entry.id)
            .and_then(|row| row.end),
        Some(common::ts(T0 + 3_600)),
        "the stop across the restart is durable"
    );
    engine2.shutdown().await;
}
