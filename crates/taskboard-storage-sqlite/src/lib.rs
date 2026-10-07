// SPDX-License-Identifier: MIT OR Apache-2.0
//! Local `SQLite` persistence actor for `taskboard`.
//!
//! Ensures the application boots instantly with cached data and keeps
//! operating when network connectivity is lost (kiosk mode). All disk IO
//! happens on background Tokio tasks; the state engine commands this actor
//! exclusively over [`tokio::sync::mpsc`] write channels.
//!
//! SQL queries are compile-time checked against the local schema via
//! `sqlx`'s `query!` macros.

#![forbid(unsafe_code)]
