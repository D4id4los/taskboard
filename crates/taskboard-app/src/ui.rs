// SPDX-License-Identifier: AGPL-3.0-only
//! The UI lifecycle and adaptation layer (GUI Phase 7, chunk 1).
//!
//! `taskboard-app` owns every IO seam and hands the presentation crate
//! typed values and channel halves (design §1.2): this module adapts the
//! booted core ([`crate::bootstrap::App`]) onto the UI's hand-over set.
//! Chunk 1 carries the *UI-command forwarder* — the actor that drains
//! the UI's command channel into the state engine; the desktop entry
//! lifecycle lands in this module too.

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

/// The UI → app command channel at its house capacity — one
/// construction site for both UI lifecycles (desktop and kiosk
/// appliance).
#[must_use]
pub fn ui_command_channel() -> (mpsc::Sender<StateCommand>, mpsc::Receiver<StateCommand>) {
    mpsc::channel(UI_COMMAND_CAPACITY)
}

/// The Q15 startup-warning surfacing: typed keymap findings logged
/// with structured fields — the keymap still applied (drops fall back
/// to preset bindings). Part of the UI hand-over contract, shared by
/// the desktop and kiosk-appliance lifecycles.
pub fn surface_keymap_findings(findings: &[taskboard_ui_slint::keymap::Finding]) {
    for finding in findings {
        tracing::warn!(
            kind = ?finding.kind,
            action = ?finding.action,
            context = %finding.context,
            stroke = %finding.stroke,
            "keymap override finding (preset binding wins)"
        );
    }
}

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
    /// Non-TTY daemon mode with no display either (systemd, ssh, cron
    /// — or the TTY-less REPL refusal, resolved by `repl::entry_point`):
    /// the daemon lifecycle.
    DaemonLifecycle,
    /// `mode = "kiosk"`: the appliance lifecycle (ADR 0016) — the daemon
    /// core plus the display-gated on-seat window. TTY-independent: a
    /// seat has no operator TTY (`ssh -t` booting the appliance is
    /// acceptable); the headless fallback is decided inside the
    /// lifecycle, not by this routing.
    KioskAppliance,
}

/// The kiosk appliance's start decision (Q3): whether a display
/// context gates the window mount. Decided *before* any
/// slint-touching call — without a display, slint 1.18.1 panics
/// during backend init (exit 101) instead of surfacing the typed
/// mount error, so the appliance falls back to the headless daemon
/// lifecycle, loudly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KioskStart {
    /// A display context exists: mount the fullscreen appliance
    /// window over the booted core.
    Appliance,
    /// No display: keep serving headless (the loud fallback —
    /// `tracing::error!` then the shared daemon tail).
    Headless,
}

/// Pure decision over the display facts ([`display_env_available`]
/// is the untested env glue, per the main-glue convention).
#[must_use]
pub fn kiosk_start(display: bool) -> KioskStart {
    if display {
        KioskStart::Appliance
    } else {
        KioskStart::Headless
    }
}

/// Whether a graphical session context exists, from the two session
/// environment variables. A variable that is set but *empty* is a
/// broken-session artifact and counts as absent. Pure so the #236
/// detection rule is testable without a display (and so the kiosk
/// wave's display gate can consume the same fn).
#[must_use]
pub fn display_available(wayland: Option<&str>, x11: Option<&str>) -> bool {
    wayland.is_some_and(|v| !v.is_empty()) || x11.is_some_and(|v| !v.is_empty())
}

/// Trivial env glue over [`display_available`]: reads
/// `WAYLAND_DISPLAY`/`DISPLAY`. Untested per the main-glue convention.
/// Pub because the binary's bare-invocation wiring is the caller.
#[must_use]
pub fn display_env_available() -> bool {
    display_available(
        std::env::var_os("WAYLAND_DISPLAY")
            .as_deref()
            .and_then(|v| v.to_str()),
        std::env::var_os("DISPLAY")
            .as_deref()
            .and_then(|v| v.to_str()),
    )
}

