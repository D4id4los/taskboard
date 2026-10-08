// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fixture-harvest helper for the Tier 0 recorded fixtures
//! (`docs/testing_strategy.org` §8).
//!
//! Builds a run-id tree (board, label, two stacks, two cards with duedate
//! and an assigned label, one archived card) against the live server, then
//! writes the fetched resources to `tests/fixtures/deck/*_live.json`:
//! `boards_live.json`, `stacks_live.json`, `labels_live.json`, and
//! `card_live.json`. Attachment fixtures are not harvestable here (uploads
//! are out of scope); `boards_live.json` keeps the review-only-run-id rule.
//! Soft-deletes the board at the end.
//!
//! Manual refresh workflow: run it, review the harvested files (they must
//! contain only run-id data), then commit.
//!
//! Credentials come from the repo-root `.env` (see `.env.example`). Run from
//! the repo root:
//!
//! ```sh
//! cargo run -p taskboard-sync-nextcloud --example harvest_deck_fixtures
//! ```

use std::path::PathBuf;

use chrono::{Duration, Utc};
use taskboard_sync_nextcloud::{DeckClient, DeckColor, DeckError, NewCard};

const FIXTURE_DIR: &str = "tests/fixtures/deck";

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
    let color = DeckColor::from_hex("00c2e0").expect("valid harvest color");
    let board = client.create_board(&run_id, &color).await?;
    println!(
        "created harvest board {id} ({title})",
        id = board.id,
        title = board.title
    );

    match harvest_tree(&client, board.id, &run_id, &color).await {
        Ok(()) => println!(
            "harvested stacks/labels/card fixtures for board {}",
            board.id
        ),
        // Best-effort teardown must not mask the harvest error.
        Err(err) => {
            let _ = client.delete_board(board.id).await;
            return Err(err);
        }
    }

    let deleted = client.delete_board(board.id).await?;
    println!(
        "soft-deleted harvest board {} (deletedAt={})",
        deleted.id, deleted.deleted_at
    );
    Ok(())
}

async fn harvest_tree(
    client: &DeckClient,
    board_id: u64,
    run_id: &str,
    color: &DeckColor,
) -> Result<(), DeckError> {
    let label = client
        .create_label(board_id, &format!("{run_id}-lbl"), color)
        .await?;
    let stack = client
        .create_stack(board_id, &format!("{run_id}-stack"), 0)
        .await?;
    let card = client
        .create_card(
            board_id,
            stack.id,
            &NewCard {
                title: format!("{run_id}-card"),
                order: Some(0),
                description: Some("harvested card".into()),
                duedate: Some(Utc::now() + Duration::days(1)),
                kind: None,
            },
        )
        .await?;
    client
        .assign_label(board_id, stack.id, card.id, label.id)
        .await?;

    write_fixture(
        "boards_live.json",
        &client
            .boards()
            .await?
            .iter()
            .filter(|b| b.title == run_id)
            .cloned()
            .collect::<Vec<_>>(),
    )
    .await;
    write_fixture(
        "stacks_live.json",
        &client
            .stacks(board_id, taskboard_sync_nextcloud::StackFilter::Active)
            .await?,
    )
    .await;
    write_fixture("labels_live.json", &client.labels(board_id).await?).await;
    write_fixture(
        "card_live.json",
        &client.card(board_id, stack.id, card.id).await?,
    )
    .await;
    Ok(())
}

async fn write_fixture<T: serde::Serialize>(name: &str, value: &T) {
    let json = serde_json::to_string_pretty(value).expect("fixture serializes");
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE_DIR)
        .join(name);
    // Async context: use tokio's non-blocking file API (rust:S7493).
    tokio::fs::write(&out, json).await.expect("write fixture");
    println!("wrote {}", out.display());
}
