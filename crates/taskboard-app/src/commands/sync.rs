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
) -> Result<SyncOutcome, CommandError> {
    crate::commands::require_connector(&config)?;
    let app = crate::bootstrap::bootstrap(config, store).await?;
    let result = sync_on(&app, no_wait, timeout).await;
    let _ = app.shutdown().await;
    result
}

/// The sync steps on a booted core; the caller owns the shutdown (the
/// session dispatcher calls this directly against the session core).
pub(crate) async fn sync_on(
    app: &crate::App,
    no_wait: bool,
    timeout: Duration,
) -> Result<SyncOutcome, CommandError> {
    // Hydrated from the db — may be a stale `Idle`/`Failed`.
    let startup = app.engine.shared_state().load_full().sync.clone();
    if no_wait {
        let view = SyncStatusView::new(&app.engine.shared_state().load_full().sync);
        return Ok(SyncOutcome::NoWait(view));
    }
    // Belt-and-braces nudge; the boot cycle coalesces with it.
    app.engine.execute(StateCommand::RequestSync).await?;
    Ok(wait_for_sync_result(app, &startup, timeout).await)
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
) -> SyncOutcome {
    let mut signals = app.engine.subscribe();
    let mut seen_change = false;
    let deadline_at = Instant::now() + deadline;
    loop {
        let status = app.engine.shared_state().load_full().sync.clone();
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
