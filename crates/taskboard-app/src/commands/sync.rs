// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard sync` (ADR 0009): trigger one full cycle, wait for it
//! bounded, and report the status honestly. The boot cycle usually
//! coalesces with the explicit nudge; the wait decides on *fresh*
//! evidence only (a `last_success` advance, a fresh `Failed` phase) —
//! a fresh failure identical to a stale persisted status publishes no
//! state change and honestly resolves as [`SyncOutcome::TimedOut`].

use std::sync::Arc;
use std::time::{Duration, Instant};

use taskboard_domain::{StateCommand, SyncErrorKind, SyncPhase, SyncStatus};

use crate::commands::CommandError;
use crate::commands::output::SyncStatusView;
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// Outcome of awaiting one fresh sync cycle after the startup status.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncOutcome {
    /// A cycle completed: `last_success` advanced past the startup
    /// status.
    Succeeded(SyncStatusView),
    /// A fresh `Failed` phase was observed (or the deadline hit while
    /// the phase was `Failed`); carries the failure classification for
    /// the stderr report next to the status payload.
    Failed(SyncErrorKind, SyncStatusView),
    /// The deadline expired without fresh evidence; the final status is
    /// reported either way.
    TimedOut(SyncStatusView),
    /// `--no-wait`: the actor was nudged, the current status is
    /// reported, no cycle was awaited. (Extension of the plan's
    /// three-arm enum: the honest payload for the skip-the-wait path.)
    NoWait(SyncStatusView),
}

impl SyncOutcome {
    /// The status payload of the outcome.
    #[must_use]
    pub fn view(&self) -> &SyncStatusView {
        match self {
            Self::Succeeded(view)
            | Self::Failed(_, view)
            | Self::TimedOut(view)
            | Self::NoWait(view) => view,
        }
    }
}

/// Boots the core, snapshots the startup status, nudges the actor, and
/// (unless `no_wait`) waits for the fresh cycle.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn run(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    no_wait: bool,
    timeout: Duration,
    connector: Option<&str>,
) -> Result<SyncOutcome, CommandError> {
    crate::commands::require_connector(&config)?;
    // The scope resolves after the gate (D6-5): `--connector` names a
    // configured connector or refuses typed before the core boots.
    let scope = connector
        .map(|name| crate::commands::resolve_connector(&config, Some(name)))
        .transpose()?
        .map(|spec| spec.name);
    let sink = crate::bootstrap::resolve_notifier_sink(&config);
    let app = crate::bootstrap::bootstrap(config, store, sink).await?;
    let result = sync_on(&app, no_wait, timeout, scope.as_ref()).await;
    let _ = app.shutdown().await;
    result
}

/// The watched sync status of one poll: the connector-scoped fold over
/// the *currently*-bound boards' rows when scoped (bindings re-derived
/// each poll — a mid-wait move changes the watched set honestly,
/// D6-6's recorded semantics), else the back-compat all-boards
/// aggregate. The map fold itself is always the domain's
/// `aggregate_rows` — never duplicated here.
fn watched_status(
    app: &crate::App,
    scope: Option<&taskboard_domain::ConnectorId>,
) -> taskboard_domain::SyncStatus {
    let state = app.engine.shared_state().load_full();
    scoped_sync_status(&state, scope)
}

/// The pure half of [`watched_status`], unit-testable over a
/// hand-built state.
fn scoped_sync_status(
    state: &taskboard_domain::AppState,
    scope: Option<&taskboard_domain::ConnectorId>,
) -> SyncStatus {
    match scope {
        Some(connector) => {
            let rows: Vec<SyncStatus> = state
                .boards
                .values()
                .filter(|b| {
                    !b.deleted && b.remote.as_ref().is_some_and(|r| &r.connector == connector)
                })
                .filter_map(|b| state.sync.get(b.id).cloned())
                .collect();
            taskboard_domain::aggregate_rows(rows.iter())
        }
        None => state.sync.aggregate(),
    }
}

