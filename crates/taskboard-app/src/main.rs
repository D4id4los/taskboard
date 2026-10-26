// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard` application runner.
//!
//! Thin process glue (ADRs 0008/0009/0012): parse the CLI, initialize
//! `tracing`, then either hand the subcommand to the library's
//! `commands::dispatch` (typed handlers, exit codes) — transparently
//! relayed over the daemon's IPC socket when one serves this database —
//! or run the daemon lifecycle: boot the in-process core through
//! `bootstrap`, serve the per-database IPC socket, wait for the signal
//! or `daemon stop`, graceful drain with a second-signal force exit.
//! All wiring lives in the `taskboard_app` library so tests and future
//! frontends reuse it.

#![forbid(unsafe_code)]

use std::io::IsTerminal as _;
use std::io::Write as _;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser as _;

use taskboard_app::App;
use taskboard_app::bootstrap::{bootstrap, wait_for_signal};
use taskboard_app::cli::{Cli, Command, DaemonCmd};
use taskboard_app::commands::{
    BasicSession, CommandCtx, CommandError, IpcRoute, dispatch, report, route_for_ipc,
};
use taskboard_app::config::AppConfig;
use taskboard_app::ipc::client;

fn init_tracing() {
    // The console subscriber is compile-time optional but runtime
    // opt-in: `console_subscriber::init` unconditionally binds gRPC
    // 127.0.0.1:6669 and panics when the port is taken. An
    // `--all-features` build (coverage CI, release checks) would
    // otherwise kill every spawned `taskboard` process that loses that
    // bind — parallel test runs and multi-process tooling included.
    #[cfg(feature = "tokio-console")]
    if std::env::var_os("TASKBOARD_CONSOLE").is_some() {
        console_subscriber::init();
        return;
    }

    {
        use tracing_subscriber::EnvFilter;
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            // Sensible default: info from our crates, warnings from deps.
            "info,tower=warn,hyper=warn,reqwest=warn".to_owned().into()
        });
        // `TASKBOARD_LOG_FORMAT=json` swaps the same fmt builder to
        // one-line JSON events on the same stderr writer — the
        // machine-readable log stream the process smoke harnesses
        // select structured fields on. Unset (or any other value)
        // keeps the human-pretty default byte-identical. Unrelated to
        // the CLI `--json` payload flag: logs vs command payloads.
        let format_json = std::env::var("TASKBOARD_LOG_FORMAT")
            .as_deref()
            .is_ok_and(|v| v.eq_ignore_ascii_case("json"));
        let builder = tracing_subscriber::fmt().with_env_filter(filter);
        // Logs go to stderr: stdout carries the `--json` payloads
        // (and human output) — a scripting surface, not a log sink.
        if format_json {
            builder.json().with_writer(std::io::stderr).init();
        } else {
            builder.with_writer(std::io::stderr).init();
        }
    }
}

fn main() -> color_eyre::Result<()> {
    // Rich, colorized error reports for the top-level binary.
    color_eyre::install()?;

    init_tracing();
    // The raw argv is captured *before* clap parses: the IPC path
    // relays these words verbatim (the server is the single parse
    // authority; clap's types cannot faithfully reconstruct quoting).
    let raw_argv: Vec<String> = std::env::args().skip(1).collect();
    let cli = Cli::parse();
    tracing::info!(version = taskboard_app::VERSION, "starting taskboard");

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run(cli, raw_argv))?;

    tracing::info!("taskboard shut down cleanly");
    Ok(())
}

