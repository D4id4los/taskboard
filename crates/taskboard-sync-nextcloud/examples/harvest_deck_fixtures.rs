// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fixture-harvest helper for the Tier 0 recorded fixtures
//! (`docs/testing_strategy.org` §8).
//!
//! Creates a run-id board with known content, lists boards against the live
//! server, writes the raw JSON to `tests/fixtures/deck/boards_live.json`,
//! then soft-deletes the board. Manual refresh workflow: run it, review the
//! harvested file (it must contain only run-id board data), then commit.
//!
//! Credentials come from the repo-root `.env` (see `.env.example`). Run from
//! the repo root:
//!
//! ```sh
//! cargo run -p taskboard-sync-nextcloud --example harvest_deck_fixtures
//! ```

use std::path::PathBuf;

use taskboard_sync_nextcloud::{DeckClient, DeckError};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), DeckError> {
    dotenvy::dotenv().ok();
    let (url, user, token) = (
        std::env::var("TASKBOARD_IT_NEXTCLOUD_URL").expect("TASKBOARD_IT_NEXTCLOUD_URL unset"),
        std::env::var("TASKBOARD_IT_NEXTCLOUD_USER").expect("TASKBOARD_IT_NEXTCLOUD_USER unset"),
        std::env::var("TASKBOARD_IT_NEXTCLOUD_TOKEN").expect("TASKBOARD_IT_NEXTCLOUD_TOKEN unset"),
    );
    let client = DeckClient::new(&url, &user, &token).expect("valid base URL");

    let run_id = {
        let id = uuid::Uuid::new_v4().simple().to_string();
        format!("taskboard-it-{}", &id[..8])
    };
    let board = client.create_board(&run_id, "00c2e0").await?;
    println!(
        "created harvest board {id} ({title})",
        id = board.id,
        title = board.title
    );

    let boards = client.boards().await?;
    let json = serde_json::to_string_pretty(&boards).expect("boards serialize");
    let out =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/deck/boards_live.json");
    std::fs::write(&out, json).expect("write fixture");
    println!("wrote {}", out.display());

    let deleted = client.delete_board(board.id).await?;
    println!(
        "soft-deleted harvest board {} (deletedAt={})",
        deleted.id, deleted.deleted_at
    );
    Ok(())
}