/// The sync steps on a booted core; the caller owns the shutdown (the
/// session dispatcher calls this directly against the session core).
pub(crate) async fn sync_on(
    app: &crate::App,
    no_wait: bool,
    timeout: Duration,
    scope: Option<&taskboard_domain::ConnectorId>,
) -> Result<SyncOutcome, CommandError> {
    // Hydrated from the db — may be a stale `Idle`/`Failed`.
    let startup = watched_status(app, scope);
    if no_wait {
        let view = SyncStatusView::new(&watched_status(app, scope));
        return Ok(SyncOutcome::NoWait(view));
    }
    // Belt-and-braces nudge; the boot cycle coalesces with it. A
    // connector-less engine (standalone gate passed on a booted session
    // whose connector was detached, WI-H) answers `NoOp` — refuse typed
    // instead of waiting out the deadline for evidence that can never
    // arrive.
    let outcome = app.engine.execute(StateCommand::RequestSync).await?;
    if outcome == taskboard_domain::CommandOutcome::NoOp {
        return Err(CommandError::SyncUnavailable);
    }
    Ok(wait_for_sync_result(app, &startup, timeout, scope).await)
}

/// Event-driven (with a polling fallback) predicate poll with a
/// wall-clock deadline, shared by `sync` and — via
/// `boards::wait_for_adoption`'s sibling loop — the binding ceremony.
///
/// Decision table (against the startup status `S0`):
/// - `Succeeded` when `last_success` advanced past `S0`;
/// - `Failed` when a fresh `Failed` phase is observed (any status
///   differing from `S0`, or after a state change was published);
/// - `TimedOut` at the deadline (`Failed` if the phase is `Failed` at
///   that point — a fresh failure identical to a stale persisted `S0`
///   publishes nothing and is indistinguishable from "still running").
pub async fn wait_for_sync_result(
    app: &crate::App,
    startup: &SyncStatus,
    deadline: Duration,
    scope: Option<&taskboard_domain::ConnectorId>,
) -> SyncOutcome {
    let mut signals = app.engine.subscribe();
    let mut seen_change = false;
    let deadline_at = Instant::now() + deadline;
    loop {
        let status = watched_status(app, scope);
        if status.last_success > startup.last_success {
            return SyncOutcome::Succeeded(SyncStatusView::new(&status));
        }
        let fresh_failure =
            matches!(status.phase, SyncPhase::Failed { .. }) && (seen_change || status != *startup);
        if fresh_failure {
            let SyncPhase::Failed { last_error } = status.phase else {
                unreachable!("checked by fresh_failure");
            };
            return SyncOutcome::Failed(last_error, SyncStatusView::new(&status));
        }
        if Instant::now() >= deadline_at {
            tracing::warn!("sync cycle did not produce fresh evidence in time");
            return match status.phase {
                SyncPhase::Failed { last_error } => {
                    SyncOutcome::Failed(last_error, SyncStatusView::new(&status))
                }
                _ => SyncOutcome::TimedOut(SyncStatusView::new(&status)),
            };
        }
        // Signals prompt an early re-check; the 200 ms poll is the
        // fallback if events lag.
        if tokio::time::timeout(Duration::from_millis(200), signals.recv())
            .await
            .is_ok()
        {
            seen_change = true;
        }
    }
}

/// The human-readable sync outcome.
#[must_use]
pub fn render_text(outcome: &SyncOutcome) -> String {
    match outcome {
        SyncOutcome::Succeeded(view) => format!(
            "sync succeeded (last success {:?}, {} pending)",
            view.last_success, view.pending_ops
        ),
        SyncOutcome::Failed(kind, view) => format!(
            "sync failed ({}) — {} pending",
            crate::commands::output::sync_error_kind(*kind),
            view.pending_ops
        ),
        SyncOutcome::TimedOut(view) => format!(
            "sync did not finish in time — phase: {}, {} pending",
            view.phase, view.pending_ops
        ),
        SyncOutcome::NoWait(view) => format!(
            "sync nudged — phase: {}, {} pending",
            view.phase, view.pending_ops
        ),
    }
}

#[cfg(test)]
mod scope_tests {
    use super::scoped_sync_status;
    use taskboard_domain::ids::BoardId;
    use taskboard_domain::{
        AppState, Board, BoardClocks, ConnectorId, RemoteBoardBinding, RemoteBoardId,
        SyncErrorKind, SyncPhase, SyncStates, SyncStatus,
    };