async fn run(mut cli: Cli, raw_argv: Vec<String>) -> color_eyre::Result<()> {
    // Subcommand arms are lib code (`commands::dispatch`); the process
    // exits with the returned code (0 success / 1 runtime failure).
    // Exit 2 is clap's usage error, 130 the second-signal force exit.
    // The daemon-*run* arm (`daemon` with no subcommand) falls through
    // to the daemon lifecycle, never into the bare-invocation entry
    // (the review-2026-10-11 regression: a `take()` inside the guard
    // swallowed it and a TTY then opened the REPL).
    let bare_invocation = cli.command.is_none();
    let daemon_run = matches!(cli.command, Some(Command::Daemon { cmd: None }));
    if let Some(command) = cli.command.take()
        && !daemon_run
    {
        // The explicit `gui` verb is a process-lifecycle arm (the daemon
        // run arm's mirror): frontend selection, not a command to relay
        // or dispatch — a relay would try to open the window in the
        // daemon's process. Handled before the transport routing; the
        // lane table pins it `Local` (never relayed).
        if matches!(command, Command::Gui) {
            let config = match AppConfig::load(cli.config.clone()) {
                Ok(config) => config,
                Err(err) => {
                    std::process::exit(i32::from(report(cli.json, &CommandError::from(err))))
                }
            };
            let config_write_path = AppConfig::write_path(cli.config.clone());
            return taskboard_app::ui::run_desktop(config, config_write_path).await;
        }
        // Transport auto-routing (ADR 0012): silent in both human and
        // JSON modes — the observable contract (exit code, stdout
        // payload, stderr envelope) is byte-identical on both paths.
        match route_for_ipc(&command) {
            IpcRoute::IpcOnly => {
                let code = match command {
                    Command::Status => status_command(cli.config.take(), cli.json).await,
                    Command::Daemon {
                        cmd: Some(DaemonCmd::Stop),
                    } => daemon_stop_command(cli.config.take(), cli.json).await,
                    // `daemon` with no subcommand is the run arm above.
                    _ => unreachable!("the IpcOnly lanes are status and daemon stop"),
                };
                std::process::exit(i32::from(code));
            }
            IpcRoute::IpcOrLocal => {
                // A resolved config derives the socket; a rejected
                // config reports typed through the in-process dispatch,
                // exactly as before.
                if let Ok(config) = AppConfig::load(cli.config.clone())
                    && let Some(conn) = client::try_connect(config.db_path()).await
                {
                    match conn.exec(&raw_argv).await {
                        Ok(outcome) => relay(&outcome),
                        // Mid-request death: the daemon holds the lock,
                        // so in-process fallback would only re-report
                        // `instance-locked`; the typed failure is the
                        // honest signal.
                        Err(err) => std::process::exit(i32::from(report(cli.json, &err))),
                    }
                }
            }
            IpcRoute::Local => {}
        }

        let mut out = std::io::stdout();
        let mut err = std::io::stderr();
        let ctx = CommandCtx {
            config_path: AppConfig::config_path(cli.config),
            io: taskboard_app::commands::CommandIo {
                out: &mut out,
                err: &mut err,
                json: cli.json,
            },
        };
        let code = dispatch(command, ctx).await;
        std::process::exit(i32::from(code));
    }

    // Bare `taskboard` routing (D4, amended per the ADR 0015 display
    // gate): `mode = "desktop"` boots the GUI *regardless of TTY* —
    // `AppMode` is frontend selection (ADR 0008), and launching from a
    // terminal is how desktop apps start. The REPL stays the TTY
    // front-end of `daemon` mode (ADR 0011); a non-TTY daemon inside a
    // graphical session boots the GUI (#236), a daemon with neither
    // TTY nor display keeps the daemon lifecycle, and a bare `kiosk`
    // boots the appliance (ADR 0016). An explicit `daemon` subcommand
    // is `Some` above and skips this routing entirely.
    if bare_invocation {
        let config = match AppConfig::load(cli.config.clone()) {
            Ok(config) => config,
            // The session boot's typed failure class, reported through
            // the same envelope a one-shot uses (JSON under `--json`).
            Err(err) => std::process::exit(i32::from(report(cli.json, &CommandError::from(err)))),
        };
        match taskboard_app::ui::bare_invocation_route(
            config.app.mode,
            std::io::stdin().is_terminal(),
            taskboard_app::ui::display_env_available(),
        ) {
            taskboard_app::ui::BareRoute::DesktopUi => {
                let config_write_path = AppConfig::write_path(cli.config.clone());
                return taskboard_app::ui::run_desktop(config, config_write_path).await;
            }
            taskboard_app::ui::BareRoute::KioskAppliance => {
                // The appliance owns its own mode handling — the display
                // gate and the loud headless fallback live inside
                // `run_kiosk` (ADR 0016), not in this routing.
                return run_kiosk(config, cli.config, cli.json).await;
            }
            taskboard_app::ui::BareRoute::DaemonLifecycle => {
                // The daemon lifecycle's mode gate refuses `Desktop`
                // (no daemon-side desktop frontend).
                return daemon_lifecycle(config, cli.config, cli.json).await;
            }
            taskboard_app::ui::BareRoute::Repl => {
                let code = taskboard_app::repl::run_session(config, cli.config, cli.json).await;
                std::process::exit(i32::from(code));
            }
        }
    }

    let config = match AppConfig::load(cli.config.clone()) {
        Ok(config) => config,
        Err(err) => std::process::exit(i32::from(report(cli.json, &CommandError::from(err)))),
    };
    daemon_lifecycle(config, cli.config, cli.json).await
}

