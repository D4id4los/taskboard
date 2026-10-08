// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared harness for the Nextcloud integration tiers (strategy §8).
//!
//! Tier 2 (dockerized) and Tier 3 (live server) tests are `#[ignore]`-marked
//! and skip themselves when their environment is not configured, so the
//! default suite stays hermetic.
// Not every tier's test binary uses every helper.
#![allow(dead_code)]

use std::time::Duration;

use taskboard_sync_nextcloud::DeckClient;

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

/// Builds the client for a resolved tier configuration.
pub(crate) fn deck_client(cfg: &LiveCfg) -> DeckClient {
    DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("tier URL must be valid")
}

/// The live-suite behaviors shared by Tier 2 (dockerized) and Tier 3 (live
/// server): both tiers re-run the *same* operations against increasingly
/// real servers, so the bodies live here exactly once and each tier file
/// keeps only config resolution, skip logic, and its `it_nextcloud_*` test
/// names. Client *behavior* (envelope, headers, errors, retries) is asserted
/// at Tiers 0/1 and deliberately not duplicated here.
pub(crate) mod suite {
    use super::run_id;
    use taskboard_sync_nextcloud::{DeckClient, DeckError};

    /// Listing succeeds and decodes well-formed boards.
    pub(crate) async fn lists_boards(client: &DeckClient) {
        let boards = client.boards().await.expect("boards listing must succeed");
        assert!(
            boards.iter().all(|b| !b.title.is_empty()),
            "decoded boards must be well-formed"
        );
    }

    /// A run-id board is created, appears in the listing, and its soft
    /// delete is stamped with a non-zero `deletedAt` in the DELETE response.
    pub(crate) async fn board_lifecycle(client: &DeckClient) {
        let title = run_id();

        let created = client
            .create_board(&title, "00c2e0")
            .await
            .expect("board creation must succeed");

        let listed = client.boards().await.expect("boards listing must succeed");
        assert!(
            listed
                .iter()
                .any(|b| b.id == created.id && b.title == title),
            "created board must appear in the listing"
        );

        // Deck's DELETE is a soft delete, and the DELETE response is the
        // authoritative, immediately-consistent record of it (the boards
        // listing can lag behind Deck's board cache).
        let deleted = client
            .delete_board(created.id)
            .await
            .expect("board deletion must succeed");
        assert!(
            !deleted.is_live(),
            "DELETE response must stamp deletedAt (got {deleted:?})"
        );
    }

    /// Deleting a missing board yields a typed error. Nextcloud 35 answers
    /// Forbidden (not `NotFound`) for boards that do not exist or are not
    /// ours; a plausible-but-absent id is used because huge ids overflow
    /// Deck's int columns and yield a 500.
    pub(crate) async fn delete_missing_board_is_typed_error(client: &DeckClient) {
        let err = client.delete_board(424_242).await.unwrap_err();
        assert!(
            matches!(err, DeckError::NotFound | DeckError::Forbidden),
            "unexpected error variant: {err:?}"
        );
    }
}
