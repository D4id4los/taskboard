// SPDX-License-Identifier: MIT OR Apache-2.0
//! D-series: the connector-only detach (WI-H). The actor's `Detach`
//! inbox command stops the actor gracefully — a cycle queued before it
//! completes first, the reports channel closes after the last report,
//! and the task finishes — without the `Shutdown` broadcast (which
//! would drain the engine, not just the connector).

use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::{
    PersistedState, RepositoryError, SyncCommand, SyncErrorKind, SyncReport, SyncStateReader,
    SystemEvent,
};
use taskboard_sync_nextcloud::{DeckClient, SyncActorConfig, spawn_sync_actor};
use tokio::sync::{broadcast, mpsc};

/// A boardless engine view: every cycle's read succeeds and finds no
/// bound board — commanded cycles report `NoBoard` without touching the
/// network (the client's URI is a dark port on purpose).
#[derive(Debug)]
struct BoardlessView;

impl SyncStateReader for BoardlessView {
    fn read_state(
        &self,
    ) -> taskboard_domain::BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        Box::pin(async { Ok(PersistedState::default()) })
    }
}

#[tokio::test]
async fn d1_detach_stops_the_actor_after_the_queued_cycle() {
    let (commands, command_rx) = mpsc::channel::<SyncCommand>(16);
    let (reports, mut report_rx) = mpsc::channel::<SyncReport>(16);
    let (system, _system_rx) = broadcast::channel::<SystemEvent>(16);
    let client = DeckClient::new("http://127.0.0.1:9", "u", "t").expect("valid base url");
    let cfg = SyncActorConfig {
        poll_interval: Duration::from_secs(3600),
        backoff_initial: Duration::from_secs(1),
        backoff_max: Duration::from_secs(1),
    };
    let join = spawn_sync_actor(
        client,
        Arc::new(BoardlessView),
        command_rx,
        reports,
        system,
        cfg,
    );

    // Queued strictly before the detach: the commanded cycle must
    // complete (and report) before the actor exits.
    commands.send(SyncCommand::SyncNow).await.expect("send");
    commands.send(SyncCommand::Detach).await.expect("send");
    drop(commands);

    // The queued cycle's evidence lands first (commanded + boardless ⇒
    // a `NoBoard` failure report), then the reports channel closes —
    // the actor returned and dropped its sender.
    let report = report_rx.recv().await.expect("the queued cycle reports");
    assert!(
        matches!(
            report,
            SyncReport::Failed {
                kind: SyncErrorKind::NoBoard,
                ..
            }
        ),
        "expected the queued cycle's NoBoard report, got {report:?}"
    );
    assert!(
        report_rx.recv().await.is_none(),
        "the reports channel closes when the detached actor exits"
    );

    tokio::time::timeout(Duration::from_secs(5), join)
        .await
        .expect("the actor finishes on Detach")
        .expect("actor task joins cleanly");
}
