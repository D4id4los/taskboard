// SPDX-License-Identifier: AGPL-3.0-only
//! The all-down network authority (DF-7, design §3.7 option (a)): an
//! app-layer task that turns the sync actors' per-connector episode
//! events into the global `NetworkLost`/`NetworkRestored` vocabulary by
//! *counting* — the engine cannot see connector counts (the registry is
//! app-layer), and a broadcast receiver cannot tell which sender a unit
//! event came from, so the actors carry the id and this task owns the
//! up/down set.
//!
//! The authority infers from events only, never from silence: a
//! connector that has not cycled yet is presumptively up (its first
//! transport failure flips it — exactly the pre-authority
//! first-failure trigger); "no report lately" is never an input.
//! Emissions happen only on set transitions, so duplicates dedupe
//! structurally. No hysteresis: the episode-edge rule is the debounce.
//!
//! Membership (the configured-connector set) is lifecycle truth seeded
//! from the module set at boot and updated by the attach/detach
//! registry hooks — app-internal bookkeeping, not inter-actor traffic.
//! Events for non-members (a raced detach's stale in-flight event) are
//! ignored: observations of members only. A drained (empty) set is the
//! standalone world and stays silent both ways, as standalone always
//! has.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use taskboard_domain::{ConnectorId, SystemEvent};

/// The up/down membership: `true` = up (not observed down). A connector
/// that has never cycled is up. `BTreeMap` for deterministic iteration.
#[derive(Debug, Default)]
pub(crate) struct AuthorityState {
    membership: BTreeMap<ConnectorId, bool>,
}

/// The global emission decision, shared by the event path and the
/// lifecycle hooks: the all-down / any-back crossing.
fn all_down(membership: &BTreeMap<ConnectorId, bool>) -> bool {
    !membership.is_empty() && membership.values().all(|up| !*up)
}

impl AuthorityState {
    /// Seeds the set from the configured connector ids, all up.
    pub(crate) fn new(ids: impl IntoIterator<Item = ConnectorId>) -> Self {
        Self {
            membership: ids.into_iter().map(|id| (id, true)).collect(),
        }
    }

    /// Processes one observed per-connector episode event; returns the
    /// global event to emit, if the observation crossed a boundary.
    /// Non-members change nothing.
    fn observe(&mut self, lost: bool, connector: &ConnectorId) -> Option<SystemEvent> {
        let up = self.membership.get_mut(connector)?;
        if lost {
            if *up {
                *up = false;
                if all_down(&self.membership) {
                    return Some(SystemEvent::NetworkLost);
                }
            }
        } else if !*up {
            *up = true;
            // First up: exactly one connector is up after this write and
            // the set is nonempty by construction.
            if self.membership.values().filter(|up| **up).count() == 1 {
                return Some(SystemEvent::NetworkRestored);
            }
        }
        None
    }

    /// Registry hook: a connector attached — inserted up. Emits
    /// `NetworkRestored` iff the pre-insert set was nonempty and
    /// all-down (a real hole closed). Coming from the empty set emits
    /// nothing: today's attach fires no network event.
    fn attach(&mut self, connector: &ConnectorId) -> Option<SystemEvent> {
        let hole_closed = all_down(&self.membership);
        self.membership.insert(connector.clone(), true);
        hole_closed.then_some(SystemEvent::NetworkRestored)
    }

    /// Registry hook: a connector detached — removed. Emits
    /// `NetworkLost` iff the post-remove set is nonempty and all-down
    /// (everything remaining is observed down). Removal to the empty
    /// set (drain-all, standalone) emits nothing.
    fn detach(&mut self, connector: &ConnectorId) -> Option<SystemEvent> {
        self.membership.remove(connector);
        all_down(&self.membership).then_some(SystemEvent::NetworkLost)
    }
}

/// The authority's surface parked on [`crate::bootstrap::App`] and the
/// task: one shared state + the broadcast sender for emissions. Sync
/// fns over the shared state are app-internal bookkeeping, not
/// cross-actor traffic (AGENTS §3's channel rules govern actor-to-actor
/// messaging, which this is not).
#[derive(Clone)]
pub(crate) struct AuthorityHandle {
    state: Arc<Mutex<AuthorityState>>,
    system: broadcast::Sender<SystemEvent>,
}

