// SPDX-License-Identifier: AGPL-3.0-only
//! Slint presentation layer for `taskboard`.
//!
//! The UI is a "dumb presentation client": it reads state via lock-free
//! `arc_swap::ArcSwap<AppState>` loads, maps fields onto widgets, and emits
//! non-blocking [`tokio::sync::mpsc`] commands on user interaction.
//! Business logic and persistence are forbidden here.
//!
//! NOTE: `#![forbid(unsafe_code)]` is intentionally *not* set in this crate:
//! Slint's generated rendering code requires `unsafe`. This is the only
//! workspace crate with that exception (see `AGENTS.md` and the ADR on
//! toolchain & quality gates).
