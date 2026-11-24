// SPDX-License-Identifier: AGPL-3.0-only
//! Crate-internal test-support fixtures (plan
//! `2026-11-18-journey-fixture-seams` §D4): the shared
//! standalone-boot shape — a seeded config file, a tempdir-backed
//! database, and the credential-refusing store — booting a real
//! engine. The tempdir guard lives in [`StandaloneFixture`], so the
//! paths are valid exactly as long as the assertion window — no
//! leaked guard on this path (#914).

#![forbid(unsafe_code)]

use std::sync::Arc;

use crate::config::{AppConfig, SyncBackend};

/// A credential-refusing store (the bootstrap test's shape): the
/// config arm never consults it. Shared by every fixture consumer so
/// the refusal semantics stay one definition.
#[derive(Debug)]
pub(crate) struct RefusingStore;

/// Convenience constructor: one `Arc<dyn CredentialStore>` per call
/// site, killing the repeated `Arc::new(RefusingStore)` spell (#981).
pub(crate) fn refusing_store() -> Arc<dyn crate::secrets::CredentialStore> {
    Arc::new(RefusingStore)
}

impl crate::secrets::CredentialStore for RefusingStore {
    fn load(
        &self,
        _: &str,
        _: &str,
    ) -> Result<crate::secrets::AppPassword, crate::secrets::CredentialError> {
        Err(crate::secrets::CredentialError::NotFound)
    }
    fn store(
        &self,
        _: &str,
        _: &str,
        _: crate::secrets::AppPassword,
    ) -> Result<(), crate::secrets::CredentialError> {
        Err(crate::secrets::CredentialError::Unsupported)
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), crate::secrets::CredentialError> {
        Err(crate::secrets::CredentialError::Unsupported)
    }
}

/// A booted standalone core plus everything it borrows from disk: the
/// tempdir guard is owned here, so the seeded config and the database
/// live exactly as long as the fixture binding does. Plain fields —
/// no `Deref`, no cleverness.
pub(crate) struct StandaloneFixture {
    /// The tempdir backing the config file and the database; drops
    /// with the fixture (the #914 leak's replacement). Never read —
    /// held for its `Drop` side effect, hence the underscore.
    #[allow(dead_code)]
    pub(crate) dir: tempfile::TempDir,
    /// The seeded `taskboard-{tag}.toml` inside [`Self::dir`].
    pub(crate) config_path: std::path::PathBuf,
    /// The booted core.
    pub(crate) app: crate::bootstrap::App,
}

/// The shared standalone-boot fixture (the `ui_requests` harness
/// shape — fakes over mocks; a real engine): seeds
/// `taskboard-{tag}.toml` with the reminders tick, points the storage
/// at the fixture's tempdir, and boots the standalone core against
/// [`RefusingStore`].
// ---- The predicate-engine fixture (plan `2026-11-22-commands-predicate-coverage` §2) ----
//
// The commands-layer decision predicates (selector ambiguity triage,
// outcome mapping, freshness/wait predicates) need the real engine over
// a *seeded* persisted state at unit cost. The fixture composes only
// existing domain fakes (`InMemoryRepository::with_state`,
// `CountingIds`) plus one shared frozen clock — the wiring is the only
// new code (the #981 dedup law: no per-test-module local doubles).
use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{
    Clock, IdGenerator, NotifierMessage, SettingsSnapshot, SyncCommand, SyncReport, SystemEvent,
    TaskRepository,
};
use taskboard_state::{DayRollConfig, spawn_state_engine};

/// The fixture's frozen instant (2023-11-14T22:13:20Z). Deterministic —
/// the clock-discipline law: no fixture output may depend on the wall
/// clock (`testing_strategy` §6.1–6.2).
#[must_use]
#[allow(dead_code)] // consumed by the predicate tests landing in this chunk
pub(crate) fn frozen_now() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(1_700_000_000, 0).expect("the frozen instant exists")
}

/// The real engine over the seeded in-memory repo, with quiet lanes.
/// The fixture owns every lane half the engine holds a receiver for, so
/// nothing closes early; `shutdown` signals and joins.
#[allow(dead_code)] // consumed by the predicate tests landing in this chunk
pub(crate) struct PredicateEngineFixture {
    /// The engine handle (the predicates' only input surface).
    pub(crate) engine: taskboard_state::EngineHandle,
    /// The system broadcast's sender — tests arm `seen_change`-class
    /// signals by sending here (the engine's `subscribe()` consumers
    /// observe it).
    pub(crate) system: tokio::sync::broadcast::Sender<SystemEvent>,
    sync_out: tokio::sync::mpsc::Sender<SyncCommand>,
    reports: tokio::sync::mpsc::Sender<SyncReport>,
    notifier: tokio::sync::mpsc::Sender<NotifierMessage>,
    join: tokio::task::JoinHandle<()>,
}

