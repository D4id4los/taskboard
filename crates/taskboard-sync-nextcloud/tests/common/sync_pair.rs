// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tier 2 harness for the D-series: the full engine + sync-actor pair
//! (the Phase 5 channel graph) over the in-memory repository, pointed at
//! the dockerized Nextcloud. Assertion surface is the engine's published
//! `AppState` plus raw `DeckClient` reads — never actor internals.
//!
//! Not every D-test uses every helper.
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::test_support::{CountingIds, InMemoryRepository};
use taskboard_domain::{AppState, Clock, SyncCommand, SyncPhase, SystemEvent};
use taskboard_state::spawn_state_engine;
use taskboard_sync_nextcloud::{DeckClient, SyncActorConfig, spawn_sync_actor};
use tokio::sync::{broadcast, mpsc};

use super::LIVE_DEADLINE;

const CFG: SyncActorConfig = SyncActorConfig {
    poll_interval: Duration::from_millis(300),
    backoff_initial: Duration::from_millis(200),
    backoff_max: Duration::from_millis(800),
};

/// The engine + actor pair over one shared fake repository.
pub(crate) struct SyncPair {
    pub engine: taskboard_state::EngineHandle,
    engine_join: tokio::task::JoinHandle<()>,
    pub repo: Arc<InMemoryRepository>,
    commands: mpsc::Sender<SyncCommand>,
    _system_tx: broadcast::Sender<SystemEvent>,
}

/// Boots the pair against `client` (a live-tier Deck client).
pub(crate) async fn boot_pair(client: DeckClient) -> SyncPair {
    let repo = Arc::new(InMemoryRepository::new());
    let ids: Arc<dyn taskboard_domain::IdGenerator> = Arc::new(CountingIds::new());
    let clock: Arc<dyn Clock> = Arc::new(taskboard_domain::SystemClock);
    let (nudges_tx, nudges_rx) = mpsc::channel::<SyncCommand>(16);
    let (report_tx, report_rx) = mpsc::channel(16);
    let (system_tx, system_engine_rx) = broadcast::channel(16);

    let (engine, engine_join) = spawn_state_engine(
        repo.clone(),
        ids,
        clock,
        nudges_tx.clone(),
        report_rx,
        system_engine_rx,
    )
    .await
    .expect("engine boots");

    let state_reader: Arc<dyn taskboard_domain::SyncStateReader> = Arc::new(engine.clone());
    let _actor = spawn_sync_actor(
        client,
        state_reader,
        nudges_rx,
        report_tx,
        system_tx.clone(),
        CFG,
    );

    SyncPair {
        engine,
        engine_join,
        repo,
        // The actor's command inlet: engine nudges and test-driven
        // SetBoard/nudges share it (the production graph).
        commands: nudges_tx,
        _system_tx: system_tx,
    }
}

impl SyncPair {
    /// Arms the pull target (the CLI `boards select` path, Phase 6).
    pub(crate) async fn set_board(&self, board: u64) {
        self.commands
            .send(SyncCommand::SetBoard(taskboard_domain::RemoteBoardId(
                board,
            )))
            .await
            .expect("actor command channel open");
    }

    /// Nudges a cycle (the engine does this after every mutating command).
    pub(crate) async fn sync_now(&self) {
        self.commands
            .send(SyncCommand::SyncNow)
            .await
            .expect("actor command channel open");
    }

    /// Stops the pair (engine last, per the Phase 5 shutdown ordering).
    pub(crate) async fn shutdown(self) {
        self.engine.flush().await;
        self.engine_join.abort();
    }
}

/// Polls the published state until `pred` holds (deadline-guarded).
pub(crate) async fn eventually(pair: &SyncPair, pred: impl Fn(&AppState) -> bool) {
    tokio::time::timeout(LIVE_DEADLINE, async {
        loop {
            if pred(&pair.engine.shared_state().load_full()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("predicate satisfied within the live deadline");
}

/// The sync phase the published state reports.
pub(crate) fn phase(pair: &SyncPair) -> SyncPhase {
    pair.engine.shared_state().load_full().sync.phase
}
