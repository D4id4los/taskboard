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
//! # Usage
//!
//! Open a database (migrations run at boot), wrap it in the storage actor,
//! and command it through the handle:
//!
//! ```
//! use std::sync::Arc;
//! use taskboard_domain::persistence::TaskRepository;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let repo = Arc::new(taskboard_storage_sqlite::open_memory().await?);
//! let (handle, join) = taskboard_storage_sqlite::spawn_storage_actor(repo);
//!
//! handle.apply(vec![]).await?; // one batch, one transaction
//! let state = handle.load().await?; // full hydration for boot
//!
//! drop(handle); // closing the inbox stops the actor…
//! join.await?; // …which the bootstrap awaits for a clean shutdown
//! # Ok(())
//! # }
//! ```
//!
//! # Layout
//!
//! - `connect`: `open`/`open_memory` with pragmas and boot migrations
//! - `repo`: the [`SqliteTaskRepository`] port adapter
//! - `codec`: pure TEXT codecs for ids, op tags, phases, validator keys
//! - `error`: [`OpenError`] and the `sqlx::Error` → `RepositoryError` map
//! - `actor`: the storage actor and its handle

#![forbid(unsafe_code)]

pub(crate) mod actor;
pub(crate) mod codec;
pub(crate) mod connect;
pub(crate) mod error;
pub(crate) mod port;
pub(crate) mod repo;

pub use actor::{StorageCommand, StorageHandle, spawn_storage_actor};
pub use connect::{open, open_memory};
pub use error::OpenError;
pub use repo::SqliteTaskRepository;
