// SPDX-License-Identifier: MIT OR Apache-2.0
//! The degraded-boot routing (B2 plan matrix case 10 + healthy pin
//! case 11, #1039): a repository that quarantined journal rows during
//! boot hydration yields exactly one typed `SystemEvent::JournalDegraded`
//! on the system feed, with the engine booted on healthy state; a
//! repository with nothing to report emits none. The storage seam is
//! faked at the diagnostics source (plan D-B2-6) — the engine routing
//! is the behavior under test, not the quarantine itself (pinned in the
//! storage crate).
//!
//! Determinism: the boot hop is synchronous inside `spawn_state_engine`
//! (before the loop spawns), so the broadcast is observable by a plain
//! drain right after spawn — no timers, no sleeps.

use std::sync::Arc;

use taskboard_domain::persistence::{
    BoxFuture, CorruptionDetail, DecodeReason, DecodeStage, LoadDiagnostics, LoadDiagnosticsSource,
    PersistedState, PersistenceAction, RepositoryError, TaskRepository,
};
use taskboard_domain::test_support::InMemoryRepository;
use taskboard_domain::{
    Clock, IdGenerator, NotifierMessage, StateCommand, SyncCommand, SystemEvent, UserDataReport,
};
use taskboard_state::{DayRollConfig, spawn_state_engine};

/// A healthy in-memory repository that reports a degraded boot load —
/// exactly what the sqlite actor publishes after quarantining rows.
#[derive(Debug)]
struct DegradedRepository {
    inner: InMemoryRepository,
    diagnostics: std::sync::Mutex<Option<LoadDiagnostics>>,
}

impl DegradedRepository {
    fn degraded() -> Self {
        Self {
            inner: InMemoryRepository::with_state(PersistedState::default()),
            diagnostics: std::sync::Mutex::new(Some(LoadDiagnostics {
                details: vec![CorruptionDetail {
                    table: Some(taskboard_domain::persistence::PersistedTable::CommandJournal),
                    row: Some(uuid::Uuid::from_u128(0x0bad)),
                    stage: Some(DecodeStage::Payload),
                    reason: Some(DecodeReason::SerdePayload),
                }],
                dropped: 1,
                head: None,
            })),
        }
    }
}

impl TaskRepository for DegradedRepository {
    fn load(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        self.inner.load()
    }

    fn apply(&self, actions: Vec<PersistenceAction>) -> BoxFuture<'_, Result<(), RepositoryError>> {
        self.inner.apply(actions)
    }

    fn load_diagnostics(&self) -> Option<&dyn LoadDiagnosticsSource> {
        Some(self)
    }
}

impl LoadDiagnosticsSource for DegradedRepository {
    fn take_load_diagnostics(&self) -> Option<LoadDiagnostics> {
        self.diagnostics.lock().expect("diagnostics mutex").take()
    }
}

async fn spawn_with(
    repo: Arc<dyn TaskRepository>,
) -> (
    taskboard_state::EngineHandle,
    tokio::sync::broadcast::Receiver<SystemEvent>,
) {
    let ids: Arc<dyn IdGenerator> = Arc::new(taskboard_domain::test_support::CountingIds::new());
    let clock: Arc<dyn Clock> = Arc::new(taskboard_domain::test_support::FrozenClock::from_secs(0));
    let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (_report_tx, report_rx) = tokio::sync::mpsc::channel::<taskboard_domain::SyncReport>(8);
    let (_user_data_tx, _user_data) = tokio::sync::mpsc::channel::<UserDataReport>(4);
    let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
    // Keep the pre-spawn receiver: the boot hop broadcasts before
    // `spawn_state_engine` returns, so a later `subscribe` misses it.
    let (notifier, _notifications) = tokio::sync::mpsc::channel::<NotifierMessage>(8);
    let (handle, _join) = spawn_state_engine(
        repo,
        ids,
        clock,
        sync_out,
        report_rx,
        system_tx.clone(),
        system_rx.resubscribe(),
        notifier,
        taskboard_domain::SettingsSnapshot::default(),
        DayRollConfig::disabled(),
        None,
    )
    .await
    .expect("engine boots");
    (handle, system_rx)
}

/// Drains the system feed synchronously: the boot hop broadcast (and any
/// other boot-window broadcast) landed before `spawn_state_engine`
/// returned, so the drain is complete without waiting.
fn drain(system_rx: &mut tokio::sync::broadcast::Receiver<SystemEvent>) -> Vec<SystemEvent> {
    let mut events = Vec::new();
    loop {
        match system_rx.try_recv() {
            Ok(event) => events.push(event),
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                panic!("system feed lagged at boot");
            }
            Err(_) => return events,
        }
    }
}

/// Case 10 — the engine routes the storage-published diagnostics as one
/// typed `JournalDegraded` event and still boots on the repository's
/// state.
#[tokio::test]
async fn a_degraded_boot_emits_one_typed_journal_degraded_event() {
    let (handle, mut system_rx) = spawn_with(Arc::new(DegradedRepository::degraded())).await;
    let events = drain(&mut system_rx);
    let degraded: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            SystemEvent::JournalDegraded {
                details,
                dropped,
                head,
            } => Some((details.clone(), *dropped, *head)),
            _ => None,
        })
        .collect();
    assert_eq!(
        degraded.len(),
        1,
        "one degraded event per degraded load, got {events:?}"
    );
    let (details, dropped, head) = &degraded[0];
    assert_eq!(*dropped, 1);
    assert_eq!(details.len(), 1);
    assert_eq!(
        details[0].table,
        Some(taskboard_domain::persistence::PersistedTable::CommandJournal)
    );
    assert_eq!(details[0].stage, Some(DecodeStage::Payload));
    assert_eq!(*head, None);
    // The engine booted on the repository's (healthy) state: the
    // quarantine degraded only the undo line, never the boot.
    let _state = handle.shared_state().load();
}

/// Case 11 — a repository with nothing to report emits no degraded
/// event: the healthy boot path is untouched (round law c).
#[tokio::test]
async fn a_healthy_boot_emits_no_journal_degraded_event() {
    let (_handle, mut system_rx) = spawn_with(Arc::new(InMemoryRepository::with_state(
        PersistedState::default(),
    )))
    .await;
    for event in drain(&mut system_rx) {
        assert!(
            !matches!(event, SystemEvent::JournalDegraded { .. }),
            "a healthy boot must not emit JournalDegraded, got {event:?}"
        );
    }
}

/// The faked repository stays `TaskRepository`-shaped for the whole
/// suite: a `StateCommand` round trip through the booted engine still
/// works on a degraded boot (the line is degraded, never the engine).
#[tokio::test]
async fn a_degraded_boot_still_executes_commands() {
    let (handle, mut system_rx) = spawn_with(Arc::new(DegradedRepository::degraded())).await;
    drain(&mut system_rx);
    let outcome = handle
        .execute(StateCommand::RequestSync)
        .await
        .expect("command executes");
    // The engine answers with a typed outcome (any variant — the pin is
    // that the degraded boot did not brick command execution).
    let _ = outcome;
}
