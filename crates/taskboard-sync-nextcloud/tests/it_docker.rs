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
    attachment_content_lifecycle, board_lifecycle, capabilities_probe, conditional_read_cycle,
    delete_missing_board_is_typed_error, full_tree_lifecycle, lists_boards, startdate_round_trip,
    startdate_unsupported_on_deck1175,
};
use common::{deck_client, live_docker_config, live_docker_deck1175_config, with_deadline};

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
    pub(crate) use super::common::sync_pair::{SyncPair, boot_pair, eventually};
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
        // The binding persists the instant `BindBoard` lands; the cycle's
        // *merge* is what adopts the remote stacks — wait for that.
        eventually(&pair, |app| {
            !app.stacks.is_empty()
                && app.boards.values().any(|b| {
                    b.remote.as_ref().map(|r| r.board)
                        == Some(taskboard_domain::RemoteBoardId(board.id))
                })
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
                        startdate: None,
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
                        startdate: None,
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
        eventually(&pair, |app| app.sync.aggregate().pending_ops == 0).await;

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
                        startdate: None,
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
                        startdate: None,
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
                        startdate: None,
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
        eventually(&pair, |app| app.sync.aggregate().pending_ops == 0).await;
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

    // ---- chunk 4: the sidecar convergence suite (C1–C4) ----------------

    /// Shared scaffolding: one fresh board + stack, two full engine+actor
    /// pairs (two devices) bound to it, and one locally-created task on
    /// A whose binding both sides learn. Returns everything C1/C2/C3 need.
    struct Convergence {
        client: DeckClient,
        board: u64,
        a: SyncPair,
        b: SyncPair,
        task_id: taskboard_domain::TaskId,
        card: u64,
    }

    async fn boot_convergence(cfg: &LiveCfg) -> Convergence {
        let client = deck_client(cfg);
        let run = run_id();
        let board = retry(|| async {
            client
                .create_board(&run, &DeckColor::from_hex("00c2e0").unwrap())
                .await
        })
        .await
        .expect("board creation must succeed");
        let _stack = retry(|| async { client.create_stack(board.id, "col", 0).await })
            .await
            .expect("stack creation must succeed");

        let a = boot_pair(client.clone()).await;
        a.set_board(board.id).await;
        let b = boot_pair(client.clone()).await;
        b.set_board(board.id).await;

        // A authors the task; the echo binds it; B adopts it next cycle.
        let taskboard_domain::CommandOutcome::CreatedTask(task_id) = a
            .engine
            .execute(taskboard_domain::StateCommand::CreateTask {
                title: format!("{run}-task"),
                stack: *a
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
        eventually(&a, |app| {
            app.tasks.get(&task_id).is_some_and(|t| t.remote.is_some())
        })
        .await;
        let card = a.engine.shared_state().load_full().tasks[&task_id]
            .remote
            .expect("bound")
            .card
            .get();
        eventually(&b, |app| {
            app.tasks
                .values()
                .any(|t| t.remote.is_some_and(|r| r.card.get() == card))
        })
        .await;
        Convergence {
            client,
            board: board.id,
            a,
            b,
            task_id,
            card,
        }
    }

    impl Convergence {
        fn b_task(&self) -> taskboard_domain::TaskId {
            self.b
                .engine
                .shared_state()
                .load_full()
                .tasks
                .values()
                .find(|t| t.remote.is_some_and(|r| r.card.get() == self.card))
                .map(|t| t.id)
                .expect("B adopted the task")
        }

        async fn anchor_exists_on_server(&self) -> bool {
            let stacks =
                retry(|| async { self.client.stacks(self.board, StackFilter::Active).await })
                    .await
                    .expect("server listing");
            let anchor = stacks
                .iter()
                .flat_map(|s| &s.cards)
                .find(|c| c.title == taskboard_domain::ANCHOR_CARD_TITLE);
            let Some(anchor) = anchor else {
                return false;
            };
            // Confirmed: the detail carries the reserved-filename attachment.
            let detail = retry(|| async {
                self.client
                    .card(self.board, anchor.stack_id, anchor.id)
                    .await
            })
            .await
            .expect("anchor detail");
            detail.attachments.iter().any(|att| {
                att.extended_data
                    .as_ref()
                    .is_some_and(|d| d.file_name == "taskboard-appdata.json")
            })
        }

        async fn teardown(self) {
            let deleted = retry(|| async { self.client.delete_board(self.board).await }).await;
            self.a.shutdown().await;
            self.b.shutdown().await;
            assert!(deleted.is_ok(), "board teardown must succeed");
        }
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Field {
        Effort,
        Scheduled,
    }

    async fn set_field(
        pair: &SyncPair,
        task: taskboard_domain::TaskId,
        field: Field,
        value: Option<i64>,
    ) {
        let scheduled =
            value.map(|secs| chrono::Utc::now() + chrono::TimeDelta::try_seconds(secs).unwrap());
        pair.engine
            .execute(taskboard_domain::StateCommand::UpdateTask {
                id: task,
                changes: taskboard_domain::TaskChanges {
                    effort: match field {
                        Field::Effort => value.map(|secs| {
                            Some(taskboard_domain::entities::Effort::new(
                                u64::try_from(secs).unwrap(),
                            ))
                        }),
                        Field::Scheduled => None,
                    },
                    scheduled: match field {
                        Field::Scheduled => Some(scheduled),
                        Field::Effort => None,
                    },
                    ..taskboard_domain::TaskChanges::default()
                },
            })
            .await
            .expect("accepted");
        pair.sync_now().await;
    }

    /// C1 — app-field convergence: A's effort edit flushes (the anchor
    /// is created server-side), B discovers, downloads, and folds it;
    /// the cleared-field write then converges as `{v: null}` end to end.
    pub(crate) async fn app_field_convergence(cfg: &LiveCfg) {
        let c = boot_convergence(cfg).await;
        set_field(&c.a, c.task_id, Field::Effort, Some(900)).await;

        // A's flush created the confirmed anchor on the server.
        tokio::time::timeout(super::common::LIVE_DEADLINE, async {
            loop {
                if c.anchor_exists_on_server().await {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
        })
        .await
        .expect("the anchor appears within the deadline");

        // B folds the effort.
        let b_task = c.b_task();
        eventually(&c.b, |app| {
            app.tasks[&b_task].effort == Some(taskboard_domain::entities::Effort::new(900))
        })
        .await;

        // The cleared-field write converges as null.
        set_field(&c.a, c.task_id, Field::Effort, None).await;
        eventually(&c.b, |app| app.tasks[&b_task].effort.is_none()).await;

        c.teardown().await;
    }

    /// C2 — rule add → tombstone → converge: the document is the rule's
    /// remote identity (B holds a rule with A's UUID), and the removal
    /// tombstone wins by clock end to end.
    pub(crate) async fn rule_convergence(cfg: &LiveCfg) {
        use taskboard_domain::reminders::ReminderOffset;
        let c = boot_convergence(cfg).await;

        let taskboard_domain::CommandOutcome::ReminderSet { rule } =
            c.a.engine
                .execute(taskboard_domain::StateCommand::SetReminderRule {
                    task: c.task_id,
                    offset: ReminderOffset::before(chrono::TimeDelta::try_minutes(-30).unwrap())
                        .unwrap(),
                })
                .await
                .expect("accepted")
        else {
            panic!("wrong receipt");
        };
        c.a.sync_now().await;
        // A's flush carries the rule; B adopts it under the same UUID.
        let b_task = c.b_task();
        eventually(&c.b, |app| {
            app.reminders
                .rules
                .values()
                .any(|r| r.task == b_task && r.id == rule)
        })
        .await;

        // A clears: the tombstone wins on B's next fold.
        c.a.engine
            .execute(taskboard_domain::StateCommand::ClearReminderRules { task: c.task_id })
            .await
            .expect("accepted");
        c.a.sync_now().await;
        eventually(&c.b, |app| {
            !app.reminders.rules.values().any(|r| r.id == rule)
        })
        .await;

        c.teardown().await;
    }

    /// C3 — the guard mirror's echo-idempotence half (tier 1 owns the
    /// deterministic race): after A's flush, A's next cycle re-folds its
    /// own document and the value survives with its clock unchanged.
    pub(crate) async fn echo_idempotence(cfg: &LiveCfg) {
        let c = boot_convergence(cfg).await;
        set_field(&c.a, c.task_id, Field::Effort, Some(900)).await;
        let before = c.a.engine.shared_state().load_full().tasks[&c.task_id].clone();
        assert_eq!(
            before.effort,
            Some(taskboard_domain::entities::Effort::new(900))
        );

        // A few of A's own cycles re-fold the document it wrote.
        for _ in 0..3 {
            c.a.sync_now().await;
        }
        let after = c.a.engine.shared_state().load_full().tasks[&c.task_id].clone();
        assert_eq!(
            after.effort,
            Some(taskboard_domain::entities::Effort::new(900)),
            "the value survives its own echo"
        );
        assert_eq!(
            after.clocks.effort, before.clocks.effort,
            "the write clock never moves backwards through the fold"
        );

        c.teardown().await;
    }

    /// C4 — the deck1175 variant: the `scheduled` fallback lane end to
    /// end on a `card_startdate == false` server (the native verdict
    /// skip + document fold + flush emission all conservative).
    pub(crate) async fn scheduled_fallback_convergence(cfg: &LiveCfg) {
        let c = boot_convergence(cfg).await;
        set_field(&c.a, c.task_id, Field::Scheduled, Some(86_400)).await;

        tokio::time::timeout(super::common::LIVE_DEADLINE, async {
            loop {
                if c.anchor_exists_on_server().await {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
        })
        .await
        .expect("the anchor appears within the deadline");

        let b_task = c.b_task();
        eventually(&c.b, |app| app.tasks[&b_task].scheduled.is_some()).await;

        c.teardown().await;
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

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_sync_app_field_convergence() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(d::app_field_convergence(&cfg))
        .await
        .expect("test must finish within the outer deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_sync_rule_convergence() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(d::rule_convergence(&cfg))
        .await
        .expect("test must finish within the outer deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_sync_echo_idempotence() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(d::echo_idempotence(&cfg))
        .await
        .expect("test must finish within the outer deadline");
}

#[tokio::test]
#[ignore = "requires the pinned Deck 1.17.5 gate tier (nextcloud_it_setup.sh with DECK_VERSION=v1.17.5)"]
async fn it_nextcloud_docker_deck1175_scheduled_fallback_convergence() {
    let Some(cfg) = live_docker_deck1175_config() else {
        return; // honest skip: the gate tier runs locally, CI does not wire it
    };
    with_deadline(d::scheduled_fallback_convergence(&cfg))
        .await
        .expect("test must finish within the outer deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_attachment_content_lifecycle() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(attachment_content_lifecycle(&deck_client(&cfg)))
        .await
        .expect("test must finish within the live deadline");
}

#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_startdate_round_trip() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(startdate_round_trip(&deck_client(&cfg)))
        .await
        .expect("test must finish within the live deadline");
}

/// The Deck 1.17.5 gate tier (DDF-10): the same attachment contract
/// fixtures, run against a second instance pinned to
/// `DECK_VERSION=v1.17.5` + `TASKBOARD_IT_NC_IMAGE=nextcloud:33-apache`,
/// credentials under the `TASKBOARD_IT_DOCKER_DECK1175` prefix. Unset ⇒
/// skip, never fail; the gate run happens locally against the pinned
/// instance (CI does not wire the variant).
#[tokio::test]
#[ignore = "requires the pinned Deck 1.17.5 gate tier (nextcloud_it_setup.sh with DECK_VERSION=v1.17.5)"]
async fn it_nextcloud_docker_deck1175_attachment_content_lifecycle() {
    let Some(cfg) = live_docker_deck1175_config() else {
        return;
    };
    with_deadline(attachment_content_lifecycle(&deck_client(&cfg)))
        .await
        .expect("test must finish within the live deadline");
}

/// The Deck 1.17.5 gate pin: the *measured* server behavior (create
/// ignores the field, the full-send PUT silently drops it) — the
/// capability table's evidence, never adapted around (D23).
#[tokio::test]
#[ignore = "requires the pinned Deck 1.17.5 gate tier (nextcloud_it_setup.sh with DECK_VERSION=v1.17.5)"]
async fn it_nextcloud_docker_deck1175_startdate_unsupported() {
    let Some(cfg) = live_docker_deck1175_config() else {
        return;
    };
    with_deadline(startdate_unsupported_on_deck1175(&deck_client(&cfg)))
        .await
        .expect("test must finish within the live deadline");
}

/// The D-V1 seam pinned against the capability-true tier (Deck 1.19.0).
#[tokio::test]
#[ignore = "requires the dockerized Nextcloud tier (scripts/nextcloud_it_setup.sh up)"]
async fn it_nextcloud_docker_capabilities() {
    let Some(cfg) = live_docker_config() else {
        return;
    };
    with_deadline(capabilities_probe(&deck_client(&cfg), "1.19.0", true))
        .await
        .expect("test must finish within the live deadline");
}

/// The D-V1 seam pinned against the gate tier (Deck 1.17.5).
#[tokio::test]
#[ignore = "requires the pinned Deck 1.17.5 gate tier (nextcloud_it_setup.sh with DECK_VERSION=v1.17.5)"]
async fn it_nextcloud_docker_deck1175_capabilities() {
    let Some(cfg) = live_docker_deck1175_config() else {
        return;
    };
    with_deadline(capabilities_probe(&deck_client(&cfg), "1.17.5", false))
        .await
        .expect("test must finish within the live deadline");
}
