// SPDX-License-Identifier: MIT OR Apache-2.0

//! The config live-reload watcher (#909, journey US-1): one actor
//! watching the resolved config file's parent directory (non-recursive,
//! plan R-3), filtering every raw `notify` event to the resolved
//! config path, coalescing rename storms through a hand-rolled 300 ms
//! quiet window over tokio timers (plan D-DEBOUNCE — no debouncer
//! crate, so the debounce tests are deterministic under
//! `tokio::time::pause`/`advance`), and reloading through the C10
//! lane's apply half verbatim (plan D-RELOAD-PATH): file-layer decode
//! → `validate` → whole-snapshot mint → compare →
//! `StateCommand::UpdateSettings` on difference.
//!
//! The failure posture is typed everywhere (journey US-1/S1–S4):
//! a mid-rename read attempt is skipped and retried on the next
//! trailing edge (never an error loop); a decode/validate refusal
//! keeps the engine's last-known-good snapshot and broadcasts one
//! typed [`SystemEvent::ConfigReload`]; self-writes and no-op
//! external saves converge by *snapshot comparison* (plan
//! D-SELFWRITE — an unchanged decode is a no-op, never an engine
//! round trip); setup failure degrades typed — reload disabled for
//! the session, one [`ConfigReloadOutcome::WatcherDisabled`]
//! broadcast, never a poll fallback.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::Watcher as _;
use taskboard_domain::{ConfigReloadOutcome, StateCommand, SystemEvent};
use tokio::sync::{broadcast, mpsc};

use crate::config::AppConfig;

/// The quiet window (plan D-DEBOUNCE): well above any observed editor
/// save storm (all events land within milliseconds), well below
/// human-imperceptible. The tests drive the window deterministically
/// through `tokio::time::pause`/`advance` over the actor's tokio
/// timers.
pub(crate) const QUIET_WINDOW: Duration = Duration::from_millis(300);

/// Whether a raw `notify` event concerns the watched config file
/// (journey US-1/S1's event filter): any of the event's paths
/// resolving to the watched path matches — rename storms report the
/// old and/or new path, atomic-rename writers report the create.
/// Everything else (lockfiles, sockets, sibling files, the database)
/// is foreign and never arms the debounce window (plan R-3).
fn path_matches(watched: &Path, event: &notify::Event) -> bool {
    event
        .paths
        .iter()
        .any(|candidate| same_file(candidate, watched))
}

/// Path identity for the filter: canonicalized when both sides
/// resolve (the file may appear/disappear during a storm), lexical
/// fallback over parent + file name when one side is momentarily
/// absent — a rename's *removed* arm must still match.
fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(canonical_a), Ok(canonical_b)) => canonical_a == canonical_b,
        _ => {
            a.file_name().is_some()
                && a.file_name() == b.file_name()
                && a.parent().zip(b.parent()).is_some_and(|(pa, pb)| pa == pb)
        }
    }
}