impl AuthorityHandle {
    pub(crate) fn new(
        state: Arc<Mutex<AuthorityState>>,
        system: broadcast::Sender<SystemEvent>,
    ) -> Self {
        Self { state, system }
    }

    /// Event path: one observed per-connector episode, emitted only on
    /// a set transition (duplicates dedupe structurally).
    fn observe(&self, lost: bool, connector: &ConnectorId) {
        if let Some(global) = self
            .state
            .lock()
            .expect("authority lock")
            .observe(lost, connector)
        {
            tracing::info!(connector = %connector, ?global, "authority: the counted truth crossed");
            let _ = self.system.send(global);
        }
    }

    /// Registry hook — see [`AuthorityState::attach`].
    pub(crate) fn attach(&self, connector: &ConnectorId) {
        if let Some(global) = self.state.lock().expect("authority lock").attach(connector) {
            tracing::info!(connector = %connector, ?global, "authority: a hole closed");
            let _ = self.system.send(global);
        }
    }

    /// Registry hook — see [`AuthorityState::detach`].
    pub(crate) fn detach(&self, connector: &ConnectorId) {
        if let Some(global) = self.state.lock().expect("authority lock").detach(connector) {
            tracing::info!(connector = %connector, ?global, "authority: everything remaining is down");
            let _ = self.system.send(global);
        }
    }
}

