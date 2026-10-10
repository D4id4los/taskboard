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

use common::suite::{
    board_lifecycle, conditional_read_cycle, delete_missing_board_is_typed_error,
    full_tree_lifecycle, lists_boards,
};
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

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_full_tree_lifecycle() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(full_tree_lifecycle(&deck_client(&cfg)))
        .await
        .expect("test must finish within the live deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_conditional_read_cycle() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(conditional_read_cycle(&deck_client(&cfg)))
        .await
        .expect("test must finish within the live deadline");
}

// ---------------------------------------------------------------------
// D-series: two-way sync against the dockerized server (phase 4 exit).
// Each test boots the full engine + actor pair, drives it through engine
// commands / server-side mutations, and asserts on the published AppState
// and raw client reads. Skip, never fail, without the tier env.
// ---------------------------------------------------------------------

mod d_common {
    // The engine+actor harness lives with the other shared helpers; alias
    // the pieces the D-tests need here to keep the test bodies tight.
    pub(crate) use super::common::sync_pair::{boot_pair, eventually};
    pub(crate) use super::common::{LiveCfg, deck_client, retrying_server_errors as retry, run_id};
    pub(crate) use taskboard_sync_nextcloud::{DeckClient, DeckColor, NewCard, StackFilter};
}

mod d {
    use super::d_common::*;

    /// D1 — the phase exit demo: create a task through the engine, verify
    /// the card on the server; create a card server-side, verify it in the
    /// published state. Two-way, one board, one pair.
    pub(crate) async fn two_way_round_trip(cfg: &LiveCfg) {
        let client = deck_client(cfg);
        let run = run_id();
        let board = retry(|| async {
            client
                .create_board(&run, &DeckColor::from_hex("00c2e0").unwrap())
                .await
        })
        .await
        .expect("board creation must succeed");
        let stack = retry(|| async {
            client
                .create_stack(board.id, &format!("{run}-col"), 0)
                .await
        })
        .await
        .expect("stack creation must succeed");

        let pair = boot_pair(client.clone()).await;
        pair.set_board(board.id).await;
        eventually(&pair, |app| {
            app.boards
                .values()
                .any(|b| b.remote == Some(taskboard_domain::RemoteBoardId(board.id)))
        })
        .await;

        // Local → server.
        let taskboard_domain::CommandOutcome::CreatedTask(task_id) = pair
            .engine
            .execute(taskboard_domain::StateCommand::CreateTask {
                title: format!("{run}-task"),
                stack: *pair
                    .engine
                    .shared_state()
                    .load_full()
                    .stacks
                    .keys()
                    .next()
                    .unwrap(),
                order: 0,
            })
            .await
            .expect("accepted")
        else {
            panic!("wrong receipt");
        };
        eventually(&pair, |app| {
            app.tasks.get(&task_id).is_some_and(|t| t.remote.is_some())
        })
        .await;
        let binding = pair.engine.shared_state().load_full().tasks[&task_id]
            .remote
            .expect("bound");
        let stacks = retry(|| async { client.stacks(board.id, StackFilter::Active).await })
            .await
            .expect("server listing");
        assert!(
            stacks
                .iter()
                .flat_map(|s| &s.cards)
                .any(|c| c.id == binding.card.get() && c.title == format!("{run}-task")),
            "the pushed card must exist on the server"
        );

        // Server → local.
        let server_card = retry(|| async {
            client
                .create_card(
                    board.id,
                    stack.id,
                    &NewCard {
                        title: format!("{run}-remote"),
                        order: Some(1),
                        description: None,
                        duedate: None,
                        kind: None,
                    },
                )
                .await
        })
        .await
        .expect("server-side card creation must succeed");
        pair.sync_now().await;
        eventually(&pair, |app| {
            app.tasks
                .values()
                .any(|t| t.remote.is_some_and(|r| r.card.get() == server_card.id))
        })
        .await;

        // Teardown.
        let deleted = retry(|| async { client.delete_board(board.id).await }).await;
        pair.shutdown().await;
        assert!(deleted.is_ok(), "board teardown must succeed");
    }

