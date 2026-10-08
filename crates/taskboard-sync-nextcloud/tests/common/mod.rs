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
    use taskboard_sync_nextcloud::{DeckClient, DeckColor, DeckError};

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
            .create_board(
                &title,
                &DeckColor::from_hex("00c2e0").expect("valid fixture color"),
            )
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

    /// Full write-surface lifecycle on a run-id board: label, two stacks,
    /// two cards, duedate + label assignment, archive, cross-stack move,
    /// card round-trip update, then a read-back of the whole tree. Soft
    /// delete is asserted only on the authoritative DELETE payload (the
    /// boards-listing lag precedent); the caller tears the board down.
    pub(crate) async fn full_tree_lifecycle(client: &DeckClient) {
        let run = run_id();
        let color =
            taskboard_sync_nextcloud::DeckColor::from_hex("00c2e0").expect("valid lifecycle color");

        let board = client
            .create_board(&run, &color)
            .await
            .expect("board creation must succeed");

        let result = tree_body(client, board.id, &run, &color).await;

        // Best-effort teardown regardless of body outcome; a failure here
        // must not mask the body's error.
        let deleted = client.delete_board(board.id).await;
        match (result, deleted) {
            (Ok(()), Ok(d)) => assert!(!d.is_live(), "DELETE payload must stamp deletedAt"),
            (Err(body_err), _) => panic!("full-tree lifecycle failed: {body_err:?}"),
            (Ok(()), Err(teardown_err)) => panic!("board teardown failed: {teardown_err:?}"),
        }
    }

    async fn tree_body(
        client: &DeckClient,
        board_id: u64,
        run: &str,
        color: &taskboard_sync_nextcloud::DeckColor,
    ) -> Result<(), taskboard_sync_nextcloud::DeckError> {
        let label = client
            .create_label(board_id, &format!("{run}-lbl"), color)
            .await?;

        let stack_a = client
            .create_stack(board_id, &format!("{run}-a"), 0)
            .await?;
        let stack_b = client
            .create_stack(board_id, &format!("{run}-b"), 1)
            .await?;

        let duedate = chrono::Utc::now() + chrono::Duration::days(1);
        let card_a = client
            .create_card(
                board_id,
                stack_a.id,
                &taskboard_sync_nextcloud::NewCard {
                    title: format!("{run}-card"),
                    order: Some(0),
                    description: None,
                    duedate: Some(duedate),
                    kind: None,
                },
            )
            .await?;
        let card_b = client
            .create_card(
                board_id,
                stack_a.id,
                &taskboard_sync_nextcloud::NewCard {
                    title: format!("{run}-archived"),
                    order: Some(1),
                    description: None,
                    duedate: None,
                    kind: None,
                },
            )
            .await?;

        client
            .assign_label(board_id, stack_a.id, card_a.id, label.id)
            .await?;
        client.archive_card(board_id, stack_a.id, card_b.id).await?;

        // Cross-stack move (the reorder primitive): card_a to stack_b.
        client
            .reorder_card(board_id, stack_a.id, card_a.id, 0, stack_b.id)
            .await?;

        // Full round-trip update on the moved card.
        let mut fetched = client.card(board_id, stack_b.id, card_a.id).await?;
        fetched.description = format!("{run}-description");
        client.update_card(board_id, &fetched).await?;

        // Read the tree back and assert terminal state.
        let stacks = client
            .stacks(board_id, taskboard_sync_nextcloud::StackFilter::Active)
            .await?;
        let stack_b_after = stacks
            .iter()
            .find(|s| s.id == stack_b.id)
            .expect("stack b listed");
        let moved = stack_b_after
            .cards
            .iter()
            .find(|c| c.id == card_a.id)
            .expect("moved card must be listed in the destination stack");
        assert_eq!(moved.description, format!("{run}-description"));
        assert!(
            moved.duedate.is_some(),
            "duedate must survive the round-trip"
        );
        assert!(
            moved.labels.contains(&label.id),
            "label must survive the round-trip"
        );
        let origin = stacks
            .iter()
            .find(|s| s.id == stack_a.id)
            .expect("stack a listed");
        assert!(
            origin.cards.iter().all(|c| c.id != card_a.id),
            "moved card must no longer be listed in the origin stack"
        );

        let labels = client.labels(board_id).await?;
        assert!(
            labels.iter().any(|l| l.id == label.id),
            "created label must be listed"
        );
        Ok(())
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
