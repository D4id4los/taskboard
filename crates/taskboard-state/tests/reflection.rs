// SPDX-License-Identifier: MIT OR Apache-2.0
// Environment gate, not a weakened assertion: this suite drives the live
// tokio engine (channels, scheduling), which testing_strategy §10 scopes
// out of Miri. The transition logic it routes to is Miri-covered in the
// domain suites and in `core.rs`.
#![cfg_attr(miri, allow(dead_code))]
//! Engine tier for the reflection verbs (DR-7 chunk 1, plan §5.4): a
//! record publishes the row (memory ≡ disk), the change-mask no-op
//! publishes nothing, the delete tombstones in place, and the
//! crash-resume leg — a fresh engine over the same repository sees the
//! entry. Undo of a reflection head refuses typed
//! (`ReflectionSurface`). Injected clocks, oneshot replies as
//! completion signals, paused-time deadlines — no sleeps, no
//! wall-clock assumptions.

use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{CommandOutcome, NotifierMessage, StateCommand, SyncCommand, SystemEvent};
use taskboard_state::{EngineHandle, ExecuteError, spawn_state_engine};

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
        taskboard_domain::SettingsSnapshot::default(),
        taskboard_state::DayRollConfig::disabled(),
        None,
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
    /// The published reflection entries, sorted by id (the map's
    /// deterministic read).
    fn published_entries(&self) -> Vec<taskboard_domain::ReflectionEntry> {
        self.handle
            .shared_state()
            .load()
            .reflection
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

fn day_window() -> (
    chrono::NaiveDate,
    (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>),
) {
    use chrono::TimeZone as _;
    let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
    let start = chrono::Utc.with_ymd_and_hms(2026, 11, 5, 0, 0, 0).unwrap();
    let end = chrono::Utc.with_ymd_and_hms(2026, 11, 6, 0, 0, 0).unwrap();
    (day, (start, end))
}

fn record_command(text: &str) -> StateCommand {
    let (day, window) = day_window();
    StateCommand::RecordReflection {
        cadence: taskboard_domain::ReviewCadence::Day,
        day,
        window,
        text: text.to_owned(),
        mood: Some(4),
        energy: None,
        flags: std::collections::BTreeSet::from([taskboard_domain::HealthFlag::GoodDay]),
    }
}

/// A1: record publishes the row, and memory equals disk at the first
/// read after the receipt (the receipt implies durability).
#[tokio::test]
async fn t1_record_publishes_the_row_memory_equals_disk() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let h = spawn_over(repo, FixedClock::new(T0)).await;

    let outcome = h.handle.execute(record_command("the day's close")).await;
    let CommandOutcome::ReflectionRecorded { entry } = outcome.expect("accepted") else {
        panic!("wrong receipt");
    };
    assert_eq!(entry.created_at, common::ts(T0));
    assert_eq!(entry.updated_at, common::ts(T0));
    assert!(entry.is_live());

    // The published snapshot holds exactly the entry, live.
    let published = h.published_entries();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].id, entry.id);
    // And the disk carries the same row (memory ≡ disk by construction).
    let persisted = h.repo.snapshot();
    assert_eq!(persisted.reflection.entries.get(&entry.id), Some(&entry));
    h.shutdown().await;
}

/// A2: the update publishes the post-write row; an all-unchanged mask
/// is the accepted no-op that publishes nothing.
#[tokio::test]
async fn t2_update_publishes_and_the_no_op_changes_nothing() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = FixedClock::new(T0);
    let h = spawn_over(repo, clock.clone()).await;
    let CommandOutcome::ReflectionRecorded { entry } = h
        .handle
        .execute(record_command("first draft"))
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };

    clock.advance(1_800);
    let outcome = h
        .handle
        .execute(StateCommand::UpdateReflection {
            id: entry.id,
            text: Some("the edited close".into()),
            mood: Some(None),
            energy: Some(Some(2)),
            flags_add: std::collections::BTreeSet::from([taskboard_domain::HealthFlag::Cramps]),
            flags_remove: std::collections::BTreeSet::new(),
        })
        .await
        .expect("accepted");
    let CommandOutcome::ReflectionUpdated { entry: updated } = outcome else {
        panic!("wrong receipt");
    };
    assert_eq!(updated.text, "the edited close");
    assert_eq!(updated.attributes.mood, None);
    assert_eq!(
        updated.attributes.energy,
        taskboard_domain::Rating::try_new(2).ok()
    );
    assert_eq!(updated.updated_at, common::ts(T0 + 1_800));
    assert_eq!(
        updated.created_at,
        common::ts(T0),
        "creation stamp untouched"
    );
    let persisted = h.repo.snapshot();
    assert_eq!(
        persisted.reflection.entries.get(&entry.id),
        Some(&updated),
        "the post-write row is durable"
    );

    // The mask-no-churn no-op: nothing persisted, nothing published.
    let before = h.repo.snapshot();
    let repeat = h
        .handle
        .execute(StateCommand::UpdateReflection {
            id: entry.id,
            text: Some("the edited close".into()),
            mood: Some(None),
            energy: Some(Some(2)),
            flags_add: std::collections::BTreeSet::new(),
            flags_remove: std::collections::BTreeSet::new(),
        })
        .await
        .expect("accepted");
    assert!(matches!(repeat, CommandOutcome::NoOp));
    assert_eq!(h.repo.snapshot(), before, "the no-op persisted nothing");
    h.shutdown().await;
}