    /// D2 — cross-stack reorder through the executor: a local move pushes
    /// the reorder primitive and the server lists the card under the
    /// target stack.
    pub(crate) async fn cross_stack_move_lands(cfg: &LiveCfg) {
        let client = deck_client(cfg);
        let run = run_id();
        let board = retry(|| async {
            client
                .create_board(&run, &DeckColor::from_hex("00c2e0").unwrap())
                .await
        })
        .await
        .unwrap();
        let stack_a = retry(|| async { client.create_stack(board.id, "a", 0).await })
            .await
            .unwrap();
        let stack_b = retry(|| async { client.create_stack(board.id, "b", 1).await })
            .await
            .unwrap();
        let card = retry(|| async {
            client
                .create_card(
                    board.id,
                    stack_a.id,
                    &NewCard {
                        title: format!("{run}-mover"),
                        order: Some(0),
                        description: None,
                        duedate: None,
                        kind: None,
                    },
                )
                .await
        })
        .await
        .unwrap();

        let pair = boot_pair(client.clone()).await;
        pair.set_board(board.id).await;
        eventually(&pair, |app| {
            app.tasks
                .values()
                .any(|t| t.remote.is_some_and(|r| r.card.get() == card.id))
        })
        .await;

        let app = pair.engine.shared_state().load_full();
        let task_id = app
            .tasks
            .values()
            .find(|t| t.remote.is_some_and(|r| r.card.get() == card.id))
            .map(|t| t.id)
            .unwrap();
        let target_stack = app
            .stacks
            .values()
            .find(|s| s.remote.is_some_and(|r| r.stack.get() == stack_b.id))
            .map(|s| s.id)
            .unwrap();
        drop(app);
        pair.engine
            .execute(taskboard_domain::StateCommand::MoveTask {
                id: task_id,
                stack: target_stack,
                order: 0,
            })
            .await
            .expect("accepted");
        eventually(&pair, |app| app.sync.pending_ops == 0).await;

        // Terminal state on the server: the card is listed under stack b.
        eventually_no_pair(&client, board.id, stack_b.id, card.id).await;

        let deleted = retry(|| async { client.delete_board(board.id).await }).await;
        pair.shutdown().await;
        assert!(deleted.is_ok());
    }