/// Spawns the authority task: subscribe → update set → maybe-emit, one
/// level of abstraction per arm. Exits on `Shutdown`, channel close, or
/// a `Lagged` backlog (a missed 64-event window means the counted truth
/// is stale; the engine's per-connector arms keep working and the
/// global edge degrades honestly — the system-event logger's
/// convention).
pub(crate) fn spawn_authority(
    handle: AuthorityHandle,
    mut events: broadcast::Receiver<SystemEvent>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(SystemEvent::Shutdown) | Err(broadcast::error::RecvError::Closed) => return,
                Ok(SystemEvent::ConnectorLost(c)) => handle.observe(true, &c),
                Ok(SystemEvent::ConnectorRestored(c)) => handle.observe(false, &c),
                Ok(_) => {} // the globals themselves and `StorageFailed`
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "authority: system event backlog overflowed");
                    return;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ids<const N: usize>(names: [&str; N]) -> Vec<ConnectorId> {
        names.iter().map(|n| ConnectorId::from(*n)).collect()
    }

    // ---- the state edges (sync fns, no runtime; D5-13 verbatim) -------

    /// N=1 pass-through: the actor's episode edge plus the transition
    /// logic — first transport failure emits the global, the duplicate
    /// does not, the recovery emits the restore.
    #[test]
    fn single_connector_passes_through_the_episode_edges() {
        let mut state = AuthorityState::new(ids(["a"]));
        assert_eq!(
            state.observe(true, &ConnectorId::from("a")),
            Some(SystemEvent::NetworkLost)
        );
        assert_eq!(
            state.observe(true, &ConnectorId::from("a")),
            None,
            "a duplicate down is no transition"
        );
        assert_eq!(
            state.observe(false, &ConnectorId::from("a")),
            Some(SystemEvent::NetworkRestored)
        );
        assert_eq!(
            state.observe(false, &ConnectorId::from("a")),
            None,
            "a duplicate up is no transition"
        );
    }

    #[test]
    fn two_connectors_emit_only_on_the_all_down_crossing() {
        let mut state = AuthorityState::new(ids(["a", "b"]));
        assert_eq!(state.observe(true, &ConnectorId::from("a")), None);
        assert_eq!(
            state.observe(true, &ConnectorId::from("b")),
            Some(SystemEvent::NetworkLost),
            "last down"
        );
        assert_eq!(
            state.observe(false, &ConnectorId::from("b")),
            Some(SystemEvent::NetworkRestored),
            "first up"
        );
        assert_eq!(
            state.observe(false, &ConnectorId::from("a")),
            None,
            "second up is no crossing"
        );
    }

    #[test]
    fn non_member_events_change_nothing() {
        let mut state = AuthorityState::new(ids(["a"]));
        assert_eq!(state.observe(true, &ConnectorId::from("ghost")), None);
        assert_eq!(state.observe(false, &ConnectorId::from("ghost")), None);
        // The member is untouched by the ghost's noise.
        assert_eq!(
            state.observe(true, &ConnectorId::from("a")),
            Some(SystemEvent::NetworkLost)
        );
    }

    #[test]
    fn attach_fills_a_real_hole_loudly_and_silently_opens_the_empty_set() {
        let mut state = AuthorityState::new(ids(["a", "b"]));
        let _ = state.observe(true, &ConnectorId::from("a"));
        let _ = state.observe(true, &ConnectorId::from("b"));
        // All-down nonempty: an attach closes a real hole.
        assert_eq!(
            state.attach(&ConnectorId::from("c")),
            Some(SystemEvent::NetworkRestored)
        );
        // An empty set is the standalone world: silent both ways.
        let mut empty = AuthorityState::new(ids([]));
        assert_eq!(empty.attach(&ConnectorId::from("a")), None);
        let mut state = AuthorityState::new(ids(["a"]));
        let _ = state.observe(true, &ConnectorId::from("a"));
        assert_eq!(
            state.detach(&ConnectorId::from("a")),
            None,
            "drain to the empty set stays silent"
        );
    }

    #[test]
    fn detach_leaving_all_down_nonempty_is_loud() {
        let mut state = AuthorityState::new(ids(["a", "b"]));
        let _ = state.observe(true, &ConnectorId::from("b"));
        // Removing the last up connector leaves a nonempty all-down set.
        assert_eq!(
            state.detach(&ConnectorId::from("a")),
            Some(SystemEvent::NetworkLost)
        );
        // A detached member's stale in-flight event is ignored.
        assert_eq!(state.observe(false, &ConnectorId::from("a")), None);
    }

    // ---- the task over a real broadcast --------------------------------

    /// The D5-13 catalogue end-to-end: seeded `{a, b}` — `Lost(a)` ⇒ no
    /// global, `Lost(b)` ⇒ `NetworkLost`, a duplicate `Lost(b)` ⇒
    /// nothing, `Restored(a)` ⇒ `NetworkRestored`, `Restored(b)` ⇒
    /// nothing. The observer rides the same broadcast.
    #[tokio::test(start_paused = true)]
    async fn the_authority_task_emits_exactly_the_crossings() {
        let (system, _) = broadcast::channel(16);
        let state = Arc::new(Mutex::new(AuthorityState::new(ids(["a", "b"]))));
        let handle = AuthorityHandle::new(state, system.clone());
        let mut observer = system.subscribe();
        let join = spawn_authority(handle, system.subscribe());

        system
            .send(SystemEvent::ConnectorLost(ConnectorId::from("a")))
            .expect("send");
        system
            .send(SystemEvent::ConnectorLost(ConnectorId::from("b")))
            .expect("send");
        system
            .send(SystemEvent::ConnectorLost(ConnectorId::from("b")))
            .expect("send");
        system
            .send(SystemEvent::ConnectorRestored(ConnectorId::from("a")))
            .expect("send");
        system
            .send(SystemEvent::ConnectorRestored(ConnectorId::from("b")))
            .expect("send");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut globals = Vec::new();
        while tokio::time::Instant::now() < deadline && globals.len() < 2 {
            // Step the paused clock so the task runs; the bounded wall
            // of events is finite, so the recv terminates.
            if let Ok(Ok(global @ (SystemEvent::NetworkLost | SystemEvent::NetworkRestored))) =
                tokio::time::timeout(Duration::from_millis(100), observer.recv()).await
            {
                globals.push(global);
            }
        }
        assert_eq!(
            globals,
            vec![SystemEvent::NetworkLost, SystemEvent::NetworkRestored],
            "exactly the two crossings, in order"
        );

        // Shutdown exits the task cleanly.
        system.send(SystemEvent::Shutdown).expect("send");
        let _ = tokio::time::timeout(Duration::from_secs(5), join).await;
    }
}
