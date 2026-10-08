// SPDX-License-Identifier: MIT OR Apache-2.0
//! Nextcloud synchronization actor for `taskboard`.
//!
//! Implements the remote side of the offline-first sync strategy: pulls and
//! pushes tasks via the Nextcloud WebDAV/REST APIs without ever blocking the
//! state engine or the UI. Network failures are typed (via `thiserror`) and
//! reported to the state engine over channels so the UI can surface sync
//! status instead of failing.
//!
//! This crate is pure IO: it holds no application state of its own.

#![forbid(unsafe_code)]

pub mod backoff;
pub mod client;
pub mod error;
pub mod ocs;

pub use backoff::BackoffPolicy;
pub use client::{DeckClient, RetrySleep, TokioSleep};
pub use error::DeckError;
pub use ocs::Board;