/// One debounced reload attempt: the C10 lane's apply half with no
/// write step (plan D-RELOAD-PATH). Returns the typed outcome; the
/// caller owns the broadcast. Never panics on a raced writer — every
/// failure is a typed outcome.
///
/// Deterministic by construction: one read + decode + mint + compare,
/// one engine round trip on difference. The read rides the async FS
/// API (`tokio::fs` — `spawn_blocking` underneath) so the debouncer's
/// actor task never parks the runtime on a blocking syscall.
pub(crate) async fn reload_once(
    path: &Path,
    engine: &taskboard_state::EngineHandle,
) -> ConfigReloadOutcome {
    // Transient absence (a rename storm's mid-rename instant) or any
    // unreadable instant: skip — retried on the next trailing edge,
    // never an error loop (US-1/S2, AC-3).
    let Ok(text) = tokio::fs::read_to_string(path).await else {
        return ConfigReloadOutcome::SkippedTransient;
    };
    // The prove gate's read half, verbatim (plan D-RELOAD-PATH): the
    // file-layer decode AND the semantic validation. A refusal keeps
    // the last-known-good snapshot — the engine is never contacted.
    let Ok(config) = AppConfig::decode_file_only_str(&text) else {
        return ConfigReloadOutcome::RefusedDecode;
    };
    if config.validate().is_err() {
        return ConfigReloadOutcome::RefusedValidate;
    }
    let settings = config.settings_snapshot();
    // Self-write / no-op convergence by snapshot comparison (plan
    // D-SELFWRITE): an unchanged decode costs one decode, not an
    // engine round trip (US-1 AC-4).
    let published = engine.shared_state().load().settings.clone();
    if settings == published {
        return ConfigReloadOutcome::Unchanged;
    }
    match engine
        .execute(StateCommand::UpdateSettings { settings })
        .await
    {
        Ok(_) => ConfigReloadOutcome::Applied,
        Err(err) => {
            tracing::warn!(%err, "config reloaded but the engine did not apply it");
            ConfigReloadOutcome::EngineGone
        }
    }
}

/// Drive the broadcast-side shutdown wait: resolves on the graceful
/// `SystemEvent::Shutdown` (the watcher's shutdown-ordering contract,
/// journey US-1/S1) or when the system bus closes; unrelated events
/// and a `Lagged` window are skipped — the watcher is not an event
/// consumer, only a sender.
async fn wait_shutdown(rx: &mut broadcast::Receiver<SystemEvent>) {
    loop {
        match rx.recv().await {
            Ok(SystemEvent::Shutdown) | Err(broadcast::error::RecvError::Closed) => return,
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
        }
    }
}

/// The watcher actor's event loop (journey US-1/S1–S4): filter →
/// quiet-window coalesce → one reload per trailing edge → one typed
/// broadcast per attempt. Exits on the system bus's `Shutdown` or
/// when the event channel closes (the notify bridge dropped — setup
/// fell over mid-session; the session keeps its last-known-good
/// state, the restart contract degrades typed).
///
/// The debounce is a trailing-edge quiet window: every matching event
/// pushes the deadline out; when the window closes untouched, exactly
/// one reload runs (US-1 AC-2 — a rename storm is one reload).
pub(crate) async fn run(
    watched: PathBuf,
    mut events: mpsc::UnboundedReceiver<notify::Event>,
    engine: taskboard_state::EngineHandle,
    system: broadcast::Sender<SystemEvent>,
    mut shutdown: broadcast::Receiver<SystemEvent>,
) {
    loop {
        // Arm: the first matching event starts the quiet window.
        let mut deadline = tokio::time::Instant::now() + QUIET_WINDOW;
        loop {
            tokio::select! {
                biased;
                () = wait_shutdown(&mut shutdown) => return,
                event = events.recv() => match event {
                    Some(event) if path_matches(&watched, &event) => break,
                    Some(_) => {}
                    None => return,
                },
            }
        }
        // Coalesce: every matching event resets the window; the
        // trailing edge fires exactly once.
        loop {
            tokio::select! {
                biased;
                () = wait_shutdown(&mut shutdown) => return,
                event = events.recv() => match event {
                    Some(event) if path_matches(&watched, &event) => {
                        deadline = tokio::time::Instant::now() + QUIET_WINDOW;
                    }
                    Some(_) => {}
                    None => return,
                },
                () = tokio::time::sleep_until(deadline) => break,
            }
        }
        let outcome = reload_once(&watched, &engine).await;
        tracing::info!(?outcome, "config reload attempt finished");
        // The bus may already be gone mid-shutdown — the outcome is
        // still applied to the engine; the report is best-effort.
        let _ = system.send(SystemEvent::ConfigReload(outcome));
    }
}

