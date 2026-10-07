// SPDX-License-Identifier: MIT OR Apache-2.0
//! Pure domain types, entities, and inter-actor protocols for `taskboard`.
//!
//! This crate is the functional core of the system. It must remain pure:
//! zero network, zero UI, zero persistence, and zero asynchronous runtime
//! dependencies. All inter-actor communication types ([`StateCommand`]-style
//! message enums) live here so that every other workspace crate can share
//! them without coupling to each other.
//!
//! Design rules (see `AGENTS.md`):
//! - Domain logic is expressed as pure functions: inputs in, outputs out,
//!   no hidden mutable state.
//! - IO interfaces (ports, e.g. `TaskRepository`) are defined here as traits
//!   and implemented by adapter crates (`taskboard-storage-sqlite`,
//!   `taskboard-sync-nextcloud`).

#![forbid(unsafe_code)]

use serde::Serialize;

/// The complete, immutable UI-facing application state.
///
/// The state engine publishes snapshots of this type through an
/// `arc_swap::ArcSwap<AppState>`; the UI reads them lock-free. Treat any
/// change to this type's shape as a snapshot-breaking change: `insta`
/// tests will surface the diff.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AppState {
    /// Server-side UTC timestamp of the last successful state mutation.
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
}