/// The pure D4 mode→route mapping, extracted so the behaviour change
/// (bare+TTY `desktop` now GUIs instead of opening a REPL) is
/// unit-testable without a display.
///
/// Daemon mode without a TTY splits by display presence (#236): a
/// file-manager launch inherits the graphical session's display
/// variables and boots the desktop UI instead of silently starting a
/// headless daemon; ssh/systemd/cron (no TTY, no display) keep the
/// daemon lifecycle, and the explicit `daemon` subcommand stays the
/// documented headless entry for sessions that do import a display.
#[must_use]
pub fn bare_invocation_route(mode: crate::config::AppMode, tty: bool, display: bool) -> BareRoute {
    match mode {
        crate::config::AppMode::Desktop => BareRoute::DesktopUi,
        crate::config::AppMode::Daemon if tty => BareRoute::Repl,
        crate::config::AppMode::Daemon if display => BareRoute::DesktopUi,
        crate::config::AppMode::Kiosk => BareRoute::KioskAppliance,
        crate::config::AppMode::Daemon => BareRoute::DaemonLifecycle,
    }
}

/// Resolves the `[ui]` table into the UI crate's typed hand-over
/// (D14): the validated effective keymap plus the Q15 startup-warning
/// summary. Pure — the figment layer stops at raw strings; the
/// keymap layer owns value validation. Findings are *returned* (the
/// caller surfaces them via `tracing`); nothing here can fail —
/// a hostile keymap never blocks UI start (Q15).
///
/// # Panics
///
/// Never on a validated config: `ui.scale` is minted via
/// `UiScale::new` and `AppConfig::validate` has already range-checked
/// it. Calling on an unvalidated `[ui]` table panics — same rule as
/// `AppConfig::server_url`/`username`.
#[must_use]
pub fn resolve_ui_settings(
    ui: &crate::config::UiSection,
    day_start: (chrono::NaiveTime, chrono::FixedOffset),
) -> (
    taskboard_ui_slint::UiSettings,
    Vec<taskboard_ui_slint::keymap::Finding>,
) {
    let raw = taskboard_ui_slint::keymap::RawKeymap {
        preset: match ui.keymap.preset {
            crate::config::KeymapPreset::Standard => taskboard_ui_slint::keymap::Preset::Standard,
            crate::config::KeymapPreset::Emacs => taskboard_ui_slint::keymap::Preset::Emacs,
            crate::config::KeymapPreset::Vi => taskboard_ui_slint::keymap::Preset::Vi,
        },
        overrides: ui
            .keymap
            .overrides
            .iter()
            .map(|(id, spec)| (id.clone(), spec.to_sequences()))
            .collect(),
    };
    let built = taskboard_ui_slint::keymap::build(&raw);
    let settings = taskboard_ui_slint::UiSettings {
        scale: taskboard_ui_slint::UiScale::new(ui.scale)
            .expect("ui.scale validated by AppConfig::validate — call only on a validated config"),
        startup: taskboard_ui_slint::adapter::StartupWarnings {
            keymap_findings: built.findings.len().try_into().unwrap_or(u32::MAX),
        },
        keymap: built.keymap,
        kiosk: false, // the desktop hand-over; the kiosk appliance (K3) sets this
        // The device-local widths (round-1 D16): copied verbatim —
        // uuid-keyed both sides, the mount converts to domain newtypes.
        stack_widths: ui.stack_widths.clone(),
        connectors: Vec::new(), // filled by the lifecycle (spawn sites) from the full config
        boards: Vec::new(),     // filled by the lifecycle from the engine state (r2 D32)
        keymap_preset: match ui.keymap.preset {
            crate::config::KeymapPreset::Standard => taskboard_ui_slint::keymap::Preset::Standard,
            crate::config::KeymapPreset::Emacs => taskboard_ui_slint::keymap::Preset::Emacs,
            crate::config::KeymapPreset::Vi => taskboard_ui_slint::keymap::Preset::Vi,
        },
        // The today window's derivation pair (chunk 10 D4): resolved
        // once here — nothing reads config mid-flight.
        day_start,
        // Overwritten by the lifecycle from the engine config right
        // after this returns (the kiosk/connectors/boards precedent).
        reminder_defaults: taskboard_domain::ReminderDefaults::default(),
    };
    (settings, built.findings)
}

