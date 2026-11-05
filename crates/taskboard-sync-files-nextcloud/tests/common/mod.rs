// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared harness for the files transport's docker-tier pins: the
//! Deck connector's harness conventions (config resolution with the
//! skip-never-fail posture, per-run isolation, wall-clock deadlines,
//! known-server-error retries), with a [`files_transport`] factory.

use std::time::Duration;

use taskboard_sync_files_nextcloud::client::WebDavFilesTransport;

pub(crate) struct LiveCfg {
    pub url: String,
    pub user: String,
    pub token: String,
}

fn read_cfg(prefix: &str) -> Option<LiveCfg> {
    let (url, user, token) = (
        std::env::var(format!("{prefix}_URL")).ok()?,
        std::env::var(format!("{prefix}_USER")).ok()?,
        std::env::var(format!("{prefix}_TOKEN")).ok()?,
    );
    if url.is_empty() || user.is_empty() || token.is_empty() {
        return None;
    }
    Some(LiveCfg { url, user, token })
}

/// Reads the hosted-tier credentials (the `TASKBOARD_IT_NEXTCLOUD`
/// prefix, the repo-root `.env` convention — `dotenvy::dotenv()`
/// first, exactly the Deck crate's live helper); `None` when unset —
/// skip, never fail.
#[allow(dead_code)] // each tests/* binary compiles this module separately
pub(crate) fn live_config() -> Option<LiveCfg> {
    let _ = dotenvy::dotenv();
    read_cfg("TASKBOARD_IT_NEXTCLOUD")
}

/// Reads the dockerized-tier credentials (the `TASKBOARD_IT_DOCKER`
/// prefix, exported by `scripts/nextcloud_it_setup.sh up`); `None`
/// when unset — skip, never fail.
#[allow(dead_code)] // each tests/* binary compiles this module separately
pub(crate) fn live_docker_config() -> Option<LiveCfg> {
    read_cfg("TASKBOARD_IT_DOCKER")
}

/// Unique per-run directory name so parallel/never-deleting runs
/// cannot collide on the server (readers never delete: every run
/// leaves its own `devices/<run-id>` subtree behind).
pub(crate) fn run_id() -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("0193c000-0000-7c89-ab01-{}", &id[..12])
}

/// Wall-clock cap for every live/docker test body.
pub(crate) const LIVE_DEADLINE: Duration = Duration::from_secs(30);

/// Runs `body` under a hard deadline so a hung server fails the test
/// instead of wedging CI.
pub(crate) async fn with_deadline<T>(
    body: impl std::future::Future<Output = T>,
) -> Result<T, tokio::time::error::Elapsed> {
    tokio::time::timeout(LIVE_DEADLINE, body).await
}

/// Retries `op` while the server answers one of its sporadic
/// `Server(500)`s (the Deck tier's known-flake posture; the transport
/// deliberately does not retry `Server` — the tier scaffolding does).
pub(crate) async fn retrying_server_errors<T>(
    mut op: impl AsyncFnMut() -> Result<T, taskboard_sync_files_nextcloud::error::TransportError>,
) -> Result<T, taskboard_sync_files_nextcloud::error::TransportError> {
    let mut attempts = 0u32;
    loop {
        match op().await {
            Err(taskboard_sync_files_nextcloud::error::TransportError::Server(_))
                if attempts < 4 =>
            {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(200 * u64::from(attempts))).await;
            }
            other => return other,
        }
    }
}

/// Builds the files transport for a resolved tier configuration.
pub(crate) fn files_transport(cfg: &LiveCfg) -> WebDavFilesTransport {
    WebDavFilesTransport::new(&cfg.url, &cfg.user, &cfg.token).expect("tier URL must be valid")
}
