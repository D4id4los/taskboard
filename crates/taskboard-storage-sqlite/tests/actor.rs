// SPDX-License-Identifier: MIT OR Apache-2.0
//! A1–A3 — storage actor routing tests (plan §10): the actor adds no
//! policy, so only routing, the flush barrier, and shutdown are pinned.

use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::persistence::PersistenceAction;
use taskboard_storage_sqlite::{open_memory, spawn_storage_actor};

fn board_action(n: u128) -> PersistenceAction {
    PersistenceAction::UpsertBoard(taskboard_domain::entities::Board {
        id: taskboard_domain::ids::BoardId::from(uuid::Uuid::from_u128(n)),
        remote: None,
        title: "board".into(),
        color: taskboard_domain::entities::Color::new("ff0000"),
        archived: false,
        deleted: false,
        remote_seen: None,
    })
}

/// A1 — apply through the handle lands; load through a second handle sees
/// it (routing works, handles are cloneable).
#[tokio::test]
async fn apply_routes_to_repo_and_load_observes() {
    let repo = Arc::new(open_memory().await.expect("open"));
    let (handle, _join) = spawn_storage_actor(repo.clone());
    let observer = handle.clone();

    handle.apply(vec![board_action(1)]).await.expect("apply");
    let loaded = observer.load().await.expect("load");
    assert_eq!(loaded.boards.len(), 1);
}

/// A2 — FIFO + reply-after-commit: two applies fired concurrently both
/// land, and once their replies resolved their effects are observable
/// without extra waiting (the flush semantics Phase 5 relies on).
#[tokio::test]
async fn apply_reply_is_a_flush_barrier() {
    let repo = Arc::new(open_memory().await.expect("open"));
    let (handle, _join) = spawn_storage_actor(repo);

    // Both commands are in flight at once; the mpsc inbox serializes them.
    let (first, second) = tokio::join!(
        handle.apply(vec![board_action(1)]),
        handle.apply(vec![board_action(2)]),
    );
    first.expect("apply 1");
    second.expect("apply 2");

    // Both replies resolved after their commits — the load must already
    // see both batches.
    let loaded = handle.load().await.expect("load");
    assert_eq!(loaded.boards.len(), 2);
}

/// A3 — dropping the last handle closes the inbox and the actor task exits
/// (deadline-guarded await, not a sleep).
#[tokio::test]
async fn dropping_handles_shuts_the_actor_down() {
    let repo = Arc::new(open_memory().await.expect("open"));
    let (handle, join) = spawn_storage_actor(repo.clone());
    let second = handle.clone();
    drop(handle);
    drop(second);

    let finished = tokio::time::timeout(Duration::from_secs(5), join).await;
    assert!(
        finished.is_ok(),
        "actor task must exit once all handles are dropped"
    );
}
