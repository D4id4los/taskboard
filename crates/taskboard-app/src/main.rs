// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard` application runner.
//!
//! Thin process glue (ADR 0008): parse the CLI, initialize `tracing`,
//! load the config, pick the credential store, boot the in-process core
//! through the library's `bootstrap`, and run the daemon lifecycle with
//! a second-signal force exit. All wiring lives in the `taskboard_app`
//! library so tests and future frontends reuse it.

#![forbid(unsafe_code)]

use std::sync::Arc;

use clap::Parser as _;

use taskboard_app::bootstrap::{bootstrap, wait_for_signal};
use taskboard_app::cli::{Cli, Command};
use taskboard_app::config::{AppConfig, CredentialStoreKind};
use taskboard_app::secrets::{CredentialStore, EnvCredentialStore, KeyringStore};

fn init_tracing() {
    #[cfg(feature = "tokio-console")]
    console_subscriber::init();

    #[cfg(not(feature = "tokio-console"))]
    {
        use tracing_subscriber::EnvFilter;
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // Sensible default: info from our crates, warnings from deps.
                "info,tower=warn,hyper=warn,reqwest=warn".to_owned().into()
            }))
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

    // Dispatch shape: `None` resolves `[app] mode` (already enforced by
    // the bootstrap mode check); `daemon` is explicit. Phase 6 grows
    // this match with `login`/`boards`/`tasks`/`sync`.
    match cli.command {
        Some(Command::Daemon) | None => {}
    }

    // Daemon lifecycle: the first signal starts the graceful drain; a
    // second signal during the drain force-exits (standard daemon UX;
    // deliberately untested glue). The first-signal wait happens here —
    // *before* arming the force-exit watcher — because `ctrl_c` and
    // SIGTERM notify every listener: an early watcher would catch the
    // first signal instead of the second.
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
