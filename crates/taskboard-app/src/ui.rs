// SPDX-License-Identifier: AGPL-3.0-only
//! The UI lifecycle and adaptation layer (GUI Phase 7, chunk 1).
//!
//! `taskboard-app` owns every IO seam and hands the presentation crate
//! typed values and channel halves (design §1.2): this module adapts the
//! booted core ([`crate::bootstrap::App`]) onto the UI's hand-over set.
//! Chunk 1 carries the *UI-command forwarder* — the actor that drains
//! the UI's command channel into the state engine; the desktop entry
//! lifecycle lands in this module too.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use taskboard_domain::StateCommand;
use taskboard_state::EngineHandle;

/// The engine-side sink the forwarder adapts onto. A trait so the
/// forwarder is hermetically testable without a state engine.
pub trait CommandSink: Send + 'static {
    /// Hands one command to the engine (fire-and-forget; drop logging
    /// on a full or closed inbox is the sink's own behaviour).
    fn dispatch(&self, command: StateCommand);
}

impl CommandSink for EngineHandle {
    fn dispatch(&self, command: StateCommand) {
        // EngineHandle::dispatch's own try_send drop logging (full or
        // closed inbox) is the state crate's tested behaviour, not
        // re-tested here (one behaviour, one test, lowest layer).
        EngineHandle::dispatch(self, command);
    }
}

/// Capacity of the UI → app command channel. The engine inbox is 256
/// deep; UI emission is human-input-scale, so 64 queued intents are
/// already an unresponsive-UI pathology rather than a throughput need.
pub(crate) const UI_COMMAND_CAPACITY: usize = 64;

/// Spawns the UI-command forwarder: drains `commands` into `sink` in
/// arrival order until the channel closes (the UI side dropped — the
/// forwarder's own exit, logged). The caller owns the handle's
/// retirement (await the clean exit, or abort once the UI session is
/// over).
pub fn spawn_ui_command_forwarder<T: CommandSink>(
    mut commands: mpsc::Receiver<StateCommand>,
    sink: T,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(command) = commands.recv().await {
            sink.dispatch(command);
        }
        tracing::info!("ui command channel closed; forwarder exiting");
    })
}

/// Where a bare `taskboard` invocation routes under a loaded config
/// (chunk decision D4): `Desktop` boots the GUI *regardless of TTY* —
/// `AppMode` is frontend selection (ADR 0008), and a terminal launch
/// (`taskboard` from a shell) is the normal way to start a desktop app.
/// The REPL stays the TTY front-end of `Daemon` mode (ADR 0011); an
/// explicit `daemon` subcommand bypasses this routing entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BareRoute {
    /// Boot the core and run the desktop UI lifecycle.
    DesktopUi,
    /// TTY + daemon mode: the REPL session (ADR 0011).
    Repl,
    /// Non-TTY daemon mode (or the TTY-less REPL refusal, resolved by
    /// `repl::entry_point`): the daemon lifecycle.
    DaemonLifecycle,
}

/// The pure D4 mode→route mapping, extracted so the behaviour change
/// (bare+TTY `desktop` now GUIs instead of opening a REPL) is
/// unit-testable without a display.
#[must_use]
pub fn bare_invocation_route(mode: crate::config::AppMode, tty: bool) -> BareRoute {
    match mode {
        crate::config::AppMode::Desktop => BareRoute::DesktopUi,
        crate::config::AppMode::Daemon if tty => BareRoute::Repl,
        crate::config::AppMode::Kiosk | crate::config::AppMode::Daemon => {
            BareRoute::DaemonLifecycle
        }
    }
}

/// The OS-DPI escape hatch (§6.2, D13): reads `SLINT_SCALE_FACTOR`
/// *before* any slint-touching call and logs its presence. The app
/// never sets it programmatically — the user's env is the input;
/// observability is the app's whole duty toward it. The winit backend
/// honours the variable at window creation; it composes with
/// `[ui] scale` (OS-DPI alignment × token multiplier).
fn observe_scale_escape_hatch() {
    match std::env::var("SLINT_SCALE_FACTOR") {
        Ok(value) => tracing::info!(
            value = %value,
            "SLINT_SCALE_FACTOR set: the winit backend will apply it at window creation"
        ),
        Err(std::env::VarError::NotPresent) => {}
        Err(std::env::VarError::NotUnicode(raw)) => {
            tracing::warn!(?raw, "SLINT_SCALE_FACTOR is not valid unicode; ignoring");
        }
    }
}

