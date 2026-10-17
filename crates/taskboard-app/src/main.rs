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
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // Sensible default: info from our crates, warnings from deps.
                "info,tower=warn,hyper=warn,reqwest=warn".to_owned().into()
            }))
            // Logs go to stderr: stdout carries the `--json` payloads
            // (and human output) — a scripting surface, not a log sink.
            .with_writer(std::io::stderr)
            .init();
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

    // Bare `taskboard` routing (D4): `mode = "desktop"` boots the GUI
    // *regardless of TTY* — `AppMode` is frontend selection (ADR 0008),
    // and launching from a terminal is how desktop apps start. The
    // REPL stays the TTY front-end of `daemon` mode (ADR 0011); a
    // non-TTY daemon (and the refused `kiosk`) keeps the daemon
    // lifecycle. An explicit `daemon` subcommand is `Some` above and
    // skips this routing entirely.
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
        ) {
            taskboard_app::ui::BareRoute::DesktopUi => {
                return taskboard_app::ui::run_desktop(config).await;
            }
            taskboard_app::ui::BareRoute::DaemonLifecycle => {
                // `kiosk` is refused by the daemon lifecycle's own mode
                // gate (no kiosk frontend until wave W1+).
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
    // The daemon's entry owns the `[app] mode` gate (the core itself is
    // mode-agnostic — the REPL serves any mode's console, ADR 0011):
    // `Desktop`/`Kiosk` have no daemon-side frontend until Phase 7.
    if let mode
    @ (taskboard_app::config::AppMode::Desktop | taskboard_app::config::AppMode::Kiosk) =
        config.app.mode
    {
        tracing::error!(?mode, "this mode is not implemented yet");
        color_eyre::eyre::bail!("app mode {mode:?} is not implemented yet (Phase 7)");
    }
    // Backend-agnostic (ADR 0010 decision 9, amended 2026-10-11): with
    // IPC the daemon's purpose is "own the core, serve commands" — a
    // `standalone` daemon gives scripters the same no-boot-per-command
    // latency the connector profile gets, and idling is over.
    let store = taskboard_app::secrets::resolve_credential_store(config.nextcloud.credential_store);

    let app = match bootstrap(
        config,
        store.clone(),
        Arc::new(taskboard_app::notifications::LogSink),
    )
    .await
    {
        Ok(app) => Arc::new(app),
        // Boot failures are reported typed (the `--json` envelope on
        // stderr), exactly like every other command — `instance-locked`
        // included: the daemon is a core boot, not a special failure.
        Err(err) => std::process::exit(i32::from(report(json, &CommandError::from(err)))),
    };

    // The daemon logs system events (StorageFailed et al.) until exit.
    let _logger = taskboard_app::bootstrap::spawn_system_event_logger(&app.system);

    // One core, N frontends (ADR 0008): while the daemon runs, the
    // socket serves every core verb — CLI invocations stop paying the
    // per-command boot cost and stop fighting the instance lock.
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
        "daemon serving commands over ipc"
    );

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
    let shutdown_timeout = Duration::from_secs(app.config.app.shutdown_timeout_secs);
    let drain = async {
        // Bounded: past the budget the server aborts its tasks (a
        // wedged peer must never hold the daemon's shutdown hostage)
        // and the refcount is released either way.
        server.join(shutdown_timeout).await;
        taskboard_app::bootstrap::shutdown_from_arc(app).await
    };
    let outcome = tokio::select! {
        outcome = drain => outcome,
        () = wait_for_signal() => {
            tracing::warn!("second signal during shutdown; forcing exit");
            std::process::exit(130);
        }
    };
    if outcome.actor_aborted {
        tracing::warn!("sync actor did not stop in time and was aborted");
    }
    Ok(())
}