/// Relays the daemon's rendered answer and exits with its code — the
/// client is a dumb relay; the contract is byte-identical to the
/// in-process dispatch (ADR 0012). Never returns.
fn relay(outcome: &client::ExecOutcome) -> ! {
    let mut out = std::io::stdout();
    let _ = out.write_all(outcome.out.as_bytes());
    let _ = out.flush();
    let mut err = std::io::stderr();
    let _ = err.write_all(outcome.err.as_bytes());
    let _ = err.flush();
    std::process::exit(i32::from(outcome.code));
}

/// `taskboard status` (ADR 0012): the daemon's live view over IPC, or
/// the client-side answer off the lock sidecar. Never boots a core,
/// never triggers a cycle; exit 0 both ways (an observation, not a
/// failure).
async fn status_command(config_path: Option<std::path::PathBuf>, json: bool) -> u8 {
    let config = match AppConfig::load(config_path) {
        Ok(config) => config,
        Err(err) => return report(json, &CommandError::from(err)),
    };
    let view = match client::try_connect(config.db_path()).await {
        Some(conn) => match conn.status().await {
            Ok(view) => view,
            Err(err) => return report(json, &err),
        },
        None => taskboard_app::commands::status::StatusView::no_daemon(config.db_path()),
    };
    // The one shared emission path (`dispatch`'s client-side arm uses
    // it too), so the two surfaces cannot drift.
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    let mut io = taskboard_app::commands::CommandIo {
        out: &mut out,
        err: &mut err,
        json,
    };
    taskboard_app::commands::status::emit(&mut io, &view);
    0
}

/// `taskboard daemon stop`: the graceful remote shutdown. Without a
/// daemon: typed `no-daemon`, exit 1.
async fn daemon_stop_command(config_path: Option<std::path::PathBuf>, json: bool) -> u8 {
    let config = match AppConfig::load(config_path) {
        Ok(config) => config,
        Err(err) => return report(json, &CommandError::from(err)),
    };
    let Some(conn) = client::try_connect(config.db_path()).await else {
        return report(json, &CommandError::NoDaemon);
    };
    match conn.shutdown().await {
        Ok(()) => 0,
        Err(err) => report(json, &err),
    }
}

/// The daemon lifecycle (explicit `daemon`, or the `[app] mode`
/// resolution from a bare non-TTY invocation): boot the core, serve the
/// per-database IPC socket, wait for the first signal or `daemon stop`,
/// drain the server, then run the graceful app shutdown — the
/// second-signal force exit stays armed throughout.
async fn daemon_lifecycle(
    config: AppConfig,
    config_path: Option<std::path::PathBuf>,
    json: bool,
) -> color_eyre::Result<()> {
    // The daemon's entry owns the `[app] mode` gate (the core is
    // mode-agnostic): `Desktop` has no daemon-side frontend — its
    // frontend is the GUI (`taskboard` bare or `taskboard gui`). `Kiosk`
    // passes: the explicit `daemon` verb is the headless maintenance
    // entry under a kiosk config too (ADR 0015 #236's unit rule; the
    // bare kiosk entry is the appliance, ADR 0016).
    if let mode @ taskboard_app::config::AppMode::Desktop = config.app.mode {
        tracing::error!(?mode, "desktop mode has no daemon-side frontend");
        color_eyre::eyre::bail!("app mode {mode:?} has no daemon-side frontend");
    }
    // Backend-agnostic (ADR 0010 decision 9, amended 2026-10-11): with
    // IPC the daemon's purpose is "own the core, serve commands" — a
    // `standalone` daemon gives scripters the same no-boot-per-command
    // latency the connector profile gets, and idling is over.
    let (app, server, web) = boot_and_serve(config, config_path, json, "daemon").await?;
    serve_and_shutdown(app, server, web).await
}