/// The watcher's spawn glue (journey W-2/W-3): resolve the watched
/// path through the single path-resolution rule, build the platform
/// watcher on the config file's parent (non-recursive), bridge the
/// notify thread's events into a tokio channel, and spawn the actor.
///
/// Setup failure is the typed fallback (US-1/S1 escape): reload is
/// disabled for the session, ONE [`ConfigReloadOutcome::WatcherDisabled`]
/// broadcast fires, and nothing else is scheduled — never a poll
/// fallback, never an error loop.
///
/// Returns the spawned actor's join handle (`None` — no task spawned
/// — on the setup-failure fallback; `None` as well when the resolved
/// path has no parent directory to watch, the same fallback class).
pub(crate) fn spawn(
    system: &broadcast::Sender<SystemEvent>,
    engine: &taskboard_state::EngineHandle,
    resolved_path: PathBuf,
) -> Option<tokio::task::JoinHandle<()>> {
    let Some(parent) = resolved_path.parent().filter(|p| !p.as_os_str().is_empty()) else {
        tracing::warn!(path = %resolved_path.display(), "config watcher: no parent directory to watch");
        let _ = system.send(SystemEvent::ConfigReload(
            ConfigReloadOutcome::WatcherDisabled,
        ));
        return None;
    };
    // The notify bridge: the crate's own thread runs the handler, so
    // the forwarder is a plain unbounded send (sync) — no blocking
    // bridge task, no second runtime hop.
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let mut watcher =
        match notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
            if let Ok(event) = result {
                let _ = event_tx.send(event);
            }
        }) {
            Ok(watcher) => watcher,
            Err(err) => {
                tracing::warn!(%err, "config watcher: the platform backend refused to start");
                let _ = system.send(SystemEvent::ConfigReload(
                    ConfigReloadOutcome::WatcherDisabled,
                ));
                return None;
            }
        };
    if let Err(err) = watcher.watch(parent, notify::RecursiveMode::NonRecursive) {
        tracing::warn!(%err, path = %parent.display(), "config watcher: the watch refused to start");
        let _ = system.send(SystemEvent::ConfigReload(
            ConfigReloadOutcome::WatcherDisabled,
        ));
        return None;
    }
    // The shutdown subscription is taken HERE, before the task is
    // spawned: a `Shutdown` broadcast sent the instant after spawn
    // must not race the task's first poll (the ordering contract the
    // bootstrap seam test pins).
    let shutdown_rx = system.subscribe();
    let system_clone = system.clone();
    let engine_clone = engine.clone();
    Some(tokio::spawn(async move {
        // The watcher handle stays alive for the actor's whole life:
        // dropping it ends event delivery (the actor would then see
        // a closed channel and exit, but keeping it here makes the
        // ownership explicit).
        let _keep_alive = watcher;
        run(
            resolved_path,
            event_rx,
            engine_clone,
            system_clone,
            shutdown_rx,
        )
        .await;
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    use notify::event::{EventKind, ModifyKind, RemoveKind};
    use taskboard_domain::SystemEvent;

    /// The #984 dated-pin gate (the #865 re-homing debt, tracked in
    /// the round-3 backlog): true when no `TASKBOARD_*` ambient is
    /// present. The watcher's reload legs compose the process env in
    /// production, and this lib crate forbids the env mutation that
    /// `taskboard_env_lock` needs — `tests/services/ui_settings.rs`
    /// is the hostile-oracle home for the re-homed legs.
    fn clean_ambient() -> bool {
        !std::env::vars_os().any(|(key, _)| key.as_encoded_bytes().starts_with(b"TASKBOARD_"))
    }

    /// A synthetic raw event (the tests never touch the platform
    /// backend — the debounce and filter run over the same tokio
    /// channel the real bridge feeds).
    fn raw_event(kind: EventKind, path: &Path) -> notify::Event {
        let mut event = notify::Event::new(kind);
        event.paths.push(path.to_path_buf());
        event
    }

    /// The filter's truth table: events on the watched path (and the
    /// rename arm's removed-path shape) match; foreign sibling files
    /// and path-less events never do (journey US-1/S1, plan R-3).
    #[test]
    fn us1_s1_watcher_filter_skips_foreign_events() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let watched = dir.path().join("taskboard.toml");
        std::fs::write(&watched, "[reminders]\ntick_secs = 30\n").expect("seed");
        let foreign = dir.path().join("sibling.lock");

        for kind in [
            EventKind::Create(notify::event::CreateKind::File),
            EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
            EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::Both)),
            EventKind::Remove(RemoveKind::File),
            EventKind::Other,
        ] {
            assert!(
                path_matches(&watched, &raw_event(kind, &watched)),
                "{kind:?} on the watched path matches"
            );
        }
        // A rename storm's removed arm reports the watched path while
        // the file is momentarily absent — canonicalization fails on
        // both sides, and the lexical fallback (parent + file name)
        // still matches.
        std::fs::remove_file(&watched).expect("removed");
        assert!(
            path_matches(&watched, &raw_event(EventKind::Other, &watched)),
            "the removed arm of a rename matches lexically"
        );
        assert!(
            !path_matches(&watched, &raw_event(EventKind::Other, &foreign)),
            "a foreign sibling never matches"
        );
        assert!(
            !path_matches(
                &watched,
                &raw_event(EventKind::Other, Path::new("/other/dir/taskboard.toml"))
            ),
            "a same-named file in another directory never matches"
        );
        let pathless = notify::Event::new(EventKind::Other);
        assert!(
            !path_matches(&watched, &pathless),
            "a path-less event never matches"
        );
    }

    /// `us1_s2_rename_storm_coalesces_into_one_reload` (US-1 AC-2):
    /// a rename storm — remove + create + three modifies, all inside
    /// the window — produces exactly ONE reload on the trailing edge,
    /// and a quiet period after produces no further reloads (never an
    /// error loop). Deterministic: `tokio::time::pause`/`advance` over
    /// the actor's tokio timers — never sleeps.
    #[tokio::test]
    async fn us1_s2_rename_storm_coalesces_into_one_reload() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        // Boot on the live clock first — the paused clock auto-advances
        // on idle, which would fire the storage pool's connect timeout
        // mid-boot. Everything below the pause is timer-driven and
        // deterministic (pause + advance, never sleeps).
        let fixture = crate::test_support::standalone_boot_fixture("storm").await;
        let app = fixture.app;
        let dir = tempfile::tempdir().expect("tempdir");
        let watched = dir.path().join("taskboard-reload-storm.toml");
        std::fs::write(&watched, "[reminders]\ntick_secs = 30\n").expect("seed");
        tokio::time::pause();
        let mut outcomes = app.system.subscribe();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let handle = tokio::spawn(run(
            watched.clone(),
            event_rx,
            app.engine.clone(),
            app.system.clone(),
            app.system.subscribe(),
        ));

        // The storm: six raw events within the window.
        for kind in [
            EventKind::Remove(RemoveKind::File),
            EventKind::Create(notify::event::CreateKind::File),
            EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
            EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
            EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::Both)),
            EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
        ] {
            event_tx
                .send(raw_event(kind, &watched))
                .expect("the actor lives");
        }

        // Mid-window: the window is still open, no reload ran.
        tokio::time::advance(
            QUIET_WINDOW
                .checked_sub(Duration::from_millis(50))
                .expect("the window exceeds the mid-window offset"),
        )
        .await;
        tokio::task::yield_now().await;
        let mut mid_window = 0_u32;
        while let Ok(event) = outcomes.try_recv() {
            if matches!(event, SystemEvent::ConfigReload(_)) {
                mid_window += 1;
            }
        }
        assert_eq!(mid_window, 0, "the quiet window is still open mid-storm");

        // One more event resets the window (the trailing-edge rule).
        event_tx
            .send(raw_event(
                EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any)),
                &watched,
            ))
            .expect("the actor lives");
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        assert!(
            !matches!(outcomes.try_recv(), Ok(SystemEvent::ConfigReload(_))),
            "the reset window has not closed yet"
        );

        // The trailing edge: exactly one reload for the whole storm
        // (the outcome class is the convergence tests' business —
        // this pin is the coalescing count). The advance runs one
        // millisecond PAST the re-armed window: the reset above moved
        // the deadline to `now + QUIET_WINDOW`, and a timer fires when
        // the clock reaches its deadline — advancing exactly the
        // window would land the clock ON the deadline.
        tokio::time::advance(QUIET_WINDOW + Duration::from_millis(1)).await;
        // The reload rides the async FS API (spawn_blocking under the
        // hood), so the outcome surfaces when a real blocking thread
        // finishes — a single yield no longer guarantees it. A bounded
        // deterministic spin (yield_now only, never a sleep) polls the
        // predicate to completion: microseconds in practice, and a
        // miss is this test's honest failure. The count accumulates
        // over the whole spin so a double reload cannot hide.
        let mut reloaded = 0_u32;
        for _ in 0..2_000 {
            tokio::task::yield_now().await;
            while let Ok(event) = outcomes.try_recv() {
                if matches!(event, SystemEvent::ConfigReload(_)) {
                    reloaded += 1;
                }
            }
        }
        assert_eq!(reloaded, 1, "one reload per storm, on the trailing edge");

        // A quiet period after fires nothing further (no error loop,
        // no self-rescheduling).
        tokio::time::advance(Duration::from_secs(10)).await;
        tokio::task::yield_now().await;
        let mut quiet = 0_u32;
        while let Ok(event) = outcomes.try_recv() {
            if matches!(event, SystemEvent::ConfigReload(_)) {
                quiet += 1;
            }
        }
        assert_eq!(quiet, 0, "no reload without a new event");

        // Shutdown ordering: the graceful broadcast ends the actor.
        app.system
            .send(SystemEvent::Shutdown)
            .expect("the actor subscribes");
        tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("the actor ended on Shutdown")
            .expect("the actor task joined cleanly");
        app.shutdown().await;
    }

    /// `us1_s3_reload_proves_the_file_layer_before_apply` (US-1 AC-5):
    /// an invalid external write refuses typed at the right stage and
    /// the engine keeps the last-known-good snapshot — the engine is
    /// never contacted on a refusal.
    #[tokio::test]
    async fn us1_s3_reload_proves_the_file_layer_before_apply() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("prove").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let before = app.engine.shared_state().load().settings.clone();

        // Garbage: the decode refuses.
        std::fs::write(&path, "not toml at all ]]=").expect("garbage seed");
        let outcome = reload_once(&path, &app.engine).await;
        assert_eq!(outcome, ConfigReloadOutcome::RefusedDecode, "{outcome:?}");
        assert_eq!(
            app.engine.shared_state().load().settings,
            before,
            "the last-known-good snapshot stands"
        );

        // Decodes but invalid: the validate arm refuses.
        std::fs::write(&path, "[reminders]\ntick_secs = 0\n").expect("invalid seed");
        let outcome = reload_once(&path, &app.engine).await;
        assert_eq!(outcome, ConfigReloadOutcome::RefusedValidate, "{outcome:?}");
        assert_eq!(
            app.engine.shared_state().load().settings,
            before,
            "the last-known-good snapshot stands"
        );
        app.shutdown().await;
    }

    /// `us1_s4_partial_write_keeps_last_good_and_retries` (US-1 AC-3):
    /// a mid-rename instant (the file missing) is the typed
    /// skip-and-retry outcome, the last-known-good state stands, and
    /// the *next* good write converges normally.
    #[tokio::test]
    async fn us1_s4_partial_write_keeps_last_good_and_retries() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("partial").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        let before = app.engine.shared_state().load().settings.clone();

        // The mid-rename instant: the watched path is absent.
        std::fs::remove_file(&path).expect("removed");
        let outcome = reload_once(&path, &app.engine).await;
        assert_eq!(
            outcome,
            ConfigReloadOutcome::SkippedTransient,
            "{outcome:?}"
        );
        assert_eq!(
            app.engine.shared_state().load().settings,
            before,
            "the last-known-good state stands through the skip"
        );

        // The retried trailing edge finds a good write: converges.
        std::fs::write(&path, "[focus]\nwork_mins = 50\n").expect("the good write");
        let outcome = reload_once(&path, &app.engine).await;
        assert_eq!(outcome, ConfigReloadOutcome::Applied, "{outcome:?}");
        assert_eq!(
            app.engine
                .shared_state()
                .load()
                .settings
                .focus
                .as_ref()
                .expect("minted")
                .work(),
            chrono::TimeDelta::minutes(50),
            "the retried reload converged"
        );
        app.shutdown().await;
    }

    /// `us1_s4_self_write_snapshot_equality_is_a_noop` (US-1 AC-4):
    /// the app's own write-through set leaves the file matching the
    /// published settings — the reload decodes, compares, and applies
    /// nothing (one decode, no engine round trip).
    #[tokio::test]
    async fn us1_s4_self_write_snapshot_equality_is_a_noop() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        let fixture = crate::test_support::standalone_boot_fixture("selfwrite").await;
        let (path, app) = (fixture.config_path.clone(), fixture.app);
        // The app's own write-through lane (the C10 set, live arm):
        // file and engine converge to the same snapshot.
        let locked = std::collections::BTreeSet::new();
        crate::commands::config::set_with_locked(
            &path,
            "focus.work_mins",
            "50",
            &locked,
            Some(&app.engine),
        )
        .await
        .expect("the self-write converges");
        let outcome = reload_once(&path, &app.engine).await;
        assert_eq!(outcome, ConfigReloadOutcome::Unchanged, "{outcome:?}");
        assert_eq!(
            app.engine
                .shared_state()
                .load()
                .settings
                .focus
                .as_ref()
                .expect("minted")
                .work(),
            chrono::TimeDelta::minutes(50)
        );
        app.shutdown().await;
    }

    /// `us1_s5_reloaded_snapshot_reaches_the_published_settings`
    /// (US-1 AC-1 + the reuse law's parity pin): the *same file
    /// delta* applied via the C10 lane's set and via an external
    /// write + reload converges the engine to the equal published
    /// snapshot — two lanes, one state.
    #[tokio::test]
    async fn us1_s5_reloaded_snapshot_reaches_the_published_settings() {
        // Dated pin: the ambient gate, `clean_ambient`'s doc (#1113).
        if !clean_ambient() {
            return;
        }
        // Lane A: the C10 write-through set.
        let fixture_a = crate::test_support::standalone_boot_fixture("parity-set").await;
        let locked = std::collections::BTreeSet::new();
        crate::commands::config::set_with_locked(
            &fixture_a.config_path,
            "focus.work_mins",
            "80",
            &locked,
            Some(&fixture_a.app.engine),
        )
        .await
        .expect("the C10 lane lands");

        // Lane B: the same file delta via an external write + reload.
        let fixture_b = crate::test_support::standalone_boot_fixture("parity-reload").await;
        std::fs::write(
            &fixture_b.config_path,
            "[focus]\nwork_mins = 80\n[reminders]\ntick_secs = 30\n",
        )
        .expect("the external write");
        let outcome = reload_once(&fixture_b.config_path, &fixture_b.app.engine).await;
        assert_eq!(outcome, ConfigReloadOutcome::Applied, "{outcome:?}");

        let published_a = fixture_a.app.engine.shared_state().load().settings.clone();
        let published_b = fixture_b.app.engine.shared_state().load().settings.clone();
        assert_eq!(
            published_a, published_b,
            "the reload converges to the state the C10 lane produces"
        );
        fixture_a.app.shutdown().await;
        fixture_b.app.shutdown().await;
    }
}
