// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared harness for the Nextcloud integration tiers (strategy §8).
//!
//! Tier 2 (dockerized) and Tier 3 (live server) tests are `#[ignore]`-marked
//! and skip themselves when their environment is not configured, so the
//! default suite stays hermetic.
//!
//! When a live tier fails on wire-format drift, consult the known-server
//! matrix in `docs/testing_strategy.org` §8 first (Tier 3 live as of
//! 2026-10-08: Nextcloud 33.0.9 / Deck 1.17.5; Tier 2 docker: Nextcloud
//! 35.0.1 / Deck 1.19.0) — it lists each observed deviation from the Deck
//! API reference.
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

/// Retries `op` while Deck answers one of its sporadic write-path
/// `Server(500)`s (known upstream flakiness, observed across create/reorder/
/// archive/delete; see the known-server matrix in `docs/testing_strategy.org`).
/// The production client deliberately does *not* retry `Server` — a retried
/// create can double-apply — but tier scaffolding operates on per-run
/// resources where a retry is safe, and reads are always safe to retry.
pub(crate) async fn retrying_server_errors<T>(
    mut op: impl AsyncFnMut() -> Result<T, taskboard_sync_nextcloud::DeckError>,
) -> Result<T, taskboard_sync_nextcloud::DeckError> {
    let mut attempts = 0u32;
    loop {
        match op().await {
            Err(taskboard_sync_nextcloud::DeckError::Server(_)) if attempts < 4 => {
                attempts += 1;
                tokio::time::sleep(Duration::from_millis(200 * u64::from(attempts))).await;
            }
            other => return other,
        }
    }
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
pub(crate) mod sync_pair;

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

        // Full round-trip update: fetch by id (the wire card's own stackId
        // is authoritative — the move itself can lag Deck's cache).
        let mut fetched = client.card(board_id, stack_a.id, card_a.id).await?;
        fetched.description = format!("{run}-description");
        client.update_card(board_id, &fetched).await?;

        // Read the tree back and assert terminal state: the card must be
        // listed under one of the board's stacks with all mutations.
        let stacks = client
            .stacks(board_id, taskboard_sync_nextcloud::StackFilter::Active)
            .await?;
        let moved = stacks
            .iter()
            .flat_map(|s| &s.cards)
            .find(|c| c.id == card_a.id)
            .expect("card must be listed under one of the board's stacks");
        assert_eq!(moved.description, format!("{run}-description"));
        assert!(
            moved.duedate.is_some(),
            "duedate must survive the round-trip"
        );
        assert!(
            moved.labels.iter().any(|l| l.id == label.id),
            "label must survive the round-trip"
        );

        // `GET /boards/{id}/labels` is not available on all Deck servers
        // (405 on Nextcloud 35); the board read carries the label list.
        let board_after = client.board(board_id).await?;
        assert!(
            board_after.labels.iter().any(|l| l.id == label.id),
            "created label must be listed on the board"
        );
        Ok(())
    }

    /// Real conditional-read cycle: first poll (validators), conditional
    /// poll (either `304` or fresh data — servers that do not emit
    /// validators on these routes must not fail the test), then a mutation,
    /// then a conditional poll with the *stale* validators which must
    /// return fresh data containing the mutation.
    pub(crate) async fn conditional_read_cycle(client: &DeckClient) {
        let first = client
            .fetch_boards(&taskboard_sync_nextcloud::Validators::default())
            .await
            .expect("first conditional fetch must succeed");
        assert!(first.data.is_some(), "first poll must return data");
        assert!(
            first.validators.etag.is_some() || first.validators.last_modified.is_some(),
            "data-bearing responses should carry validators (server-dependent)"
        );

        // Server-dependent: a validator-aware server answers 304 here.
        if let Ok(cached) = client.fetch_boards(&first.validators).await
            && cached.data.is_none()
        {
            assert_eq!(
                cached.validators, first.validators,
                "304 must keep the validators usable for the next poll"
            );
        }

        // Mutate, then poll conditionally with the stale validators.
        let run = run_id();
        let color =
            taskboard_sync_nextcloud::DeckColor::from_hex("00c2e0").expect("valid cycle color");
        let created = client
            .create_board(&run, &color)
            .await
            .expect("mutation board creation must succeed");

        let after = client
            .fetch_boards(&first.validators)
            .await
            .expect("post-mutation conditional fetch must succeed");
        let boards = after
            .data
            .unwrap_or_else(|| panic!("stale validators must not yield 304 after a mutation"));
        assert!(
            boards.iter().any(|b| b.id == created.id),
            "post-mutation listing must contain the new board"
        );

        let deleted = client
            .delete_board(created.id)
            .await
            .expect("teardown delete");
        assert!(!deleted.is_live());
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
