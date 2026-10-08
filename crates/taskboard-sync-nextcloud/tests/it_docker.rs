// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tier 2 integration tests: real (dockerized) Nextcloud + real Deck app
//! (`docs/testing_strategy.org` §8). Secretless by construction — the
//! setup script mints the app password locally.
//!
//! Run locally:
//!
//! ```sh
//! eval "$(scripts/nextcloud_it_setup.sh up)" &&
//!   cargo nextest run -p taskboard-sync-nextcloud --run-ignored only \
//!     -E 'test(it_nextcloud_docker)'
//! ```

mod common;

use common::{LiveCfg, live_docker_config, run_id, with_deadline};
use taskboard_sync_nextcloud::{DeckClient, DeckError};

fn client(cfg: &LiveCfg) -> DeckClient {
    DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("docker URL must be valid")
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_lists_boards() {
    let Some(cfg) = live_docker_config() else {
        return; // skip, never fail: tier not configured
    };
    with_deadline(async {
        let boards = client(&cfg)
            .boards()
            .await
            .expect("boards listing must succeed");
        assert!(
            boards.iter().all(|b| !b.title.is_empty()),
            "decoded boards must be well-formed"
        );
    })
    .await
    .expect("test must finish within the live deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_board_lifecycle() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(async {
        let client = client(&cfg);
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
    })
    .await
    .expect("test must finish within the live deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_delete_missing_board_is_not_found() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(async {
        // A plausible-but-absent id (u64::MAX overflows Deck's int columns
        // and yields a 500). Nextcloud 35 answers Forbidden for boards that
        // do not exist or are not ours; either way it must be a typed error.
        let err = client(&cfg).delete_board(424_242).await.unwrap_err();
        assert!(
            matches!(err, DeckError::NotFound | DeckError::Forbidden),
            "unexpected error variant: {err:?}"
        );
    })
    .await
    .expect("test must finish within the live deadline");
}
