// SPDX-License-Identifier: MIT OR Apache-2.0
//! Error types of the sqlite adapter.

use taskboard_domain::persistence::RepositoryError;

/// Failure to open/migrate a database (construction-time, not port-level).
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// The database file could not be opened or the pool created.
    #[error("sqlite connect failed")]
    Connect(#[source] sqlx::Error),
    /// Pending migrations could not be applied.
    #[error("sqlite migration failed")]
    Migrate(#[source] sqlx::migrate::MigrateError),
}

/// Maps sqlite failures onto the port's two failure classes (plan decision
/// 12): decoding/integrity failures are `Corrupted`, everything else (io,
/// busy, locked) is transient `Unavailable`. The match is on error shape,
/// never on message text. A crate-local function rather than a
/// `From` impl — both sides are foreign types (orphan rule).
/// The `must_use` is implied by the pure mapping (clippy suggestion suppressed:
/// the function is always used inside `map_err` closures).
#[allow(clippy::must_use_candidate)]
pub fn repo_error(err: &sqlx::Error) -> RepositoryError {
    match &err {
        sqlx::Error::ColumnDecode { .. } | sqlx::Error::ColumnNotFound(_) => {
            RepositoryError::Corrupted
        }
        sqlx::Error::Database(db) => match db.kind() {
            sqlx::error::ErrorKind::UniqueViolation
            | sqlx::error::ErrorKind::ForeignKeyViolation
            | sqlx::error::ErrorKind::NotNullViolation
            | sqlx::error::ErrorKind::CheckViolation => RepositoryError::Corrupted,
            // Other database errors (syntax, RAISE-triggered aborts, …)
            // are not integrity verdicts about the stored data.
            // Non-exhaustive upstream enum: everything else (syntax,
            // RAISE-triggered aborts, …) is not an integrity verdict.
            _ => RepositoryError::Unavailable,
        },
        _ => RepositoryError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_class_errors_map_to_corrupted() {
        let err = sqlx::Error::ColumnNotFound("title".into());
        assert_eq!(repo_error(&err), RepositoryError::Corrupted);
    }

    #[test]
    fn constraint_violations_map_to_corrupted() {
        let err = sqlx::Error::Database(Box::new(FakeDbError));
        assert_eq!(repo_error(&err), RepositoryError::Corrupted);
    }

    #[test]
    fn io_errors_map_to_unavailable() {
        let err = sqlx::Error::Io(std::io::Error::other("disk gone"));
        assert_eq!(repo_error(&err), RepositoryError::Unavailable);
    }

    /// A `DatabaseError` whose kind is a unique violation (shape of a real
    /// sqlite `UNIQUE` failure; the driver's own error type is not
    /// constructible outside the crate).
    struct FakeDbError;

    impl std::fmt::Debug for FakeDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("FakeDbError")
        }
    }
    impl std::fmt::Display for FakeDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("UNIQUE constraint failed")
        }
    }
    impl std::error::Error for FakeDbError {}

    impl sqlx::error::DatabaseError for FakeDbError {
        fn message(&self) -> &'static str {
            "UNIQUE constraint failed"
        }
        fn kind(&self) -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::UniqueViolation
        }
        fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
            self
        }
        fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
            self
        }
    }
}