/// The today window's derivation pair (chunk 10 D4): the parsed
/// `[preferences] day_start_time` (config validation owns the refusal)
/// plus the device's fixed UTC offset at derivation time — the
/// `day_window` consumers contract verbatim.
#[must_use]
pub fn resolve_day_start(
    config: &crate::config::AppConfig,
) -> (chrono::NaiveTime, chrono::FixedOffset) {
    let day_start_time = config
        .preferences
        .day_start_time
        .parse::<chrono::NaiveTime>()
        .unwrap_or_default();
    let offset = *chrono::Local::now().offset();
    (day_start_time, offset)
}

/// The secret-free connector list for the settings hand-over (round-1
/// D2): `configured_connectors` (F-C4, the single source of "which
/// connectors exist") mapped onto the UI crate's row type. Resolved by
/// both UI lifecycles from the full config — `resolve_ui_settings`
/// deliberately sees only the `[ui]` table.
///
/// # Errors
///
/// [`ConfigError::InvalidConnectorName`] when the config's connector
/// names fail the kebab predicate — a raw, unvalidated config.
#[allow(clippy::result_large_err)] // mirrors the enum's one grant above
pub fn resolve_connector_infos(
    config: &crate::config::AppConfig,
) -> Result<Vec<taskboard_ui_slint::requests::ConnectorInfo>, crate::config::ConfigError> {
    // The legacy-singleton world (no array) synthesizes its one row
    // read-only (D4): displayed, refused on mutation.
    let legacy = config.sync.connectors.is_none();
    Ok(crate::connectors::configured_connectors(config)?
        .into_iter()
        .map(|spec| taskboard_ui_slint::requests::ConnectorInfo {
            name: spec.name.to_string(),
            server_url: spec.server_url,
            username: spec.username,
            read_only: legacy,
        })
        .collect())
}

/// The board-selector rows for the settings hand-over (r2 D32): the
/// live boards (`!deleted && !archived` — the same rule the board
/// surface renders by, so the surface and the selector agree) with
/// their binding's connector name. Pure over the state snapshot; the
/// [`EngineHandle`](taskboard_state::EngineHandle) wrapper below is
/// the lifecycle-facing convenience.
#[must_use]
pub fn resolve_board_infos_from_state(
    state: &taskboard_domain::AppState,
) -> Vec<taskboard_ui_slint::requests::BoardInfo> {
    taskboard_ui_slint::presentation::live_boards(state)
        .into_iter()
        .map(|board| taskboard_ui_slint::requests::BoardInfo {
            id: board.id.as_uuid(),
            name: board.title.clone(),
            bound_connector: board
                .remote
                .as_ref()
                .map(|binding| binding.connector.to_string()),
        })
        .collect()
}

/// The engine-facing wrapper over
/// [`resolve_board_infos_from_state`] — the shape both UI lifecycles
/// call at hand-over (r2 D32).
#[must_use]
pub fn resolve_board_infos(engine: &EngineHandle) -> Vec<taskboard_ui_slint::requests::BoardInfo> {
    resolve_board_infos_from_state(&engine.shared_state().load())
}