/// Spawns the real state engine over `state` (the sanctioned fake
/// repository) with [`CountingIds`] and the frozen clock. Quiet lanes:
/// no sync actor is wired, so `RequestSync` answers `NoOp` — the wait
/// predicates under test take the engine handle directly.
///
/// # Panics
///
/// Via `expect` when the engine fails to boot — the in-memory repo
/// cannot corrupt, so a boot failure is a fixture bug, not test data.
#[allow(dead_code)] // consumed by the predicate tests landing in this chunk
pub(crate) async fn predicate_engine(
    state: taskboard_domain::PersistedState,
) -> PredicateEngineFixture {
    let repo: Arc<dyn TaskRepository> = Arc::new(InMemoryRepository::with_state(state));
    let ids: Arc<dyn IdGenerator> = Arc::new(CountingIds::new());
    let clock: Arc<dyn Clock> = Arc::new(taskboard_domain::test_support::FrozenClock::new(
        frozen_now(),
    ));
    let (sync_out, sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
    let (reports, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
    let (system, system_rx_parked) = tokio::sync::broadcast::channel::<SystemEvent>(64);
    let system_rx = system.subscribe();
    let (notifier, notifications_rx) = tokio::sync::mpsc::channel::<NotifierMessage>(64);
    let (engine, join) = spawn_state_engine(
        repo,
        ids,
        clock,
        sync_out.clone(),
        report_rx,
        system.clone(),
        system_rx,
        notifier.clone(),
        SettingsSnapshot::default(),
        DayRollConfig::disabled(),
        None,
    )
    .await
    .expect("the in-memory repo boots");
    // Park the receiving halves in this scope tuple so nothing closes
    // while the engine runs (a closed lane is inert by contract, but
    // the fixture's quiet-lane shape keeps them open).
    std::mem::forget((sync_in, notifications_rx, system_rx_parked));
    PredicateEngineFixture {
        engine,
        system,
        sync_out,
        reports,
        notifier,
        join,
    }
}

impl PredicateEngineFixture {
    /// The published state snapshot (the predicates' `&AppState` input).
    #[allow(dead_code)] // consumed by the predicate tests landing in this chunk
    pub(crate) fn state(&self) -> taskboard_domain::AppState {
        (*self.engine.shared_state().load_full()).clone()
    }

    /// Signals shutdown and joins the actor (the fixture's teardown).
    /// The lane senders drop first: the drain ends when the reports
    /// channel closes, so a parked sender here would deadlock the join
    /// (the bootstrap's own shutdown law, `App::shutdown`).
    #[allow(dead_code)] // consumed by the predicate tests landing in this chunk
    pub(crate) async fn shutdown(self) {
        let PredicateEngineFixture {
            engine,
            system,
            sync_out,
            reports,
            notifier,
            join,
        } = self;
        drop((engine, sync_out, reports, notifier));
        let _ = system.send(SystemEvent::Shutdown);
        drop(system);
        let _ = join.await;
    }
}

// ---- Seed mints (the PersistedState composers) -----------------------------
//
// Fixed-u128 raw ids keep the seeded graphs' identities legible in the
// assertions. Rows mint all-epoch clocks (remote-wins semantics, the
// same base the snapshot strategies use) and are open structs — tests
// edit fields directly instead of growing a parameter per flag.

/// A live board from `raw`: the domain mint with the seed family's
/// hex-suffixed title convention (#1269).
#[must_use]
#[allow(dead_code)] // consumed by the predicate tests landing in this chunk
pub(crate) fn seed_board(raw: u128) -> taskboard_domain::Board {
    taskboard_domain::test_support::mint_board(raw, &format!("board {raw:x}"))
}

/// A live stack of `board` from `raw`, at `order`: the domain mint
/// with the seed family's hex-suffixed title and the caller's order
/// (#1269).
#[must_use]
#[allow(dead_code)] // consumed by the predicate tests landing in this chunk
pub(crate) fn seed_stack(
    raw: u128,
    board: taskboard_domain::BoardId,
    order: i64,
) -> taskboard_domain::Stack {
    let mut stack =
        taskboard_domain::test_support::mint_stack(raw, board, &format!("stack {raw:x}"));
    stack.order = order;
    stack
}

/// A live, open task of `stack` from `raw`, at `order`: the domain
/// mint with the seed family's hex-suffixed title and the caller's
/// order (#1269).
#[must_use]
#[allow(dead_code)] // consumed by the predicate tests landing in this chunk
pub(crate) fn seed_task(
    raw: u128,
    stack: taskboard_domain::StackId,
    order: i64,
) -> taskboard_domain::entities::Task {
    let mut task = taskboard_domain::test_support::mint_task(raw, stack, &format!("task {raw:x}"));
    task.order = order;
    task
}

/// A live label of `board` from `raw`: the domain mint with the seed
/// family's hex-suffixed title convention (#1269).
#[must_use]
#[allow(dead_code)] // consumed by the predicate tests landing in this chunk
pub(crate) fn seed_label(raw: u128, board: taskboard_domain::BoardId) -> taskboard_domain::Label {
    taskboard_domain::test_support::mint_label(raw, board, &format!("label {raw:x}"))
}

pub(crate) async fn standalone_boot_fixture(tag: &str) -> StandaloneFixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join(format!("taskboard-{tag}.toml"));
    tokio::fs::write(&config_path, "[reminders]\ntick_secs = 30\n")
        .await
        .expect("seed config");
    let mut config = AppConfig::default();
    config.storage.db_path = Some(dir.path().join("taskboard.db"));
    config.sync.backend = Some(SyncBackend::Standalone);
    config.validate().expect("config valid");
    let app = crate::bootstrap::bootstrap_in(
        config,
        refusing_store(),
        Arc::new(crate::notifications::LogSink),
        crate::bootstrap::BootIdentity::At(dir.path().to_owned()),
    )
    .await
    .expect("standalone boot");
    StandaloneFixture {
        dir,
        config_path,
        app,
    }
}