/// The shared headless boot (ADR 0016): resolve the credential store,
/// bootstrap the core (boot failures reported typed, the `--json`
/// envelope on stderr — `instance-locked` included), start the
/// system-event logger, and spawn the per-database IPC socket. One
/// core, N frontends (ADR 0008): the socket serves every core verb
/// while the lifecycle runs. `what` names the lifecycle in the
/// serving log line. The daemon lifecycle and the kiosk appliance
/// both enter here.
async fn boot_and_serve(
    config: AppConfig,
    config_path: Option<std::path::PathBuf>,
    json: bool,
    what: &'static str,
) -> color_eyre::Result<(
    Arc<App>,
    taskboard_app::ipc::server::IpcServerHandle,
    Option<taskboard_app::web::DashboardServerHandle>,
)> {
    let store = taskboard_app::secrets::resolve_credential_store(
        config.nextcloud.credential_store,
        config.credential_base_dir(),
    );

    let sink = taskboard_app::bootstrap::resolve_notifier_sink(&config);
    let app = match bootstrap(config, store.clone(), sink).await {
        Ok(app) => Arc::new(app),
        // Boot failures are reported typed (the `--json` envelope on
        // stderr), exactly like every other command — `instance-locked`
        // included: the lifecycle is a core boot, not a special
        // failure.
        Err(err) => std::process::exit(i32::from(report(json, &CommandError::from(err)))),
    };

    let _logger = taskboard_app::bootstrap::spawn_system_event_logger(&app.system);

    let session = Arc::new(BasicSession {
        config: app.config.clone(),
        store,
        config_path: AppConfig::config_path(config_path),
    });
    let server =
        match taskboard_app::ipc::server::spawn_server(app.clone(), session, app.config.db_path())
            .await
        {
            Ok(server) => server,
            Err(err) => return Err(err.into()),
        };
    tracing::info!(
        db = %app.config.db_path().display(),
        "{what} serving commands over ipc"
    );
    // The web kiosk dashboard (K11): opt-in through `[web]`, spawned
    // beside the IPC socket — the decode rules were enforced at config
    // load, the token ceremony and the LAN-bind refusal live in the
    // spawn. An absent or disabled table spawns nothing.
    let web = if app.config.web.enabled {
        match taskboard_app::web::spawn_dashboard(
            &app.config,
            app.engine.clone(),
            app.config.web.clone(),
        )
        .await
        {
            Ok(handle) => Some(handle),
            Err(err) => return Err(err.into()),
        }
    } else {
        None
    };
    Ok((app, server, web))
}

/// The shared serve→drain→shutdown tail (ADR 0016): wait for the first
/// exit — shutdown signal or `daemon stop` over IPC — then the bounded
/// drain with the second-signal force exit armed throughout. The daemon
/// lifecycle and the kiosk appliance's headless fallback both enter
/// here; the appliance's GUI branch enters [`drain_and_shutdown`]
/// directly (its serve phase already ended with the window).
async fn serve_and_shutdown(
    app: Arc<App>,
    server: taskboard_app::ipc::server::IpcServerHandle,
    web: Option<taskboard_app::web::DashboardServerHandle>,
) -> color_eyre::Result<()> {
    // The first-signal wait happens here — *before* arming the
    // force-exit watcher — because `ctrl_c` and SIGTERM notify every
    // listener: an early watcher would catch the first signal instead
    // of the second.
    tokio::select! {
        () = wait_for_signal() => {
            tracing::info!("shutdown signal received");
        }
        () = server.stopped() => {
            tracing::info!("shutdown requested over ipc");
        }
    }
    drain_and_shutdown(app, server, web).await
}

