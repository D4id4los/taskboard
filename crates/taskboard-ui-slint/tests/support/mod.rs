// SPDX-License-Identifier: AGPL-3.0-only

//! The journey-harness support modules (chunk 4): each test binary
//! declares `mod support;` and pulls in only what it drives. The
//! modules compile per test binary and are invisible to the shipped
//! library.

pub mod driver;
pub mod golden_compare;
pub mod guard;
