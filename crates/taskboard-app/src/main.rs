// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard` application runner.
//!
//! Thin process glue (ADRs 0008/0009): parse the CLI, initialize
//! `tracing`, then either hand the subcommand to the library's
//! `commands::dispatch` (typed handlers, exit codes) or run the daemon
//! lifecycle — boot the in-process core through `bootstrap`, wait for
//! the signal, graceful drain with a second-signal force exit. All
//! wiring lives in the `taskboard_app` library so tests and future
//! frontends reuse it.

#![forbid(unsafe_code)]

use std::sync::Arc;

use clap::Parser as _;

use taskboard_app::bootstrap::{bootstrap, wait_for_signal};
use taskboard_app::cli::{Cli, Command};
use taskboard_app::commands::{CommandCtx, dispatch};
use taskboard_app::config::{AppConfig, CredentialStoreKind};
use taskboard_app::secrets::{CredentialStore, EnvCredentialStore, KeyringStore};

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
    let cli = Cli::parse();
    tracing::info!(version = taskboard_app::VERSION, "starting taskboard");

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run(cli))?;

    tracing::info!("taskboard shut down cleanly");
    Ok(())
}

async fn run(cli: Cli) -> color_eyre::Result<()> {
    // Subcommand arms are lib code (`commands::dispatch`); the process
    // exits with the returned code (0 success / 1 runtime failure).
    // Exit 2 is clap's usage error, 130 the second-signal force exit.
    if let Some(command) = cli.command
        && !matches!(command, Command::Daemon)
    {
        let ctx = CommandCtx {
            json: cli.json,
            config_path: AppConfig::config_path(cli.config),
        };
        let code = dispatch(command, ctx).await;
        std::process::exit(i32::from(code));
    }

    // Daemon lifecycle (explicit `daemon`, or no subcommand: `[app]
    // mode` resolution — the daemon until the UI phases land).
    let config = AppConfig::load(cli.config)?;
    let store: Arc<dyn CredentialStore> = match config.nextcloud.credential_store {
        CredentialStoreKind::Keyring => Arc::new(KeyringStore),
        CredentialStoreKind::Env => Arc::new(EnvCredentialStore),
    };

    // `Desktop`/`Kiosk` decode but have no frontend yet (Phase 7): the
    // typed error is the placeholder arm that gets replaced there.
    let app = match bootstrap(config, store).await {
        Ok(app) => app,
        Err(taskboard_app::BootstrapError::ModeNotImplemented(mode)) => {
            tracing::error!(?mode, "this mode is not implemented yet");
            color_eyre::eyre::bail!("app mode {mode:?} is not implemented yet (Phase 7)");
        }
        Err(err) => return Err(err.into()),
    };

    // The first-signal wait happens here — *before* arming the
    // force-exit watcher — because `ctrl_c` and SIGTERM notify every
    // listener: an early watcher would catch the first signal instead
    // of the second.
    wait_for_signal().await;
    tracing::info!("shutdown signal received");
    let outcome = tokio::select! {
        outcome = app.shutdown() => outcome,
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