/// The drain half of the shared tail, without the serve wait: bounded
/// server join and graceful core shutdown via `shutdown_from_arc`, the
/// second-signal force exit armed throughout. A core still shared at
/// the tail is the typed F9-01 runtime failure.
async fn drain_and_shutdown(
    app: Arc<App>,
    server: taskboard_app::ipc::server::IpcServerHandle,
    web: Option<taskboard_app::web::DashboardServerHandle>,
) -> color_eyre::Result<()> {
    let shutdown_timeout = Duration::from_secs(app.config.app.shutdown_timeout_secs);
    let drain = async {
        // Bounded: past the budget the servers abort their tasks (a
        // wedged peer must never hold the daemon's shutdown hostage)
        // and the refcount is released either way. The dashboard's
        // drain rides the same budget as the IPC socket's.
        server.join(shutdown_timeout).await;
        if let Some(web) = web {
            web.join(shutdown_timeout).await;
        }
        taskboard_app::bootstrap::shutdown_from_arc(app).await
    };
    let outcome = tokio::select! {
        outcome = drain => outcome,
        () = wait_for_signal() => {
            tracing::warn!("second signal during shutdown; forcing exit");
            std::process::exit(130);
        }
    };
    // A shared core at the tail is a typed runtime failure (the salvage
    // flush+broadcast already ran) — exit 1 via eyre, never a panic.
    let outcome = outcome?;
    if outcome.actor_aborted {
        tracing::warn!("sync actor did not stop in time and was aborted");
    }
    Ok(())
}

/// The kiosk appliance lifecycle (ADR 0016, Q1/Q2/Q3): boot the daemon
/// core through the shared [`boot_and_serve`] (exactly the daemon
/// lifecycle's boot), then decide on the display context *before* any
/// slint-touching call (KDF-2: slint panics headless during backend
/// init). With a display: mount the fullscreen appliance window (the
/// ADR 0015 hand-over with `UiSettings.kiosk = true`) and watch three
/// exits — signal, `daemon stop`, window close — all converging on the
/// shared drain. Without one: fall back loudly to the headless daemon
/// tail, so the same binary serves both the seat and the ssh session.
/// The `[kiosk.rotation]` hand-over (K13): the validated table maps
/// into the typed state machine. `expect` is safe by construction —
/// `AppConfig::load` ran `validate()`, which refuses fewer than two
/// distinct modes and a sub-floor dwell, so the minting constructor
/// cannot fail here; a future caller bypassing validation invalidates
/// this constraint.
fn kiosk_rotation_handover(
    config: &AppConfig,
) -> Option<taskboard_ui_slint::view_state::KioskRotation> {
    config.kiosk.rotation.as_ref().map(|rotation| {
        taskboard_ui_slint::view_state::KioskRotation::new(
            rotation
                .modes
                .iter()
                .map(|mode| match mode {
                    taskboard_app::config::KioskRotationMode::Board => {
                        taskboard_ui_slint::view_state::KioskMode::Board
                    }
                    taskboard_app::config::KioskRotationMode::Today => {
                        taskboard_ui_slint::view_state::KioskMode::Today
                    }
                })
                .collect(),
            Duration::from_secs(rotation.dwell_secs),
        )
        .expect("kiosk rotation validated at boot")
    })
}