/// A3: the delete tombstones in place — the row survives the publish,
/// live reads filter it; a repeat is the accepted no-op; an unknown id
/// is the typed rejection (P-6).
#[tokio::test]
async fn t3_delete_tombstones_in_place() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = FixedClock::new(T0);
    let h = spawn_over(repo, clock.clone()).await;
    let CommandOutcome::ReflectionRecorded { entry } = h
        .handle
        .execute(record_command("to be tombstoned"))
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };

    clock.advance(600);
    let outcome = h
        .handle
        .execute(StateCommand::DeleteReflection { id: entry.id })
        .await
        .expect("accepted");
    assert!(matches!(outcome, CommandOutcome::ReflectionDeleted { .. }));

    // Tombstone visible in the map, absent from the live seam.
    let published = h.published_entries();
    assert_eq!(published.len(), 1, "the row survives the publish");
    assert!(published[0].deleted);
    let live: usize = {
        let snapshot = h.handle.shared_state().load_full();
        snapshot.reflection.live_entries().count()
    };
    assert_eq!(live, 0, "the live seam filters the tombstone");
    let persisted = h.repo.snapshot();
    assert!(persisted.reflection.entries[&entry.id].deleted);

    // Repeat: the accepted no-op.
    let repeat = h
        .handle
        .execute(StateCommand::DeleteReflection { id: entry.id })
        .await
        .expect("accepted");
    assert!(matches!(repeat, CommandOutcome::NoOp));
    // Unknown: typed.
    let stranger = taskboard_domain::ReflectionId::from(uuid::Uuid::from_u128(999));
    let err = h
        .handle
        .execute(StateCommand::DeleteReflection { id: stranger })
        .await
        .expect_err("typed");
    assert!(matches!(
        err,
        ExecuteError::Rejected(taskboard_domain::CommandError::UnknownReflection(_))
    ));
    h.shutdown().await;
}

/// A4: the crash-resume — one shared repository; engine 1 records and
/// shuts down; engine 2 boots over the same repo and hydrates the entry
/// with its original stamps and tombstone state.
#[tokio::test]
async fn t4_fresh_engine_over_the_same_repo_sees_the_entries() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let clock = FixedClock::new(T0);
    let engine1 = spawn_over(repo.clone(), clock.clone()).await;
    let CommandOutcome::ReflectionRecorded { entry } = engine1
        .handle
        .execute(record_command("survives the restart"))
        .await
        .expect("accepted")
    else {
        panic!("wrong receipt");
    };
    engine1.shutdown().await;

    clock.advance(3_600);
    let engine2 = spawn_over(repo.clone(), clock.clone()).await;
    let published = engine2.published_entries();
    assert_eq!(published.len(), 1, "the entry hydrated");
    assert_eq!(published[0], entry, "same row, verbatim");
    engine2.shutdown().await;
}

/// A5: undo over a reflection head refuses with the typed
/// `ReflectionSurface` reason.
#[tokio::test]
async fn t5_undo_over_a_reflection_head_refuses_typed() {
    let repo = Arc::new(InMemoryRepository::with_state(seed_board_state("kiosk")));
    let h = spawn_over(repo, FixedClock::new(T0)).await;
    h.handle
        .execute(record_command("not undoable"))
        .await
        .expect("accepted");
    let err = h
        .handle
        .execute(StateCommand::Undo)
        .await
        .expect_err("typed");
    assert!(matches!(
        err,
        ExecuteError::Rejected(taskboard_domain::CommandError::TargetNotUndoable(
            taskboard_domain::NotUndoableReason::ReflectionSurface
        ))
    ));
    h.shutdown().await;
}
