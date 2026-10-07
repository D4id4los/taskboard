// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard` application runner.
//!
//! Entry point of the kiosk/desktop application. Responsibilities:
//! 1. Load hierarchical configuration (figment: defaults → `taskboard.toml`
//!    → `TASKBOARD_` environment overrides).
//! 2. Initialize `tracing` (with `log` compatibility for dependencies) and,
//!    when built with `--features tokio-console`, the console subscriber.
//! 3. Wire the actor channels (`mpsc` commands, `broadcast` signals,
//!    `ArcSwap<AppState>`).
//! 4. Spawn the background actors (state engine, sync, storage) and mount
//!    the UI, then step aside.
//!
//! Actor wiring is currently a scaffold: each subsystem lands incrementally
//! behind its own feature work. The channel topology contract lives in
//! `docs/architecture.org`.

#![forbid(unsafe_code)]

use taskboard_domain::AppState;

/// Application version, taken from the shared workspace version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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
    tracing::info!(version = VERSION, "starting taskboard");

    // Placeholder state holder: the state engine will own the write side of
    // this ArcSwap; the UI will load from it lock-free.
    let _state = arc_swap::ArcSwap::from_pointee(AppState::default());

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async_main())?;

    tracing::info!("taskboard shut down cleanly");
    Ok(())
}

// Kept async in anticipation of the actor wiring; the first await points
// (state engine, sync, storage actors) arrive with their own feature work.
#[allow(clippy::unused_async)]
async fn async_main() -> color_eyre::Result<()> {
    // TODO(feature work): wire figment config, actor channels, spawn the
    // state engine / sync / storage actors, mount the UI.
    Ok(())
}
