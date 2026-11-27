// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard sync` (ADR 0009): trigger one full cycle, wait for it
//! bounded, and report the status honestly. The boot cycle usually
//! coalesces with the explicit nudge; the wait decides on *fresh*
//! evidence only (an advanced generation counter, a fresh `Failed`
//! phase) — a fresh failure identical to a stale persisted status
//! publishes no state change and honestly resolves as
//! [`SyncOutcome::TimedOut`].

use std::sync::Arc;
use std::time::{Duration, Instant};

use taskboard_domain::{StateCommand, SyncErrorKind, SyncPhase, SyncStatus, SystemEvent, TaskId};
use taskboard_state::EngineHandle;

use crate::commands::CommandError;
use crate::commands::output::SyncStatusView;
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// Outcome of awaiting one fresh sync cycle after the startup status.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncOutcome {
    /// A cycle completed: a sync report landed since the startup
    /// snapshot (the generation counter advanced).
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
    // The one-shot verb owns no `CommandIo` (the `run_core` body shape)
    // — the process lane is stdout, the same lane `println!` used.
    let mut stdout = std::io::stdout();
    let result = sync_on(&app, no_wait, timeout, scope.as_ref(), &mut stdout).await;
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
    engine: &EngineHandle,
    scope: Option<&taskboard_domain::ConnectorId>,
) -> taskboard_domain::SyncStatus {
    let state = engine.shared_state().load_full();
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
/// Resurrect surfacing writes to `out` — the caller's stdout/`io.out`
/// lane (the writer seam, #1229 plan D6: the `println!` shell bypassed
/// `CommandIo` and was uncapturable).
pub(crate) async fn sync_on(
    app: &crate::App,
    no_wait: bool,
    timeout: Duration,
    scope: Option<&taskboard_domain::ConnectorId>,
    out: &mut (dyn std::io::Write + Send),
) -> Result<SyncOutcome, CommandError> {
    if no_wait {
        let view = SyncStatusView::new(&watched_status(&app.engine, scope));
        return Ok(SyncOutcome::NoWait(view));
    }
    // The resurrect listener rides the system broadcast: subscribe
    // *before* the nudge so a resurrect born in the awaited cycle is
    // observable (the engine emits during report ingestion).
    let mut system_rx = app.system.subscribe();
    let outcome = sync_on_engine(&app.engine, timeout, scope).await?;
    report_resurrections(app, &mut system_rx, out);
    Ok(outcome)
}

/// The nudge + bounded wait on a running engine, no `App` in sight
/// (D-6.8 phase 2 D-3): the move ceremony's pull step runs inside the
/// UI process whose engine is already up — a fresh `App` bootstrap
/// would refuse on the process-lifetime instance lock. The CLI's
/// resurrect printing stays with [`sync_on`], which delegates here.
pub(crate) async fn sync_on_engine(
    engine: &EngineHandle,
    timeout: Duration,
    scope: Option<&taskboard_domain::ConnectorId>,
) -> Result<SyncOutcome, CommandError> {
    // Belt-and-braces nudge; the boot cycle coalesces with it. A
    // connector-less engine (standalone gate passed on a booted session
    // whose connector was detached, WI-H) answers `NoOp` — refuse typed
    // instead of waiting out the deadline for evidence that can never
    // arrive.
    // Hydrated from the db — may be a stale `Idle`/`Failed`; read
    // *before* the nudge so the wait decides on genuinely fresh
    // evidence (a cycle landing during the nudge is fresh).
    let startup = watched_status(engine, scope);
    let outcome = engine.execute(StateCommand::RequestSync).await?;
    if outcome == taskboard_domain::CommandOutcome::NoOp {
        return Err(CommandError::SyncUnavailable);
    }
    Ok(wait_for_sync_result(engine, &startup, timeout, scope).await)
}

/// The CLI's resurrect surfacing: drains the system broadcast and prints
/// one status line per resurrected task (never a prompt — a kiosk or
/// daemon session just reads its log). The titles resolve against the
/// *current* shared snapshot: the resurrected task is live and unbound
/// by the time the cycle settles.
fn report_resurrections(
    app: &crate::App,
    system_rx: &mut tokio::sync::broadcast::Receiver<SystemEvent>,
    out: &mut (dyn std::io::Write + Send),
) {
    let mut events = Vec::new();
    while let Ok(event) = system_rx.try_recv() {
        events.push(event);
    }
    let state = app.engine.shared_state().load_full();
    for id in resurrected_tasks(&events) {
        let title = state.tasks.get(&id).map_or("", |t| t.title.as_str());
        let _ = writeln!(
            out,
            "sync: task \"{title}\" held unsynced work; re-created as a new card"
        );
    }
}

/// The pure fold behind the surfacing: the resurrected ids in arrival
/// order, duplicates preserved (two episodes are two lines — the honest
/// report).
fn resurrected_tasks(events: &[SystemEvent]) -> Vec<TaskId> {
    events
        .iter()
        .filter_map(|event| match event {
            SystemEvent::TaskResurrected(id) => Some(*id),
            _ => None,
        })
        .collect()
}

/// The pure freshness classifier: has a cycle landed since the startup
/// snapshot, and what is its verdict? Freshness is *structural* — the
/// per-board `SyncCounters.cycles` generation counter, incremented once
/// per ingested report — never a comparison of two wall stamps, so a
/// backward host-clock step cannot suppress or fake it
/// (`last_success` is a display fact, not a freshness signal).
fn fresh_outcome(startup: &SyncStatus, status: &SyncStatus) -> Option<SyncOutcome> {
    let fresh = status.counters.cycles > startup.counters.cycles;
    if !fresh {
        return None; // keep waiting — no report has landed yet
    }
    match status.phase {
        SyncPhase::Failed { last_error } => {
            Some(SyncOutcome::Failed(last_error, SyncStatusView::new(status)))
        }
        // Some watched board is still in flight — patience.
        SyncPhase::Syncing => None,
        // Offline after a landed report: the cycle ended in a connector
        // outage — the episode's network verdict, not a success (the
        // engine holds `Offline` on a lost connector's boards for the
        // episode's duration, so this is the steady outage row, k7).
        SyncPhase::Offline => Some(SyncOutcome::Failed(
            SyncErrorKind::Network,
            SyncStatusView::new(status),
        )),
        // Idle after a landed report: the cycle is done and healthy.
        SyncPhase::Idle => Some(SyncOutcome::Succeeded(SyncStatusView::new(status))),
    }
}

/// Event-driven (with a polling fallback) predicate poll with a
/// wall-clock deadline, shared by `sync` and — via
/// `boards::wait_for_adoption`'s sibling loop — the binding ceremony.
///
/// Decision table (against the startup status `S0`):
/// - `Succeeded` when a report landed since `S0` (the generation
///   counter advanced) and no watched board is `Failed`/`Syncing`;
/// - `Failed` when the fresh cycle's verdict is `Failed`, or when a
///   fresh-failure signal arrives on a stale row (any status differing
///   from `S0`, or after a state change was published);
/// - `TimedOut` at the deadline (`Failed` if the phase is `Failed` at
///   that point — a fresh failure identical to a stale persisted `S0`
///   publishes nothing and is indistinguishable from "still running").
pub async fn wait_for_sync_result(
    engine: &EngineHandle,
    startup: &SyncStatus,
    deadline: Duration,
    scope: Option<&taskboard_domain::ConnectorId>,
) -> SyncOutcome {
    let mut signals = engine.subscribe();
    let mut seen_change = false;
    let deadline_at = Instant::now() + deadline;
    loop {
        let status = watched_status(engine, scope);
        if let Some(outcome) = fresh_outcome(startup, &status) {
            return outcome;
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

#[cfg(test)]
mod freshness_tests {
    use super::{SyncOutcome, fresh_outcome};
    use chrono::TimeZone;
    use taskboard_domain::{SyncCounters, SyncErrorKind, SyncPhase, SyncStatus};

    const T0: i64 = 36_000;

    fn status(
        cycles: u64,
        phase: SyncPhase,
        last_success: Option<chrono::DateTime<chrono::Utc>>,
    ) -> SyncStatus {
        SyncStatus {
            counters: SyncCounters {
                cycles,
                ..SyncCounters::default()
            },
            phase,
            last_success,
            ..SyncStatus::default()
        }
    }

    fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    /// Fresh + Idle is a completed cycle.
    #[test]
    fn fresh_idle_is_succeeded() {
        let startup = status(0, SyncPhase::Idle, None);
        let now = status(1, SyncPhase::Idle, Some(ts(T0)));
        assert!(matches!(
            fresh_outcome(&startup, &now),
            Some(SyncOutcome::Succeeded(_))
        ));
    }

    /// Fresh + Failed carries the fresh verdict.
    #[test]
    fn fresh_failed_is_failed() {
        let startup = status(0, SyncPhase::Idle, None);
        let now = status(
            1,
            SyncPhase::Failed {
                last_error: SyncErrorKind::Network,
            },
            None,
        );
        assert!(matches!(
            fresh_outcome(&startup, &now),
            Some(SyncOutcome::Failed(SyncErrorKind::Network, _))
        ));
    }

    /// Fresh but still in flight: some watched board hasn't reported.
    #[test]
    fn fresh_syncing_keeps_waiting() {
        let startup = status(0, SyncPhase::Idle, None);
        let now = status(1, SyncPhase::Syncing, None);
        assert_eq!(fresh_outcome(&startup, &now), None);
    }

    /// Fresh + Offline is the outage verdict (k7): the engine holds
    /// `Offline` on a lost connector's boards for the episode, so a
    /// cycle that lands there is a failed (network) cycle, never a
    /// success.
    #[test]
    fn fresh_offline_is_a_network_failure() {
        let startup = status(0, SyncPhase::Idle, None);
        let now = status(1, SyncPhase::Offline, None);
        assert!(matches!(
            fresh_outcome(&startup, &now),
            Some(SyncOutcome::Failed(SyncErrorKind::Network, _))
        ));
    }

    /// Equal cycles: nothing landed, whatever the wall stamps claim.
    #[test]
    fn stale_rows_keep_waiting_regardless_of_stamps() {
        let startup = status(3, SyncPhase::Idle, Some(ts(T0)));
        let advanced_stamp = status(3, SyncPhase::Idle, Some(ts(T0 + 60)));
        assert_eq!(fresh_outcome(&startup, &advanced_stamp), None);
    }

    /// The false-`TimedOut` regression fixture: a fresh cycle whose
    /// `last_success` moved *backward* (host clock stepped back). The
    /// pre-fix predicate `status.last_success > startup.last_success`
    /// is false here, so the old loop ran to the deadline and answered
    /// `TimedOut` for a completed, successful cycle. The generation
    /// counter sees the truth.
    #[test]
    fn a_backward_clock_step_does_not_hide_a_fresh_cycle() {
        let startup = status(0, SyncPhase::Idle, Some(ts(T0 + 3_600)));
        let now = status(1, SyncPhase::Idle, Some(ts(T0)));
        assert!(matches!(
            fresh_outcome(&startup, &now),
            Some(SyncOutcome::Succeeded(_))
        ));
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
                    board: RemoteBoardId::deck(remote),
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
                authors: taskboard_domain::entities::BoardAuthors::default(),
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
            board: RemoteBoardId::deck(2),
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

        let outcome = sync_on(
            &app,
            false,
            Duration::from_secs(1),
            None,
            &mut std::io::sink(),
        )
        .await;
        assert!(
            matches!(outcome, Err(CommandError::SyncUnavailable)),
            "expected the typed refusal, got {outcome:?}"
        );

        let _ = app.shutdown().await;
    }
}

#[cfg(test)]
mod resurrect_fold_tests {
    use super::resurrected_tasks;
    use taskboard_domain::SystemEvent;
    use taskboard_domain::{ConnectorId, TaskId};
    use uuid::Uuid;

    fn task_id(n: u128) -> TaskId {
        TaskId::from(Uuid::from_u128(n))
    }

    #[test]
    fn the_fold_yields_only_resurrected_ids_in_arrival_order() {
        let events = [
            SystemEvent::NetworkLost,
            SystemEvent::TaskResurrected(task_id(1)),
            SystemEvent::ConnectorLost(ConnectorId::from("nc")),
            SystemEvent::TaskResurrected(task_id(2)),
            SystemEvent::StorageFailed,
        ];
        assert_eq!(resurrected_tasks(&events), vec![task_id(1), task_id(2)]);
    }

    #[test]
    fn duplicates_are_preserved_two_episodes_are_two_lines() {
        let events = [
            SystemEvent::TaskResurrected(task_id(7)),
            SystemEvent::TaskResurrected(task_id(7)),
        ];
        assert_eq!(resurrected_tasks(&events), vec![task_id(7), task_id(7)]);
    }

    #[test]
    fn the_empty_broadcast_yields_no_lines() {
        assert_eq!(resurrected_tasks(&[]).len(), 0);
    }
}

#[cfg(test)]
mod wait_tests {
    use super::{SyncOutcome, wait_for_sync_result};
    use crate::test_support::{PredicateEngineFixture, predicate_engine, seed_board};
    use std::time::Duration;
    use taskboard_domain::{
        ConnectorId, PersistedState, RemoteBoardBinding, RemoteBoardId, SyncCounters,
        SyncErrorKind, SyncPhase, SyncStates, SyncStatus,
    };

    const BOARD: u128 = 0xC1;
    const SPAM_BOARD: u128 = 0xC2;

    fn status(cycles: u64, phase: SyncPhase) -> SyncStatus {
        SyncStatus {
            counters: SyncCounters {
                cycles,
                ..SyncCounters::default()
            },
            phase,
            last_success: None,
            ..SyncStatus::default()
        }
    }

    /// One deck-bound board carrying the seeded sync row (the aggregate
    /// fold over a single row is the row itself).
    fn seeded(row: SyncStatus) -> PersistedState {
        let mut p = PersistedState::default();
        let mut b = seed_board(BOARD);
        b.remote = Some(RemoteBoardBinding {
            connector: ConnectorId::from("nc"),
            board: RemoteBoardId::deck(1),
        });
        let id = b.id;
        p.boards.insert(id, b);
        // The unbound twin the signal spam renames (a bound board's
        // rename would flip its row to `Syncing`).
        let spam = seed_board(SPAM_BOARD);
        p.boards.insert(spam.id, spam);
        p.sync = SyncStates::with_row(id, row);
        p
    }

    /// Boots the fixture over the seeded row; `startup` is the explicit
    /// pre-nudge snapshot the wait decides freshness against (the
    /// single-read capture law — never re-read).
    async fn fixture_with(
        row: SyncStatus,
        startup: SyncStatus,
    ) -> (PredicateEngineFixture, SyncStatus) {
        let fixture = predicate_engine(seeded(row)).await;
        (fixture, startup)
    }

    /// The pre-nudge snapshot of a core that has not seen a report yet:
    /// the zero cycle, Idle default.
    fn fresh_startup() -> SyncStatus {
        SyncStatus::default()
    }

    /// Fresh cycle + Idle ⇒ `Succeeded` (the generation counter
    /// advanced past the startup snapshot).
    #[tokio::test]
    async fn a_fresh_idle_cycle_is_succeeded() {
        let (fixture, startup) = fixture_with(status(1, SyncPhase::Idle), fresh_startup()).await;
        let outcome =
            wait_for_sync_result(&fixture.engine, &startup, Duration::from_millis(200), None).await;
        assert!(matches!(outcome, SyncOutcome::Succeeded(_)));
        fixture.shutdown().await;
    }

    /// Fresh cycle + `Failed` ⇒ `Failed` carrying the classification.
    #[tokio::test]
    async fn a_fresh_failure_is_failed() {
        let (fixture, startup) = fixture_with(
            status(
                1,
                SyncPhase::Failed {
                    last_error: SyncErrorKind::Network,
                },
            ),
            fresh_startup(),
        )
        .await;
        let outcome =
            wait_for_sync_result(&fixture.engine, &startup, Duration::from_millis(200), None).await;
        assert!(matches!(
            outcome,
            SyncOutcome::Failed(SyncErrorKind::Network, _)
        ));
        fixture.shutdown().await;
    }

    /// A stale persisted `Failed` identical to the startup snapshot
    /// publishes nothing (S0-indistinguishability): the wait runs to
    /// the deadline and reports the phase it is stuck with — `Failed`
    /// (the loop's own doc names this leg; it is *not* an early fresh
    /// verdict, which the next test pins by the signal arm).
    #[tokio::test(start_paused = true)]
    async fn a_stale_persisted_failure_never_becomes_a_fresh_verdict() {
        let stale = status(
            0,
            SyncPhase::Failed {
                last_error: SyncErrorKind::Network,
            },
        );
        let (fixture, startup) = fixture_with(stale.clone(), stale).await;
        let outcome =
            wait_for_sync_result(&fixture.engine, &startup, Duration::from_millis(50), None).await;
        assert!(
            matches!(outcome, SyncOutcome::Failed(SyncErrorKind::Network, _)),
            "the deadline leg reports the stale phase verbatim"
        );
        fixture.shutdown().await;
    }

    /// The signal-armed leg: a state change was published (the seen
    /// signal) while the row is still the stale `Failed` — the wait
    /// returns `Failed` long before its (virtual) deadline. The
    /// `seen_change || status != startup` flip (`||` → `&&`) keeps
    /// waiting out the whole deadline instead — the same variant, an
    /// order of magnitude later, so the pin is the early arm firing
    /// without fresh counters.
    #[tokio::test(start_paused = true)]
    async fn a_published_change_arms_the_stale_failure_leg() {
        let stale = status(
            0,
            SyncPhase::Failed {
                last_error: SyncErrorKind::Auth,
            },
        );
        let (fixture, startup) = fixture_with(stale.clone(), stale).await;
        let engine = fixture.engine.clone();
        let waiter = tokio::spawn(async move {
            wait_for_sync_result(&engine, &startup, Duration::from_secs(600), None).await
        });
        // A 50 ms-cadence publish spam (virtual time): the waiter's
        // subscription arms inside its first loop pass, so one of the
        // publishes is guaranteed observed well inside its deadline —
        // the arm-before-evidence race cannot starve the `seen_change`
        // conjunct. `UpdateSettings` is the one accepted command that
        // publishes and signals *unconditionally* (a no-op-planned
        // edit signals nothing, and a bound board's rename would mark
        // its row `Syncing`, masking the stale-Failed leg under the
        // patience arm) — and it never touches the watched sync rows.
        let spam_engine = fixture.engine.clone();
        let spam = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                let command = taskboard_domain::StateCommand::UpdateSettings {
                    settings: taskboard_domain::SettingsSnapshot::default(),
                };
                if spam_engine.execute(command).await.is_err() {
                    break;
                }
            }
        });
        let outcome = waiter.await.expect("the waiter completes");
        spam.abort();
        assert!(matches!(
            outcome,
            SyncOutcome::Failed(SyncErrorKind::Auth, _)
        ));
        fixture.shutdown().await;
    }

    /// No report ever lands (a stale Idle row) ⇒ `TimedOut` at the
    /// deadline — the honest no-evidence answer.
    #[tokio::test(start_paused = true)]
    async fn a_stale_idle_row_times_out() {
        let idle = status(0, SyncPhase::Idle);
        let (fixture, startup) = fixture_with(idle.clone(), idle).await;
        let outcome =
            wait_for_sync_result(&fixture.engine, &startup, Duration::from_millis(50), None).await;
        assert!(matches!(outcome, SyncOutcome::TimedOut(_)));
        fixture.shutdown().await;
    }
}

#[cfg(test)]
mod resurrect_surface_pins {
    use super::report_resurrections;
    use std::sync::Arc;

    use taskboard_domain::{StateCommand, SystemEvent};

    use crate::config::AppConfig;
    use crate::secrets::{AppPassword, CredentialError, CredentialStore};

    #[derive(Debug)]
    struct FixedStore(AppPassword);

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

    async fn booted_app(dir: &tempfile::TempDir) -> crate::App {
        let mut config = AppConfig::default();
        config.nextcloud.server_url = Some("http://127.0.0.1:9".into());
        config.nextcloud.username = Some("alice".into());
        config.storage.db_path = Some(dir.path().join("taskboard.db"));
        config.app.shutdown_timeout_secs = 2;
        config.validate().expect("valid");
        let store: Arc<dyn CredentialStore> = Arc::new(FixedStore(AppPassword::new("pw")));
        crate::bootstrap::bootstrap(config, store, Arc::new(crate::notifications::LogSink))
            .await
            .expect("boots")
    }

    /// The writer-seam pin (#1229, plan §5/D6): N resurrect events
    /// (with a duplicate, preserving the arrival-order duplicate law)
    /// produce exactly N lines on the *injected writer*, in fold
    /// order — the duplicated id's lines are equal, the distinct ids'
    /// lines differ. Count + order are structural; the sentence is
    /// never compared.
    /// Captures a titled task; the id is state truth, read back from
    /// the engine outcome.
    async fn capture(app: &crate::App, title: &str) -> taskboard_domain::TaskId {
        match app
            .engine
            .execute(StateCommand::CaptureTask {
                title: title.to_owned(),
                board: None,
            })
            .await
            .expect("capture")
        {
            taskboard_domain::CommandOutcome::CreatedTask(id) => id,
            other => panic!("capture outcome: {other:?}"),
        }
    }

    #[tokio::test]
    async fn resurrect_events_render_exactly_n_lines_in_fold_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        let app = booted_app(&dir).await;
        let one = capture(&app, "resurrect-one").await;
        let two = capture(&app, "resurrect-two").await;

        let mut rx = app.system.subscribe();
        app.system
            .send(SystemEvent::TaskResurrected(one))
            .expect("broadcast alive");
        app.system
            .send(SystemEvent::TaskResurrected(two))
            .expect("broadcast alive");
        app.system
            .send(SystemEvent::TaskResurrected(one))
            .expect("broadcast alive");

        let mut sink: Vec<u8> = Vec::new();
        report_resurrections(&app, &mut rx, &mut sink);
        let text = String::from_utf8(sink).expect("utf-8 sink");
        let lines: Vec<&str> = text.split('\n').filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 3, "two episodes + the duplicate = three lines");
        assert_eq!(
            lines[0], lines[2],
            "the duplicated id renders the same line twice"
        );
        assert_ne!(lines[0], lines[1], "distinct ids render distinct lines");

        app.shutdown().await;
    }
}
