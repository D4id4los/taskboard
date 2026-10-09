// SPDX-License-Identifier: MIT OR Apache-2.0
//! Engine-lifecycle error types.
//!
//! Responsibility split: the domain owns what a command *means* (the
//! semantic [`CommandError`] rejections), this crate owns what *running*
//! the engine means — storage round-trip failures and dead channels.
//! The CLI matches once, on [`ExecuteError`].

use taskboard_domain::{CommandError, RepositoryError};

/// Failure classes of an executed command.
#[derive(Debug, thiserror::Error)]
pub enum ExecuteError {
    /// The command was semantically rejected (unknown/deleted target,
    /// no live board). Never touched storage.
    #[error("command rejected")]
    Rejected(#[from] CommandError),
    /// The persistence batch failed; memory was never advanced, so the
    /// published state is unchanged and a retry re-plans from it.
    #[error("persistence failed; state unchanged")]
    Storage(#[from] RepositoryError),
    /// The engine loop is gone (stopped or never started on this handle).
    #[error("engine is stopped")]
    EngineGone,
}

/// Failure classes of engine boot.
#[derive(Debug, thiserror::Error)]
pub enum EngineStartupError {
    /// The boot hydration (`repo.load()`) failed. The bootstrap decides
    /// retry-vs-abort; the engine never starts on unhydrated state.
    #[error("boot hydration failed")]
    Load(#[from] RepositoryError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejections_wrap_their_semantic_cause() {
        let err = ExecuteError::from(CommandError::NoBoard);
        assert!(matches!(err, ExecuteError::Rejected(CommandError::NoBoard)));
    }

    #[test]
    fn storage_class_carries_the_repository_error() {
        let err = ExecuteError::from(RepositoryError::Unavailable);
        assert!(matches!(
            err,
            ExecuteError::Storage(RepositoryError::Unavailable)
        ));
    }

    #[test]
    fn startup_error_carries_the_load_error() {
        let err = EngineStartupError::from(RepositoryError::Corrupted);
        assert!(matches!(
            err,
            EngineStartupError::Load(RepositoryError::Corrupted)
        ));
    }
}
