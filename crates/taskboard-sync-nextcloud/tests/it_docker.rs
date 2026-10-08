// SPDX-License-Identifier: MIT OR Apache-2.0
//! Tier 2 integration tests: real (dockerized) Nextcloud + real Deck app
//! (`docs/testing_strategy.org` §8). Secretless by construction — the
//! setup script mints the app password locally.
//!
//! The behavioral bodies live once in [`common::suite`]; this file only
//! resolves the tier configuration, skips when unconfigured, and provides
//! the `it_nextcloud_docker_*` names the CI job filters on.
//!
//! Run locally:
//!
//! ```sh
//! eval "$(scripts/nextcloud_it_setup.sh up)" &&
//!   cargo nextest run -p taskboard-sync-nextcloud --run-ignored only \
//!     -E 'test(it_nextcloud_docker)'
//! ```

mod common;

use common::suite::{board_lifecycle, delete_missing_board_is_typed_error, lists_boards};
use common::{deck_client, live_docker_config, with_deadline};

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_lists_boards() {
    let Some(cfg) = live_docker_config() else {
        return; // skip, never fail: tier not configured
    };
    with_deadline(lists_boards(&deck_client(&cfg)))
        .await
        .expect("test must finish within the live deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_board_lifecycle() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(board_lifecycle(&deck_client(&cfg)))
        .await
        .expect("test must finish within the live deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_delete_missing_board_is_not_found() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(delete_missing_board_is_typed_error(&deck_client(&cfg)))
        .await
        .expect("test must finish within the live deadline");
}
