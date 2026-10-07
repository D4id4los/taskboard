// SPDX-License-Identifier: MIT OR Apache-2.0
//! The `taskboard` State Engine actor.
//!
//! Single source of runtime truth. The engine consumes [`tokio::sync::mpsc`]
//! commands, mutates in-memory state, publishes immutable snapshots through
//! `arc_swap::ArcSwap<AppState>`, and emits
//! [`tokio::sync::broadcast`] signals to prompt UI repaints.
//!
//! Boundary rules (see `AGENTS.md`):
//! - Owns the *write* side of `ArcSwap<AppState>`; readers never mutate.
//! - Communicates with other actors exclusively via typed channels.
//! - Tested as a black box: drive commands through channels, snapshot the
//!   resulting `AppState` with `insta`.

#![forbid(unsafe_code)]
