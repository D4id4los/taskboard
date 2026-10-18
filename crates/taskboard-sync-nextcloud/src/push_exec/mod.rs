// SPDX-License-Identifier: MIT OR Apache-2.0
//! The push group executor: turns one `PushGroup` into client calls,
//! resolving bindings through an in-cycle overlay and classifying every
//! outcome (phase 4 plan §5.3).
//!
//! The overlay is seeded from the read state's bindings and extended as
//! create echoes return — never re-derived by search (decision 7). Every
//! op that touches an existing bound card fetches the card first
//! (fetch-before-write, decision 5): the round-trip PUT is built from the
//! fetched card, and a fetch 404 re-resolves the card once against the
//! fresh listings before classifying — a stale binding (the card moved
//! remotely) rebinds and pushes at the fresh location instead of being
//! misread as `RemoteMissing` (issue #258). The delete path re-resolves
//! the same way: a delete 404 is retried once at the fresh binding, and
//! only a 404 that survives the re-resolve converges `RemoteMissing`
//! (issue #319). The label-delta and delete arms emit one outcome per
//! subsumed op, so a partially-failed update no longer reports lies
//! (issue #314).

mod execute;
mod outcomes;
mod overlay;

pub use execute::PushExecutor;
pub use outcomes::GroupOutcome;
pub use overlay::BindingOverlay;

#[cfg(test)]
mod tests;
