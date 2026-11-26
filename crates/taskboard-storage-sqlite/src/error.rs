// SPDX-License-Identifier: MIT OR Apache-2.0
//! Error types of the sqlite adapter.

use taskboard_domain::persistence::{CorruptionDetail, DecodeReason, DecodeStage, RepositoryError};

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
/// `From` impl — both sides are foreign types (orphan rule). Non-exhaustive
/// upstream enums: everything else (syntax, RAISE-triggered aborts, …) is
/// not an integrity verdict.
#[allow(clippy::must_use_candidate)]
pub(crate) fn repo_error(err: &sqlx::Error) -> RepositoryError {
    // No table/row is in hand at this mapping (plan D-B2-4): the detail
    // carries the failure class only (`CorruptionDetail::UNKNOWN`'s
    // legs plus the stage/reason the shape implies).
    match &err {
        sqlx::Error::ColumnDecode { .. } | sqlx::Error::ColumnNotFound(_) => {
            RepositoryError::corrupted(CorruptionDetail::staged(
                DecodeStage::Column,
                DecodeReason::MissingColumn,
            ))
        }
        sqlx::Error::Database(db) => match db.kind() {
            sqlx::error::ErrorKind::UniqueViolation
            | sqlx::error::ErrorKind::ForeignKeyViolation
            | sqlx::error::ErrorKind::NotNullViolation
            | sqlx::error::ErrorKind::CheckViolation => RepositoryError::corrupted(
                CorruptionDetail::staged(DecodeStage::Invariant, DecodeReason::ConstraintViolation),
            ),
            // Other database errors (syntax, RAISE-triggered aborts, …)
            // are not integrity verdicts about the stored data.
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
        assert!(matches!(repo_error(&err), RepositoryError::Corrupted(_)));
    }

    #[test]
    fn constraint_violations_map_to_corrupted() {
        let err = sqlx::Error::Database(Box::new(FakeDbError::new(ErrorKinds::unique_violation)));
        assert!(matches!(repo_error(&err), RepositoryError::Corrupted(_)));
    }

    /// One row per constraint `ErrorKind` the driver reports: each is an
    /// integrity verdict about the stored data, so every one of them must
    /// carry the `Corrupted` verdict with the staged
    /// (`Invariant`, `ConstraintViolation`) detail. A mapping flip on any
    /// single kind fails its row. (#1375)
    #[test]
    fn every_constraint_kind_maps_to_the_corrupted_verdict() {
        for kind in [
            ErrorKinds::unique_violation,
            ErrorKinds::foreign_key_violation,
            ErrorKinds::not_null_violation,
            ErrorKinds::check_violation,
        ] {
            let err = sqlx::Error::Database(Box::new(FakeDbError::new(kind)));
            match repo_error(&err) {
                RepositoryError::Corrupted(detail) => {
                    assert_eq!(detail.stage, Some(DecodeStage::Invariant));
                    assert_eq!(detail.reason, Some(DecodeReason::ConstraintViolation));
                }
                RepositoryError::Unavailable => {
                    panic!("constraint kind {:?} mapped to Unavailable", kind());
                }
            }
        }
    }

    /// The catch-all arm for database errors that are not integrity
    /// verdicts: `ErrorKind` is exhaustive over the four constraint kinds
    /// the mapping treats plus `Other`/`ExclusionViolation`, so the
    /// unknown-kind arm is pinned with those two — both must stay
    /// transient `Unavailable`. (#1375)
    #[test]
    fn database_other_kind_maps_to_unavailable() {
        for kind in [ErrorKinds::other, ErrorKinds::exclusion_violation] {
            let err = sqlx::Error::Database(Box::new(FakeDbError::new(kind)));
            assert_eq!(repo_error(&err), RepositoryError::Unavailable);
        }
    }

    /// Column decode failures are integrity verdicts: the staged detail
    /// names the `Column` stage. (#1375)
    #[test]
    fn column_decode_maps_to_corrupted_with_column_stage() {
        let err = sqlx::Error::ColumnDecode {
            index: "0".to_string(),
            source: Box::new(std::io::Error::other("bad utf")),
        };
        match repo_error(&err) {
            RepositoryError::Corrupted(detail) => {
                assert_eq!(detail.stage, Some(DecodeStage::Column));
            }
            RepositoryError::Unavailable => panic!("ColumnDecode mapped to Unavailable"),
        }
    }

    #[test]
    fn io_errors_map_to_unavailable() {
        let err = sqlx::Error::Io(std::io::Error::other("disk gone"));
        assert_eq!(repo_error(&err), RepositoryError::Unavailable);
    }

    /// Unit-variant constructors for the `ErrorKind`s the fixtures need:
    /// `ErrorKind` is neither `Copy` nor `Clone`, so a fixture cannot own
    /// one and hand it back from `kind()` on a shared reference — a
    /// capturing-free constructor function pointer sidesteps that.
    #[allow(dead_code)]
    struct ErrorKinds;

    impl ErrorKinds {
        fn unique_violation() -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::UniqueViolation
        }
        fn foreign_key_violation() -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::ForeignKeyViolation
        }
        fn not_null_violation() -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::NotNullViolation
        }
        fn check_violation() -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::CheckViolation
        }
        fn other() -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::Other
        }
        fn exclusion_violation() -> sqlx::error::ErrorKind {
            sqlx::error::ErrorKind::ExclusionViolation
        }
    }

    /// A `DatabaseError` fixture parametrized over `ErrorKind` (shape of a
    /// real sqlite constraint failure; the driver's own error type is not
    /// constructible outside the crate).
    struct FakeDbError(fn() -> sqlx::error::ErrorKind);

    impl FakeDbError {
        fn new(kind: fn() -> sqlx::error::ErrorKind) -> Self {
            Self(kind)
        }
    }

    impl std::fmt::Debug for FakeDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("FakeDbError")
        }
    }
    impl std::fmt::Display for FakeDbError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("fake constraint failure")
        }
    }
    impl std::error::Error for FakeDbError {}

    impl sqlx::error::DatabaseError for FakeDbError {
        fn message(&self) -> &'static str {
            "fake constraint failure"
        }
        fn kind(&self) -> sqlx::error::ErrorKind {
            (self.0)()
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
