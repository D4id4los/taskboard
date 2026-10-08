// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared harness for the Nextcloud integration tiers (strategy §8).
//!
//! Tier 2 (dockerized) and Tier 3 (live server) tests are `#[ignore]`-marked
//! and skip themselves when their environment is not configured, so the
//! default suite stays hermetic.
// Not every tier's test binary uses every helper.
#![allow(dead_code)]

use std::time::Duration;

/// Reads Tier 2 docker-tier credentials; `None` when any is unset.
pub(crate) fn live_docker_config() -> Option<LiveCfg> {
    read_cfg("TASKBOARD_IT_DOCKER")
}

/// Reads Tier 3 live-server credentials from the environment (after loading
/// the repo-root `.env` when present); `None` when any is unset.
pub(crate) fn live_config() -> Option<LiveCfg> {
    let _ = dotenvy::dotenv();
    read_cfg("TASKBOARD_IT_NEXTCLOUD")
}

#[derive(Debug, Clone)]
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

/// Unique per-run prefix for test resources so parallel/never-deleting runs
/// cannot collide on the server.
pub(crate) fn run_id() -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("taskboard-it-{}", &id[..8])
}

/// Wall-clock cap for every live/docker test body.
pub(crate) const LIVE_DEADLINE: Duration = Duration::from_secs(30);

/// Runs `body` under a hard deadline so a hung server fails the test instead
/// of wedging CI.
pub(crate) async fn with_deadline<T>(
    body: impl std::future::Future<Output = T>,
) -> Result<T, tokio::time::error::Elapsed> {
    tokio::time::timeout(LIVE_DEADLINE, body).await
}
