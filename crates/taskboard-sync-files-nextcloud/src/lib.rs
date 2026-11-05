// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Nextcloud `WebDAV` files transport (design D-SF6–D-SF8, decisions
//! D-FT1…D-FT12 in the chunk plan): one poll-cycle actor over the
//! files-sync substrate's pure core.
//!
//! The transport contract in one screen:
//!
//! - **Conditional-PUT discipline.** Create-once files (`identity.json`,
//!   the MKCOL ladder) go through `If-None-Match: *`; a `412` means
//!   someone else won the create race and the loser *adopts* the served
//!   file (F3). Single-writer rewrites (`appdata.json`, `info.json`)
//!   carry `If-Match` of the last-known [`Etag`], **echoed verbatim as
//!   served** (F4) — never trimmed, never re-quoted.
//! - **An unexpected `412` on an own single-writer path is the fork
//!   alarm** (D-SF4): the path joins the stop-write set, reads continue
//!   forever, the finding surfaces loudly (typed status + `error!` log).
//!   Create-once `412`s are adoption arms, never forks.
//! - **Etags are version tokens, never content identities** (F5): they
//!   churn on every upload, so the skip-PUT decision compares a *local*
//!   `StateHash`; etags only drive PROPFIND change detection and the
//!   `If-Match` guards.
//! - **Readers never delete.** The [`FilesTransport`] verb set is
//!   closed — `MKCOL`, `PROPFIND`, `GET`, conditional `PUT`. No
//!   `DELETE`, no `MOVE`: no code path can remove or relocate a file
//!   (the trashbin is the user's recovery, F8).
//! - **The actor is not a network authority.** It subscribes to the
//!   system broadcast only for `Shutdown` and publishes nothing on it;
//!   counting network episodes is `taskboard-app`'s authority's job.
//!
//! Module map: typed errors ([`error`]), the verbatim etag newtype and
//! the in-memory etag store ([`etag`]), the transport seam
//! ([`transport`]).

#![forbid(unsafe_code)]

pub mod client;
pub mod error;
pub mod etag;
pub mod extract;
pub mod transport;
