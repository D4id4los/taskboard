// SPDX-License-Identifier: MIT OR Apache-2.0
//! The files-sync substrate's pure core (ADR 0020): the tree-layout
//! types, the tolerant per-user document codecs, the merge fold, the
//! state hash, and the heartbeat payload types.
//!
//! Substrate contract in one screen (design D-SF1–D-SF5, decisions
//! D-FC1…D-FC10 in the chunk plan):
//!
//! - **Per-device path ownership.** Each device publishes only inside
//!   its own `devices/<device-id>/` subtree and reads everything else.
//!   The tree's file taxonomy lives in [`layout`]; reserved kinds
//!   (`ops/*.json`, `snapshot-*.json`) are tolerated opaque — this
//!   crate defines no payload type for them, so "never parse, never
//!   delete, never choke" holds by construction.
//! - **Current-state v1.** A device's `appdata.json` carries its whole
//!   current view (its own rows and everything it has folded from
//!   peers): re-publication is how offline peers' data survives a
//!   device's death, and the fold is idempotent, so this is safe.
//! - **Tolerant relay** ([`document`]). Unknown envelope keys,
//!   unknown/reserved sections, unknown body keys, and unmodelable
//!   rows are *carried* verbatim — a device re-publishes its whole
//!   view, so anything it drops is lost to the fleet on that device's
//!   death. Reserved section names (`settings`, `proofs`) are relayed
//!   when seen and never emitted.
//! - **A total, input-order-free fold** ([`fold`]): per entry key the
//!   winner is the max over `(rank, clock, author, content)` — no
//!   input permutation can change any winner, which is what makes the
//!   commutativity/idempotence/associativity laws hold by
//!   construction (they are proptested as the fold's contract).
//! - **A tagged state hash** ([`hash`]) over the canonicalised merged
//!   document, so heartbeat hashes and published files agree by
//!   construction.
//!
//! Crate invariant: **zero IO** — no filesystem, no network, no async
//! runtime, no UI dependency. Everything here is a pure function over
//! data; the transport crate (a separate member) owns every side
//! effect. [`ids::DeviceId::mint`] is the only nondeterminism,
//! confined to identity creation.
//!
//! Module map: identifiers ([`ids`]), the tree taxonomy ([`layout`]),
//! identity payloads ([`identity`]), the heartbeat payload
//! ([`heartbeat`]), the per-user document codec ([`document`]), the
//! merge fold ([`fold`]), the state hash ([`hash`]).

#![forbid(unsafe_code)]

pub mod document;
pub mod fold;
pub mod hash;
pub mod heartbeat;
pub mod identity;
pub mod ids;
pub mod layout;

/// Compiled for our own tests (dev-dep `proptest`) and for downstream
/// crates that enable the `test-support` feature (the engine tests of
/// the seam chunk consume the document strategies).
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
