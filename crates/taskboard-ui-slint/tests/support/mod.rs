// SPDX-License-Identifier: AGPL-3.0-only

//! The journey-harness support modules (chunk 4): each test binary
//! declares `mod support;` and pulls in only what it drives. The
//! modules compile per test binary and are invisible to the shipped
//! library, and each binary drives its own subset of the shared
//! surface — the `dead_code` allow below covers that per-binary rest,
//! not never-used helpers.

#![allow(dead_code)]

pub mod driver;
pub mod fake_engine;
pub mod fixture;
pub mod golden_compare;
pub mod guard;
