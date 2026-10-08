// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tier 3 integration tests against the private production server
//! (`docs/testing_strategy.org` §8). Local, manual only: credentials come
//! from a repo-root `.env` (see `.env.example`); app password, never the
//! account password.
//!
//! Run from the repo root:
//!
//! ```sh
//! cargo nextest run -p taskboard-sync-nextcloud --run-ignored only \
//!   -E 'test(it_nextcloud_live)'
//! ```

mod common;

use common::{LiveCfg, live_config, run_id, with_deadline};
use taskboard_sync_nextcloud::{DeckClient, DeckError};

fn client(cfg: &LiveCfg) -> DeckClient {
    DeckClient::new(&cfg.url, &cfg.user, &cfg.token).expect("live URL must be valid")
}

#[tokio::test]
#[ignore = "requires live-server credentials in .env (see .env.example)"]
async fn it_nextcloud_live_lists_boards() {
    let Some(cfg) = live_config() else {
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
#[ignore = "requires live-server credentials in .env (see .env.example)"]
async fn it_nextcloud_live_board_lifecycle() {
    let Some(cfg) = live_config() else {
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

        // Soft delete only; the run-id board is the sole touched resource.
        let deleted = client
            .delete_board(created.id)
            .await
            .expect("board deletion must succeed");
        assert!(!deleted.is_live(), "delete must stamp deletedAt");
    })
    .await
    .expect("test must finish within the live deadline");
}

#[tokio::test]
#[ignore = "requires live-server credentials in .env (see .env.example)"]
async fn it_nextcloud_live_bad_token_is_unauthorized() {
    let Some(mut cfg) = live_config() else {
        return;
    };
    cfg.token = "not-a-valid-app-password".to_owned();
    with_deadline(async {
        let err = client(&cfg).boards().await.unwrap_err();
        // Servers answer 401 for a bad app password; after retries it must
        // still surface as a typed error, never a panic.
        assert!(
            matches!(err, DeckError::Unauthorized | DeckError::Forbidden),
            "unexpected error variant: {err:?}"
        );
    })
    .await
    .expect("test must finish within the live deadline");
}
