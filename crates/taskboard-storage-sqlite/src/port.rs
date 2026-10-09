// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `TaskRepository` port adapter for [`StorageHandle`].
//!
//! With this impl the state engine consumes the storage actor through the
//! domain's port (`Arc<dyn TaskRepository>`): the actor's oneshot
//! round-trips become the port's `BoxFuture`s. The engine crate never
//! depends on this one (ADR 0006).

use taskboard_domain::persistence::{
    BoxFuture, PersistedState, PersistenceAction, RepositoryError, TaskRepository,
};

use crate::actor::StorageHandle;

impl TaskRepository for StorageHandle {
    fn load(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        Box::pin(self.load())
    }

    fn apply(&self, actions: Vec<PersistenceAction>) -> BoxFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.apply(actions))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spawn_storage_actor;
    use std::sync::Arc;

    async fn handle_over_memory() -> (StorageHandle, tokio::task::JoinHandle<()>) {
        let repo = crate::open_memory().await.expect("memory db opens");
        spawn_storage_actor(Arc::new(repo))
    }

    #[tokio::test]
    async fn ok_results_delegate_through_the_actor() {
        let (handle, join) = handle_over_memory().await;
        let repo: Arc<dyn TaskRepository> = Arc::new(handle);

        let state = repo.load().await.expect("fresh db loads");
        assert_eq!(state, PersistedState::default());

        repo.apply(vec![taskboard_domain::PersistenceAction::UpsertSyncStatus(
            taskboard_domain::SyncStatus {
                phase: taskboard_domain::SyncPhase::Offline,
                last_success: None,
                pending_ops: 0,
            },
        )])
        .await
        .expect("apply");
        let state = repo.load().await.expect("reload");
        assert_eq!(state.sync.phase, taskboard_domain::SyncPhase::Offline);
        drop(repo);
        let _ = join.await;
    }

    #[tokio::test]
    async fn dead_actor_maps_to_unavailable() {
        let (handle, join) = handle_over_memory().await;
        let probe = handle.clone();
        drop(handle);
        join.abort(); // the actor is gone; the inbox is closed

        let repo: Arc<dyn TaskRepository> = Arc::new(probe);
        assert!(matches!(
            repo.load().await,
            Err(RepositoryError::Unavailable)
        ));
        assert!(matches!(
            repo.apply(vec![]).await,
            Err(RepositoryError::Unavailable)
        ));
    }
}