async fn run_kiosk(
    config: AppConfig,
    config_path: Option<std::path::PathBuf>,
    json: bool,
) -> color_eyre::Result<()> {
    let config_write_path = AppConfig::write_path(config_path.clone());
    let (app, server, mut web) =
        boot_and_serve(config, config_path, json, "kiosk appliance").await?;

    // The `[kiosk] fixed_board` pin's typed boot resolution (KDF-10):
    // the state is loaded (boards live in the ArcSwap), the display
    // gate has not run — the refusal covers the appliance *and* the
    // loud headless fallback, because the deployment contract does not
    // depend on display presence. Unknown/dead board = typed config
    // error, never a silent default, never a runtime warning.
    let live_boards: Vec<uuid::Uuid> = app
        .engine
        .shared_state()
        .load()
        .boards
        .keys()
        .map(|id| id.as_uuid())
        .collect();
    match app.config.resolve_fixed_board(&live_boards) {
        Ok(Some(id)) => tracing::info!(board = %id, "kiosk fixed board resolved"),
        Ok(None) => {}
        Err(err) => {
            // The server is already spawned — drain through the shared
            // tail (the mount-error arm's convention), then report
            // typed.
            let _ = drain_and_shutdown(app, server, web.take()).await;
            std::process::exit(i32::from(report(json, &CommandError::from(err))));
        }
    }

    match taskboard_app::ui::kiosk_start(taskboard_app::ui::display_env_available()) {
        taskboard_app::ui::KioskStart::Headless => {
            // The loud fallback (Q3): no display, no window — but the
            // socket is already serving, so keep serving the shared
            // daemon tail.
            tracing::error!("no display available; continuing as headless daemon");
            serve_and_shutdown(app, server, web.take()).await
        }
        taskboard_app::ui::KioskStart::Appliance => {
            // The hand-over, in `run_desktop`'s order: the escape hatch
            // and findings before anything slint-side, then mount with
            // the kiosk window profile set (chunk 2's consumer site).
            taskboard_app::ui::observe_scale_escape_hatch();
            let (mut ui_settings, keymap_findings) = taskboard_app::ui::resolve_ui_settings(
                &app.config.ui,
                taskboard_app::ui::resolve_day_start(&app.config),
            );
            ui_settings.kiosk = true;
            ui_settings.kiosk_rotation = kiosk_rotation_handover(&app.config);
            ui_settings.reminder_defaults = app.config.reminder_engine_config().defaults;
            ui_settings.connectors = taskboard_app::ui::resolve_connector_infos(&app.config)?;
            ui_settings.boards = taskboard_app::ui::resolve_board_infos(&app.engine);
            taskboard_app::ui::surface_keymap_findings(&keymap_findings);
            let (commands_tx, commands_rx) = taskboard_app::ui::ui_command_channel();
            let forwarder =
                taskboard_app::ui::spawn_ui_command_forwarder(commands_rx, app.engine.clone());
            let (requests_tx, requests_rx) = taskboard_app::ui_requests::ui_request_channel();
            let request_forwarder = taskboard_app::ui_requests::spawn_ui_request_forwarder(
                requests_rx,
                app.engine.shared_state(),
                taskboard_app::ui_requests::ConnectionWorkspace {
                    config_write_path,
                    store: taskboard_app::secrets::resolve_credential_store(
                        app.config.nextcloud.credential_store,
                        app.config.credential_base_dir(),
                    ),
                    control: app.connector_control(),
                    engine: app.engine.clone(),
                },
            );

            let handle = match taskboard_ui_slint::mount(
                app.engine.shared_state(),
                app.engine.subscribe(),
                app.system.subscribe(),
                commands_tx,
                requests_tx,
                ui_settings,
            ) {
                Ok(handle) => handle,
                // Mount failure (the display-var-outlives-its-server
                // class, typed `UiMountError`): the server is already
                // spawned, so drain through the shared tail instead of
                // dropping the socket sidecar and actors silently; the
                // mount error is the reported outcome.
                Err(err) => {
                    forwarder.abort();
                    request_forwarder.abort();
                    let _ = drain_and_shutdown(app, server, web.take()).await;
                    return Err(err.into());
                }
            };

            // The exit watcher (Q2): signal and `daemon stop` both quit
            // the event loop; window close is the third exit and needs
            // no arm — it ends `run` directly.
            let stop_rx = server.stopped_receiver();
            let watcher = tokio::spawn(async move {
                tokio::select! {
                    () = wait_for_signal() => {
                        tracing::info!("shutdown signal received");
                    }
                    () = taskboard_app::ipc::server::IpcServerHandle::stopped_from(stop_rx) => {
                        tracing::info!("shutdown requested over ipc");
                    }
                }
                // The retry matters here exactly as in `run_desktop`:
                // a stop racing the boot window (before `run` starts
                // the loop) is otherwise silently lost.
                taskboard_app::ui::quit_until_accepted().await;
            });

            let run_result = taskboard_ui_slint::run(handle);

            watcher.abort();
            forwarder.abort();
            request_forwarder.abort();
            // The shutdown-tail error (a still-shared core) outranks
            // the loop result, mirroring `run_desktop`'s order.
            drain_and_shutdown(app, server, web.take()).await?;
            run_result?;
            Ok(())
        }
    }
}
