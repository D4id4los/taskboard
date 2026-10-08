// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tier 3 integration tests against the private production server
//! (`docs/testing_strategy.org` §8). Local, manual only: credentials come
//! from a repo-root `.env` (see `.env.example`); app password, never the
//! account password.
//!
//! The behavioral bodies shared with Tier 2 live once in [`common::suite`];
//! this file only resolves the live configuration, skips when unconfigured,
//! and adds the tier-specific production-auth check.
//!
//! Run from the repo root:
//!
//! ```sh
//! cargo nextest run -p taskboard-sync-nextcloud --run-ignored only \
//!   -E 'test(it_nextcloud_live)'
//! ```

mod common;

use common::suite::{board_lifecycle, full_tree_lifecycle, lists_boards};
use common::{deck_client, live_config, with_deadline};
use taskboard_sync_nextcloud::DeckError;

#[tokio::test]
#[ignore = "requires live-server credentials in .env (see .env.example)"]
async fn it_nextcloud_live_lists_boards() {
    let Some(cfg) = live_config() else {
        return; // skip, never fail: tier not configured
    };
    with_deadline(lists_boards(&deck_client(&cfg)))
        .await
        .expect("test must finish within the live deadline");
}

#[tokio::test]
#[ignore = "requires live-server credentials in .env (see .env.example)"]
async fn it_nextcloud_live_board_lifecycle() {
    let Some(cfg) = live_config() else {
        return;
    };
    with_deadline(board_lifecycle(&deck_client(&cfg)))
        .await
        .expect("test must finish within the live deadline");
}

#[tokio::test]
#[ignore = "requires live-server credentials in .env (see .env.example)"]
async fn it_nextcloud_live_bad_token_is_unauthorized() {
    let Some(mut cfg) = live_config() else {
        return;
    };
    // Tier 3 only: proves the production auth path rejects a bad app
    // password with a typed error (never a panic), after retries.
    cfg.token = "not-a-valid-app-password".to_owned();
    with_deadline(async {
        let err = deck_client(&cfg).boards().await.unwrap_err();
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

#[tokio::test]
#[ignore = "requires live-server credentials in .env (see .env.example)"]
async fn it_nextcloud_live_full_tree_lifecycle() {
    let Some(cfg) = live_config() else {
        return;
    };
    with_deadline(full_tree_lifecycle(&deck_client(&cfg)))
        .await
        .expect("test must finish within the live deadline");
}
