// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `taskboard` State Engine actor.
//!
//! Single source of runtime truth. The engine consumes
//! [`EngineCommand`]s over an owned `tokio::sync::mpsc` inbox, advances
//! its working state **only** by interpreting the action batches that
//! were durably applied to the repository (memory ≡ disk by
//! construction, ADR 0006), publishes immutable [`AppState`] snapshots
//! through `arc_swap::ArcSwap<AppState>`, and emits
//! [`tokio::sync::broadcast`] [`EngineSignal`]s to prompt UI repaints.
//!
//! Boundary rules (see `AGENTS.md`):
//! - Owns the *write* side of `ArcSwap<AppState>`; readers never mutate.
//! - Consumes the domain's [`TaskRepository`] port (`Arc<dyn
//!   TaskRepository>`) — this crate never depends on a storage
//!   implementation.
//! - Command semantics live in the domain (`taskboard_domain::
//!   plan_command`, `apply_sync_report`); this crate is the thin
//!   orchestrator: channels, `ArcSwap` publishing, sequencing.
//! - Tested as a black box: drive commands through the handle, snapshot
//!   the resulting `AppState` with `insta`.
//!
//! # Usage
//!
//! ```
//! use std::sync::Arc;
//! use taskboard_domain::{
//!     AppState, Clock, IdGenerator, StateCommand, SyncCommand, SyncReport,
//!     SystemEvent, test_support::{CountingIds, InMemoryRepository},
//! };
//! use taskboard_state::spawn_state_engine;
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! // The bootstrap owns the channel graph and keeps the matching halves.
//! let (sync_out, _sync_in) = tokio::sync::mpsc::channel::<SyncCommand>(8);
//! let (report_tx, report_rx) = tokio::sync::mpsc::channel::<SyncReport>(8);
//! let (system_tx, system_rx) = tokio::sync::broadcast::channel::<SystemEvent>(8);
//!
//! let (engine, join) = spawn_state_engine(
//!     Arc::new(InMemoryRepository::new()),
//!     Arc::new(CountingIds::new()),
//!     Arc::new(taskboard_domain::SystemClock),
//!     sync_out,
//!     report_rx,
//!     system_rx,
//! )
//! .await?;
//!
//! // Lock-free UI reads; typed command receipts for the CLI.
//! let _state: Arc<arc_swap::ArcSwap<AppState>> = engine.shared_state();
//! let outcome = engine.execute(StateCommand::CreateStack {
//!     title: "todo".into(),
//!     order: 1,
//! }).await?;
//! assert!(matches!(
//!     outcome,
//!     taskboard_domain::CommandOutcome::CreatedStack(_)
//! ));
//!
//! system_tx.send(SystemEvent::Shutdown)?;
//! join.await?;
//! # Ok(())
//! # }
//! # #[tokio::main] async fn main() { demo().await.unwrap(); }
//! ```

#![forbid(unsafe_code)]

pub(crate) mod actor;
pub(crate) mod core;
pub mod error;

pub use actor::{EngineCommand, EngineHandle, spawn_state_engine};
pub use core::EngineCore;
pub use error::{EngineStartupError, ExecuteError};
