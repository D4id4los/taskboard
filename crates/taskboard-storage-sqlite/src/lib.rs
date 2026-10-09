// SPDX-License-Identifier: MIT OR Apache-2.0
//! Local `SQLite` persistence actor for `taskboard`.
//!
//! Ensures the application boots instantly with cached data and keeps
//! operating when network connectivity is lost (kiosk mode). All disk IO
//! happens on background Tokio tasks; the state engine commands this actor
//! exclusively over [`tokio::sync::mpsc`] write channels
//! ([`StorageCommand`], [`StorageHandle`], [`spawn_storage_actor`]).
//!
//! The repository implements the domain's
//! [`TaskRepository`](taskboard_domain::persistence::TaskRepository) port:
//! `load` hydrates the full
//! [`PersistedState`](taskboard_domain::persistence::PersistedState),
//! `apply` lands a whole batch in one transaction. `pending_ops` is never
//! stored — it is derived at load from the outbox depth. Tombstones are
//! ordinary rows and are retained forever (see
//! `docs/adr/0005-sqlite-local-persistence.org`).
//!
//! # Compile-time checked SQL and the offline cache
//!
//! SQL queries are compile-time checked against the local schema via
//! `sqlx`'s `query!` macros. The macros resolve against the committed
//! `.sqlx/` offline cache at the workspace root; if a build errors with a
//! `DATABASE_URL` complaint (usually after touching a query or a
//! migration), regenerate the cache and commit it together with the change:
//!
//! ```text
//! scripts/sqlx-prepare.sh          # runs sqlx-cli 0.9 against a throwaway db
//! ```
//!
//! Install the matching CLI once (version skew corrupts the offline data
//! format): `cargo install sqlx-cli --version 0.9 --no-default-features
//! --features sqlite`. CI compiles with `SQLX_OFFLINE=true` and fails any
//! PR that forgot to refresh `.sqlx/`.
//!
//! # Layout
//!
//! - [`connect`]: `open`/`open_memory` with pragmas and boot migrations
//! - [`repo`]: the [`SqliteTaskRepository`] port adapter
//! - [`codec`]: pure TEXT codecs for ids, op tags, phases, validator keys
//! - [`error`]: [`OpenError`] and the `sqlx::Error` → `RepositoryError` map
//! - [`actor`]: the storage actor and its handle

#![forbid(unsafe_code)]

pub mod actor;
pub mod codec;
pub mod connect;
pub mod error;
pub mod repo;

pub use actor::{StorageCommand, StorageHandle, spawn_storage_actor};
pub use connect::{open, open_memory};
pub use error::OpenError;
pub use repo::SqliteTaskRepository;