/// The OS-DPI escape hatch (§6.2, D13): reads `SLINT_SCALE_FACTOR`
/// *before* any slint-touching call and logs its presence. The app
/// never sets it programmatically — the user's env is the input;
/// observability is the app's whole duty toward it. The winit backend
/// honours the variable at window creation; it composes with
/// `[ui] scale` (OS-DPI alignment × token multiplier). Pub because the
/// kiosk appliance's mount path observes the same hatch (ADR 0016).
pub fn observe_scale_escape_hatch() {
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
/// run the slint event loop on the process main thread — see
/// [`run_desktop`].
///
/// Quits the slint event loop reliably from outside the loop thread.
/// A bare `quit()` before the loop exists is dropped by the backend
/// (`Ok` with no delivery), so: a quit landing pre-boot is parked by
/// [`taskboard_ui_slint::quit`] on the pre-loop flag ([`run`] honors
/// it), and here we additionally wait for the loop to actually run —
/// bounded at ~10 s — before delivering the quit through the live
/// proxy. Giving up is logged; `run`'s own result decides the outcome.
pub async fn quit_until_accepted() {
    let mut waited_ms = 0u32;
    while !taskboard_ui_slint::event_loop_running() {
        if waited_ms >= 10_000 {
            tracing::warn!("event loop never started; quit not delivered through the proxy");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        waited_ms += 50;
    }
    if let Err(err) = taskboard_ui_slint::quit() {
        tracing::warn!(error = %err, "event loop refused the quit");
    }
}

/// The desktop entry (`taskboard gui`): boots the core, mounts the
/// window, and run the slint event loop **on the process main thread**
/// (we are inside `block_on`; the multi-thread runtime's workers keep
/// driving every actor). Exit paths: window close ends the loop;
/// `ctrl_c`/SIGTERM land via the exit watcher calling
/// [`taskboard_ui_slint::quit`] — both reach the same graceful
/// shutdown.
///
/// # Errors
///
/// Bootstrap, mount, and event-loop failures, as typed errors.
pub async fn run_desktop(
    config: crate::config::AppConfig,
    config_write_path: std::path::PathBuf,
) -> color_eyre::Result<()> {
    // D13: the env var is read before anything slint-side.
    observe_scale_escape_hatch();

    let (mut ui_settings, keymap_findings) =
        resolve_ui_settings(&config.ui, resolve_day_start(&config));
    // The reminder defaults ride the hand-over (R3 chunk C): the
    // notification surface derives from the same value the engine
    // evaluates with.
    ui_settings.reminder_defaults = config.reminder_engine_config().defaults;
    surface_keymap_findings(&keymap_findings);
    let store = crate::secrets::resolve_credential_store(
        config.nextcloud.credential_store,
        config.credential_base_dir(),
    );
    let sink = crate::bootstrap::resolve_notifier_sink(&config);
    let app = crate::bootstrap::bootstrap(config, store, sink).await?;

    // The hand-over (D6): the app keeps the receiver half of the
    // command channel and forwards; the UI never sees an EngineHandle.
    let (commands_tx, commands_rx) = ui_command_channel();
    let forwarder = spawn_ui_command_forwarder(commands_rx, app.engine.clone());
    let (requests_tx, requests_rx) = crate::ui_requests::ui_request_channel();
    let request_forwarder = crate::ui_requests::spawn_ui_request_forwarder(
        requests_rx,
        app.engine.shared_state(),
        crate::ui_requests::ConnectionWorkspace {
            config_write_path,
            store: crate::secrets::resolve_credential_store(
                app.config.nextcloud.credential_store,
                app.config.credential_base_dir(),
            ),
            control: app.connector_control(),
            engine: app.engine.clone(),
        },
    );

    ui_settings.connectors = resolve_connector_infos(&app.config)?;
    ui_settings.boards = resolve_board_infos(&app.engine);
    let handle = taskboard_ui_slint::mount(
        app.engine.shared_state(),
        app.engine.subscribe(),
        app.system.subscribe(),
        commands_tx,
        requests_tx,
        ui_settings,
    )?;

    // D5b: the physical exit paths are wired so shutdown is always
    // graceful (`app.exit` as an action is chunk 2). Thin main-glue
    // class — deliberately untested; the forwarder and config carry
    // the tests.
    let exit_watcher = tokio::spawn(async {
        crate::bootstrap::wait_for_signal().await;
        tracing::info!("shutdown signal received");
        quit_until_accepted().await;
    });

    let run_result = taskboard_ui_slint::run(handle);

    exit_watcher.abort();
    forwarder.abort();
    request_forwarder.abort();
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
                board: taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7()),
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

    /// The pure D4 routing: `desktop` routes to the UI lifecycle
    /// regardless of TTY; `daemon` keeps the TTY-sensitive REPL rule
    /// and splits the non-TTY cell by display presence (#236 — a
    /// file-manager launch boots the GUI, ssh/systemd/cron stay
    /// headless); `kiosk` is the appliance lifecycle in every cell
    /// (ADR 0016).
    #[test]
    fn bare_invocation_routing_maps_modes_per_d4() {
        use crate::config::AppMode;
        // Desktop is display-blind and TTY-blind (D4).
        for tty in [true, false] {
            for display in [true, false] {
                assert_eq!(
                    bare_invocation_route(AppMode::Desktop, tty, display),
                    BareRoute::DesktopUi
                );
            }
        }
        // Daemon + TTY is the REPL under either display value.
        for display in [true, false] {
            assert_eq!(
                bare_invocation_route(AppMode::Daemon, true, display),
                BareRoute::Repl
            );
        }
        // The #236 fix: non-TTY daemon with a graphical session boots
        // the GUI instead of a silent headless daemon.
        assert_eq!(
            bare_invocation_route(AppMode::Daemon, false, true),
            BareRoute::DesktopUi
        );
        // Non-TTY daemon without a display stays the daemon lifecycle
        // (systemd, ssh, cron).
        assert_eq!(
            bare_invocation_route(AppMode::Daemon, false, false),
            BareRoute::DaemonLifecycle
        );
        // Kiosk is the appliance lifecycle in every cell: TTY-
        // and display-independent (ADR 0016) — the headless fallback
        // is decided inside the appliance, not by this routing.
        for tty in [true, false] {
            for display in [true, false] {
                assert_eq!(
                    bare_invocation_route(AppMode::Kiosk, tty, display),
                    BareRoute::KioskAppliance
                );
            }
        }
    }

    /// The Q3 start decision: a display context gates the window
    /// mount; without one the appliance falls back to the headless
    /// daemon lifecycle (loudly, at the caller).
    #[test]
    fn kiosk_start_splits_the_display_gate() {
        assert_eq!(kiosk_start(true), KioskStart::Appliance);
        assert_eq!(kiosk_start(false), KioskStart::Headless);
    }

    /// The #236 detection rule: a graphical session exists when either
    /// session variable is set non-empty; empty or absent counts as no
    /// display (broken-session artifact).
    #[test]
    fn display_available_needs_one_non_empty_session_var() {
        assert!(display_available(Some("wayland-0"), None));
        assert!(display_available(None, Some(":0")));
        assert!(display_available(Some(""), Some(":0")));
        assert!(!display_available(Some(""), Some("")));
        assert!(!display_available(None, None));
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

#[cfg(test)]
mod keymap_resolution_tests {
    use super::*;
    use crate::config::{KeymapPreset, KeymapSection, OverrideSpec};
    use std::collections::BTreeMap;

    /// T30 — the hand-over is typed and total: valid overrides land
    /// in `UiSettings.keymap`, the findings count rides
    /// `StartupWarnings`, and a hostile table changes nothing but the
    /// findings (never an error — Q15).
    #[test]
    fn resolve_hands_over_keymap_and_counts_findings() {
        // Hostile config: unknown action, malformed stroke, and a
        // conflict with the preset — plus one valid override.
        let ui = crate::config::UiSection {
            scale: 1.25,
            stack_widths: BTreeMap::new(),
            keymap: KeymapSection {
                preset: KeymapPreset::Standard,
                overrides: BTreeMap::from([
                    (
                        "ghost.action".to_owned(),
                        OverrideSpec::Single("X".to_owned()),
                    ),
                    (
                        "task.complete".to_owned(),
                        OverrideSpec::Single("Ctrl+!!".to_owned()),
                    ),
                    (
                        "app.undo".to_owned(),
                        OverrideSpec::Single("Ctrl+Q".to_owned()),
                    ),
                    (
                        "app.exit".to_owned(),
                        OverrideSpec::Single("Ctrl+\\".to_owned()),
                    ),
                ]),
            },
        };
        let (settings, findings) = resolve_ui_settings(
            &ui,
            (
                chrono::NaiveTime::MIN,
                chrono::FixedOffset::east_opt(0).expect("zero"),
            ),
        );

        // Structural asserts on the handed-over map (no strings).
        assert_eq!(settings.scale.get(), 1.25);
        assert_eq!(
            settings.startup.keymap_findings,
            u32::try_from(findings.len()).expect("findings fit u32")
        );
        assert_eq!(findings.len(), 3, "exhaustive: every offender listed");
        let seq = taskboard_ui_slint::keymap::stroke::parse("Ctrl+\\")
            .expect("the valid override's stroke parses");
        assert_eq!(
            settings
                .keymap
                .resolve(&[taskboard_ui_slint::actions::ContextId::Global], &[seq]),
            Some(taskboard_ui_slint::actions::ActionId::AppExit),
            "the one valid override landed in the keymap"
        );

        // The dropped overrides left the preset bindings intact.
        let undo = taskboard_ui_slint::keymap::stroke::parse("Ctrl+Z").expect("parses");
        assert_eq!(
            settings
                .keymap
                .resolve(&[taskboard_ui_slint::actions::ContextId::Global], &[undo]),
            Some(taskboard_ui_slint::actions::ActionId::AppUndo),
            "preset fallback survives the dropped override"
        );
    }

    /// The vi preset selection carries through to the handed-over
    /// keymap (mode contexts bound).
    #[test]
    fn preset_selection_reaches_the_keymap() {
        let ui = crate::config::UiSection {
            keymap: KeymapSection {
                preset: KeymapPreset::Vi,
                overrides: BTreeMap::new(),
            },
            ..crate::config::UiSection::default()
        };
        let (settings, findings) = resolve_ui_settings(
            &ui,
            (
                chrono::NaiveTime::MIN,
                chrono::FixedOffset::east_opt(0).expect("zero"),
            ),
        );
        assert_eq!(findings, Vec::new());
        let colon = taskboard_ui_slint::keymap::stroke::parse(":").expect("parses");
        assert_eq!(
            settings.keymap.resolve(
                &[taskboard_ui_slint::actions::ContextId::ViNormal],
                &[colon]
            ),
            Some(taskboard_ui_slint::actions::ActionId::AppMenu),
            "vi preset bound app.menu in the normal-mode context"
        );
    }

    /// T-18's app half — the resolved hand-over carries the config's
    /// width map verbatim (the mount's uuid→newtype conversion is the
    /// T-13 load path).
    #[test]
    fn resolve_copies_the_stack_width_map() {
        let board = uuid::Uuid::from_u128(0x1001);
        let stack = uuid::Uuid::from_u128(0x2001);
        let ui = crate::config::UiSection {
            stack_widths: BTreeMap::from([(board, BTreeMap::from([(stack, 240u32)]))]),
            ..crate::config::UiSection::default()
        };
        let (settings, _) = resolve_ui_settings(
            &ui,
            (
                chrono::NaiveTime::MIN,
                chrono::FixedOffset::east_opt(0).expect("zero"),
            ),
        );
        assert_eq!(
            settings
                .stack_widths
                .get(&board)
                .and_then(|s| s.get(&stack)),
            Some(&240u32)
        );
    }

    /// A1 (r2 D32) — the board-selector resolution over a hand-built
    /// `AppState`: live boards only (deleted/archived drop), the
    /// binding's connector name rides the row, uuid round-trips.
    #[test]
    fn board_infos_resolve_live_boards_with_binding_names() {
        use taskboard_domain::{Board, BoardClocks, BoardId, RemoteBoardBinding, RemoteBoardId};
        let live_unbound = BoardId::from_uuid(uuid::Uuid::from_u128(0x1001));
        let live_bound = BoardId::from_uuid(uuid::Uuid::from_u128(0x1002));
        let archived = BoardId::from_uuid(uuid::Uuid::from_u128(0x1003));
        let deleted = BoardId::from_uuid(uuid::Uuid::from_u128(0x1004));
        let epoch = BoardClocks::epoch();
        let board = |id, title: &str, remote| Board {
            id,
            remote,
            title: title.to_owned(),
            color: taskboard_domain::Color::new(""),
            archived: id == archived,
            deleted: id == deleted,
            clocks: BoardClocks {
                title: epoch,
                color: epoch,
                archived: epoch,
                deleted: epoch,
            },
            remote_seen: None,
        };
        let mut state = taskboard_domain::AppState::default();
        state.boards.insert(
            live_bound,
            board(
                live_bound,
                "bound",
                Some(RemoteBoardBinding {
                    connector: taskboard_domain::ConnectorId::from("work"),
                    board: RemoteBoardId(7),
                }),
            ),
        );
        state
            .boards
            .insert(live_unbound, board(live_unbound, "free", None));
        state.boards.insert(archived, board(archived, "old", None));
        state.boards.insert(deleted, board(deleted, "gone", None));

        let rows = resolve_board_infos_from_state(&state);
        assert_eq!(rows.len(), 2, "only the live boards resolve");
        let bound = rows.iter().find(|r| r.name == "bound").expect("row");
        assert_eq!(bound.id, live_bound.as_uuid(), "uuid round-trips");
        assert_eq!(bound.bound_connector.as_deref(), Some("work"));
        let free = rows.iter().find(|r| r.name == "free").expect("row");
        assert_eq!(free.bound_connector, None);
    }
}