/// The desktop UI lifecycle (D4/D5): boot the core exactly like the
/// daemon path (same store selection, `LogSink` notifier), build the
/// UI hand-over, spawn the command forwarder, mount the window, then
/// run the slint event loop **on the process main thread** (we are
/// inside `block_on`; the multi-thread runtime's workers keep driving
/// every actor). Exit paths: window close ends the loop; `ctrl_c`/SIGTERM
/// land via the exit watcher calling [`taskboard_ui_slint::quit`] —
/// both reach the same graceful shutdown.
///
/// # Errors
///
/// Bootstrap, mount, and event-loop failures, as typed errors.
pub async fn run_desktop(config: crate::config::AppConfig) -> color_eyre::Result<()> {
    // D13: the env var is read before anything slint-side.
    observe_scale_escape_hatch();

    let scale = config.ui.scale;
    let store = crate::secrets::resolve_credential_store(config.nextcloud.credential_store);
    let app =
        crate::bootstrap::bootstrap(config, store, Arc::new(crate::notifications::LogSink)).await?;

    // The hand-over (D6): the app keeps the receiver half of the
    // command channel and forwards; the UI never sees an EngineHandle.
    let (commands_tx, commands_rx) = mpsc::channel(UI_COMMAND_CAPACITY);
    let forwarder = spawn_ui_command_forwarder(commands_rx, app.engine.clone());

    let handle = taskboard_ui_slint::mount(
        app.engine.shared_state(),
        app.engine.subscribe(),
        app.system.subscribe(),
        commands_tx,
        taskboard_ui_slint::UiSettings { scale },
    )?;

    // D5b: the physical exit paths are wired so shutdown is always
    // graceful (`app.exit` as an action is chunk 2). Thin main-glue
    // class — deliberately untested; the forwarder and config carry
    // the tests.
    let exit_watcher = tokio::spawn(async {
        crate::bootstrap::wait_for_signal().await;
        tracing::info!("shutdown signal received");
        taskboard_ui_slint::quit();
    });

    let run_result = taskboard_ui_slint::run(handle);

    exit_watcher.abort();
    forwarder.abort();
    let outcome = app.shutdown().await;
    if outcome.actor_aborted {
        tracing::warn!("sync actor did not stop in time and was aborted");
    }
    run_result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Fake sink: collects dispatch order/content hermetically (fakes
    /// over mocks; no engine spawned). The log lives behind an `Arc` so
    /// the test keeps a handle while the forwarder owns the sink.
    #[derive(Clone, Default)]
    struct FakeSink(Arc<Mutex<Vec<StateCommand>>>);

    impl CommandSink for FakeSink {
        fn dispatch(&self, command: StateCommand) {
            self.0.lock().expect("sink lock").push(command);
        }
    }

    fn sample_commands() -> Vec<StateCommand> {
        use taskboard_domain::StackId;
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        vec![
            StateCommand::CreateTask {
                title: "first".into(),
                stack,
                order: 0,
            },
            StateCommand::CreateTask {
                title: "second".into(),
                stack,
                order: 1,
            },
            StateCommand::CreateStack {
                title: "done".into(),
                order: 2,
            },
        ]
    }

    #[tokio::test]
    async fn forwards_the_sequence_in_order_and_exits_on_close() {
        let sink = FakeSink::default();
        let commands = sample_commands();
        let (tx, rx) = mpsc::channel::<StateCommand>(UI_COMMAND_CAPACITY);
        let forwarder = spawn_ui_command_forwarder(rx, sink.clone());

        for command in &commands {
            tx.send(command.clone())
                .await
                .expect("channel has capacity");
        }
        drop(tx); // the UI side going away ends the forwarder

        forwarder.await.expect("forwarder joins cleanly");
        let collected = sink.0.lock().expect("sink lock").clone();
        assert_eq!(collected, commands, "exact sequence, order preserved");
    }

    /// The pure D4 routing (test 17): `desktop` routes to the UI
    /// lifecycle regardless of TTY — the recorded behaviour change;
    /// `daemon` keeps the TTY-sensitive REPL rule.
    #[test]
    fn bare_invocation_routing_maps_modes_per_d4() {
        use crate::config::AppMode;
        assert_eq!(
            bare_invocation_route(AppMode::Desktop, true),
            BareRoute::DesktopUi,
            "bare+TTY desktop GUIs (D4 behaviour change)"
        );
        assert_eq!(
            bare_invocation_route(AppMode::Desktop, false),
            BareRoute::DesktopUi
        );
        assert_eq!(
            bare_invocation_route(AppMode::Daemon, true),
            BareRoute::Repl
        );
        assert_eq!(
            bare_invocation_route(AppMode::Daemon, false),
            BareRoute::DaemonLifecycle
        );
        // Kiosk stays refused downstream (the daemon-lifecycle gate);
        // it never routes to the UI in v0.1.0.
        assert_eq!(
            bare_invocation_route(AppMode::Kiosk, true),
            BareRoute::DaemonLifecycle
        );
    }

    #[tokio::test]
    async fn exits_immediately_when_the_channel_starts_closed() {
        let sink = FakeSink::default();
        // The sender binding is `_`, so the channel starts closed; the
        // receiver is passed to the forwarder as-is.
        let (_, rx) = mpsc::channel::<StateCommand>(UI_COMMAND_CAPACITY);
        let forwarder = spawn_ui_command_forwarder(rx, sink.clone());
        // Join-based terminal-state assert: no sleeps, no polling.
        forwarder
            .await
            .expect("forwarder exits on a closed channel");
        assert!(sink.0.lock().expect("sink lock").is_empty());
    }
}
