// SPDX-License-Identifier: MIT OR Apache-2.0

//! The wiremock `WebDAV` client integration family (#1087, plan §3.4):
//! the client request facts and the retained-etag suite. Members are
//! verbatim moves under `tests/suite/` (subdirectories are not
//! auto-discovered as targets); zero test renames — the only
//! nextest-string delta is the inserted member-module segment. The
//! `#[ignore]` tier-2/3 binaries (`it_docker`, `it_live`) stay
//! standalone and keep their own `common` harness declaration — the
//! members here do not consume it, so this root declares none.

#[path = "suite/client_wiremock.rs"]
mod client_wiremock;
#[path = "suite/retained_etag_wiremock.rs"]
mod retained_etag_wiremock;
#[path = "suite/timeout_wiremock.rs"]
mod timeout_wiremock;
