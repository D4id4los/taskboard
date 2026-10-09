// SPDX-License-Identifier: MIT OR Apache-2.0
//! Nextcloud synchronization actor for `taskboard`.
//!
//! Implements the remote side of the offline-first sync strategy: pulls and
//! pushes tasks via the Nextcloud Deck OCS REST API without ever blocking the
//! state engine or the UI. Network failures are typed (via `thiserror`) and
//! reported to the state engine over channels so the UI can surface sync
//! status instead of failing.
//!
//! This crate is pure IO: it holds no application state of its own.

#![forbid(unsafe_code)]

pub mod actor;
pub mod backoff;
pub mod client;
pub mod color;
pub mod error;
pub mod mapping;
pub mod model;
pub mod ocs;
pub mod poll;
pub mod push_exec;

pub use actor::{SyncActorConfig, spawn_sync_actor};
pub use backoff::BackoffPolicy;
pub use client::{
    BoardChanges, CloneOptions, DeckClient, Fetch, LabelChanges, NewCard, RetrySleep, StackChanges,
    TokioSleep, Validators,
};
pub use color::{DeckColor, ParseColorError};
pub use error::DeckError;
pub use model::{
    Attachment, Board, BoardPermissions, Card, CardLabel, ExtendedData, Label, Participant, Stack,
    StackFilter,
};
pub use poll::poll_backoff;
pub use push_exec::{BindingOverlay, GroupOutcome, PushExecutor};