    async fn eventually_no_pair(client: &DeckClient, board: u64, stack_b: u64, card: u64) {
        tokio::time::timeout(super::common::LIVE_DEADLINE, async {
            loop {
                let stacks = retry(|| async { client.stacks(board, StackFilter::Active).await })
                    .await
                    .unwrap();
                if stacks
                    .iter()
                    .find(|s| s.id == stack_b)
                    .is_some_and(|s| s.cards.iter().any(|c| c.id == card))
                {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        })
        .await
        .expect("the moved card must land under the target stack");
    }

    /// D3 — archived listing behavior: archive server-side → pulled as
    /// `archived = true` (never tombstoned); unarchive → resurrects in
    /// place.
    pub(crate) async fn archived_cards_survive(cfg: &LiveCfg) {
        let client = deck_client(cfg);
        let run = run_id();
        let board = retry(|| async {
            client
                .create_board(&run, &DeckColor::from_hex("00c2e0").unwrap())
                .await
        })
        .await
        .unwrap();
        let stack = retry(|| async { client.create_stack(board.id, "col", 0).await })
            .await
            .unwrap();
        let card = retry(|| async {
            client
                .create_card(
                    board.id,
                    stack.id,
                    &NewCard {
                        title: format!("{run}-arch"),
                        order: Some(0),
                        description: None,
                        duedate: None,
                        kind: None,
                    },
                )
                .await
        })
        .await
        .unwrap();

        let pair = boot_pair(client.clone()).await;
        pair.set_board(board.id).await;
        eventually(&pair, |app| {
            app.tasks
                .values()
                .any(|t| t.remote.is_some_and(|r| r.card.get() == card.id))
        })
        .await;

        retry(|| async { client.archive_card(board.id, stack.id, card.id).await })
            .await
            .unwrap();
        pair.sync_now().await;
        eventually(&pair, |app| {
            app.tasks
                .values()
                .any(|t| t.remote.is_some_and(|r| r.card.get() == card.id) && t.archived)
        })
        .await;

        retry(|| async { client.unarchive_card(board.id, stack.id, card.id).await })
            .await
            .unwrap();
        pair.sync_now().await;
        eventually(&pair, |app| {
            app.tasks.values().any(|t| {
                t.remote.is_some_and(|r| r.card.get() == card.id) && !t.archived && !t.deleted
            })
        })
        .await;

        let deleted = client.delete_board(board.id).await;
        pair.shutdown().await;
        assert!(deleted.is_ok());
    }

    /// D4 — deleted-board listing behavior: deleting the bound board
    /// server-side cascades the local tombstones.
    pub(crate) async fn deleted_board_cascades(cfg: &LiveCfg) {
        let client = deck_client(cfg);
        let run = run_id();
        let board = client
            .create_board(&run, &DeckColor::from_hex("00c2e0").unwrap())
            .await
            .unwrap();
        let stack = client.create_stack(board.id, "col", 0).await.unwrap();
        let card = retry(|| async {
            client
                .create_card(
                    board.id,
                    stack.id,
                    &NewCard {
                        title: format!("{run}-doomed"),
                        order: Some(0),
                        description: None,
                        duedate: None,
                        kind: None,
                    },
                )
                .await
        })
        .await
        .unwrap();

        let pair = boot_pair(client.clone()).await;
        pair.set_board(board.id).await;
        eventually(&pair, |app| {
            app.tasks
                .values()
                .any(|t| t.remote.is_some_and(|r| r.card.get() == card.id))
        })
        .await;

        retry(|| async { client.delete_board(board.id).await })
            .await
            .unwrap();
        pair.sync_now().await;
        eventually(&pair, |app| {
            !app.tasks.is_empty()
                && app.tasks.values().all(|t| t.deleted)
                && app.boards.values().all(|b| b.deleted)
        })
        .await;

        pair.shutdown().await;
    }

    /// D5 — label visibility through the board payload (the NC35
    /// `labels()` 405 path must not matter): a server-created label is
    /// adopted.
    pub(crate) async fn server_labels_adopt(cfg: &LiveCfg) {
        let client = deck_client(cfg);
        let run = run_id();
        let board = client
            .create_board(&run, &DeckColor::from_hex("00c2e0").unwrap())
            .await
            .unwrap();
        let label = retry(|| async {
            client
                .create_label(
                    board.id,
                    &format!("{run}-lbl"),
                    &DeckColor::from_hex("ff0000").unwrap(),
                )
                .await
        })
        .await
        .unwrap();

        let pair = boot_pair(client.clone()).await;
        pair.set_board(board.id).await;
        eventually(&pair, |app| {
            app.labels
                .values()
                .any(|l| l.remote.is_some_and(|r| r.label.get() == label.id))
        })
        .await;

        let deleted = client.delete_board(board.id).await;
        pair.shutdown().await;
        assert!(deleted.is_ok());
    }

    /// D6 — `update_card` round-trip honoring `done`/duedate field
    /// writes: a local `SetTaskDone` + a due date push and read back
    /// correctly.
    pub(crate) async fn card_field_writes_round_trip(cfg: &LiveCfg) {
        let client = deck_client(cfg);
        let run = run_id();
        let board = client
            .create_board(&run, &DeckColor::from_hex("00c2e0").unwrap())
            .await
            .unwrap();
        let stack = client.create_stack(board.id, "col", 0).await.unwrap();
        let card = retry(|| async {
            client
                .create_card(
                    board.id,
                    stack.id,
                    &NewCard {
                        title: format!("{run}-fields"),
                        order: Some(0),
                        description: None,
                        duedate: None,
                        kind: None,
                    },
                )
                .await
        })
        .await
        .unwrap();

        let pair = boot_pair(client.clone()).await;
        pair.set_board(board.id).await;
        eventually(&pair, |app| {
            app.tasks
                .values()
                .any(|t| t.remote.is_some_and(|r| r.card.get() == card.id))
        })
        .await;

        let app = pair.engine.shared_state().load_full();
        let task_id = app
            .tasks
            .values()
            .find(|t| t.remote.is_some_and(|r| r.card.get() == card.id))
            .map(|t| t.id)
            .unwrap();
        drop(app);
        let due = chrono::Utc::now() + chrono::Duration::days(3);
        pair.engine
            .execute(taskboard_domain::StateCommand::UpdateTask {
                id: task_id,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(due)),
                    ..taskboard_domain::TaskChanges::default()
                },
            })
            .await
            .expect("accepted");
        pair.engine
            .execute(taskboard_domain::StateCommand::SetTaskDone {
                id: task_id,
                done: true,
            })
            .await
            .expect("accepted");
        eventually(&pair, |app| app.sync.pending_ops == 0).await;
        // Terminal state on the server: done and the due date, read back
        // through the raw client. Polled under the deadline: Deck's
        // listings lag its write cache (the D2 precedent).
        tokio::time::timeout(super::common::LIVE_DEADLINE, async {
            loop {
                let Ok(stacks) = client.stacks(board.id, StackFilter::Active).await else {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    continue;
                };
                if let Some(remote) = stacks
                    .iter()
                    .flat_map(|s| &s.cards)
                    .find(|c| c.id == card.id)
                    && remote.done.is_some()
                    && remote.duedate.map(|d| d.timestamp()) == Some(due.timestamp())
                {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        })
        .await
        .expect("the pushed done + due date must read back on the server");

        let deleted = retry(|| async { client.delete_board(board.id).await }).await;
        pair.shutdown().await;
        assert!(deleted.is_ok());
    }
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_sync_two_way_round_trip() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(d::two_way_round_trip(&cfg))
        .await
        .expect("test must finish within the outer deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_sync_cross_stack_move() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(d::cross_stack_move_lands(&cfg))
        .await
        .expect("test must finish within the outer deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_sync_archived_cards() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(d::archived_cards_survive(&cfg))
        .await
        .expect("test must finish within the outer deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_sync_deleted_board_cascade() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(d::deleted_board_cascades(&cfg))
        .await
        .expect("test must finish within the outer deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_sync_server_labels_adopt() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(d::server_labels_adopt(&cfg))
        .await
        .expect("test must finish within the outer deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_sync_card_field_writes() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(d::card_field_writes_round_trip(&cfg))
        .await
        .expect("test must finish within the outer deadline");
}