    fn board(raw: u128, tag: &str, remote: u64) -> (BoardId, Board) {
        let id = BoardId::from(uuid::Uuid::from_u128(raw));
        (
            id,
            Board {
                id,
                remote: Some(RemoteBoardBinding {
                    connector: ConnectorId::from(tag),
                    board: RemoteBoardId(remote),
                }),
                title: "b".to_owned(),
                color: taskboard_domain::Color::new("0082c9"),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: BoardClocks::epoch(),
                    color: BoardClocks::epoch(),
                    archived: BoardClocks::epoch(),
                    deleted: BoardClocks::epoch(),
                },
                remote_seen: None,
            },
        )
    }

    fn status(phase: SyncPhase, pending: u32) -> SyncStatus {
        SyncStatus {
            counters: taskboard_domain::SyncCounters::default(),
            phase,
            last_success: None,
            pending_ops: pending,
            ..SyncStatus::default()
        }
    }

    /// The scoped fold: only the named connector's currently-bound live
    /// boards' rows fold (D6-6); the unscoped call is the all-boards
    /// aggregate, unchanged.
    #[test]
    fn the_scoped_fold_watches_only_the_named_connectors_boards() {
        let mut state = AppState::default();
        let (a1, row) = board(0xA1, "alpha", 1);
        state.boards.insert(a1, row);
        let (a2, row) = board(0xA2, "alpha", 2);
        state.boards.insert(a2, row);
        let (b1, row) = board(0xB1, "beta", 1);
        state.boards.insert(b1, row);
        let mut sync = SyncStates::new();
        sync.set_stored(
            a1,
            SyncPhase::Failed {
                last_error: SyncErrorKind::Network,
            },
            None,
        );
        sync.set_stored(a2, SyncPhase::Syncing, None);
        sync.set_stored(b1, SyncPhase::Idle, None);
        state.sync = sync;

        let alpha = ConnectorId::from("alpha");
        let beta = ConnectorId::from("beta");
        let scoped = scoped_sync_status(&state, Some(&alpha));
        assert_eq!(scoped.phase, SyncPhase::Syncing, "alpha's dominance");
        assert_eq!(scoped.pending_ops, 0);
        assert_eq!(
            scoped_sync_status(&state, Some(&beta)),
            status(SyncPhase::Idle, 0),
            "beta unaffected by alpha's rows"
        );
        assert_eq!(
            scoped_sync_status(&state, None),
            state.sync.aggregate(),
            "the unscoped watch is the back-compat aggregate"
        );

        // The watch re-derives from current bindings: a board moved to
        // beta changes alpha's watched set on the next poll.
        state.boards.get_mut(&a2).expect("live").remote = Some(RemoteBoardBinding {
            connector: beta.clone(),
            board: RemoteBoardId(2),
        });
        let scoped = scoped_sync_status(&state, Some(&alpha));
        assert_eq!(
            scoped.phase,
            SyncPhase::Failed {
                last_error: SyncErrorKind::Network
            },
            "a1's row only — a2 moved to beta"
        );
    }
}

#[cfg(test)]
mod detach_tests {
    use super::*;
    use crate::bootstrap::DetachOutcome;
    use crate::config::AppConfig;
    use crate::secrets::{AppPassword, CredentialError, CredentialStore};

    /// A fixed store: the fake the bootstrap doc example uses.
    #[derive(Debug)]
    struct FixedStore(crate::secrets::AppPassword);

    impl CredentialStore for FixedStore {
        fn load(&self, _: &str, _: &str) -> Result<AppPassword, CredentialError> {
            Ok(self.0.clone())
        }
        fn store(&self, _: &str, _: &str, _: AppPassword) -> Result<(), CredentialError> {
            Err(CredentialError::Unsupported)
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), CredentialError> {
            Err(CredentialError::Unsupported)
        }
    }

    /// WI-H tier 5: on a detached core, `sync_on` refuses typed
    /// `SyncUnavailable` instead of nudging a dead channel and lying
    /// via `SyncTimedOut`. The pre-detach path is unchanged (the
    /// nudge answers `SyncRequested` — pinned at the B/engine tiers).
    #[tokio::test]
    async fn sync_on_a_detached_core_refuses_sync_unavailable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut config = AppConfig::default();
        config.nextcloud.server_url = Some("http://127.0.0.1:9".into());
        config.nextcloud.username = Some("alice".into());
        config.storage.db_path = Some(dir.path().join("taskboard.db"));
        config.app.shutdown_timeout_secs = 2;
        config.validate().expect("valid");
        let store: std::sync::Arc<dyn CredentialStore> =
            std::sync::Arc::new(FixedStore(AppPassword::new("pw")));
        let app =
            crate::bootstrap::bootstrap(config, store, Arc::new(crate::notifications::LogSink))
                .await
                .expect("boots");

        assert_eq!(app.detach_connector().await, DetachOutcome::Detached);

        let outcome = sync_on(&app, false, Duration::from_secs(1), None).await;
        assert!(
            matches!(outcome, Err(CommandError::SyncUnavailable)),
            "expected the typed refusal, got {outcome:?}"
        );

        let _ = app.shutdown().await;
    }
}
