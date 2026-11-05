// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed errors for the `WebDAV` files transport.
//!
//! Mirrors `taskboard-sync-nextcloud`'s [`DeckError`] table: every
//! surprise is a typed outcome, never a crash (the hosted-tier drift
//! posture, characterization F1). Retryable conditions (`429`, `503`,
//! transport failures) are reported only after the retry budget is
//! exhausted; precondition failures (`412`) are *results*, never
//! retried.

use thiserror::Error;

/// Errors surfaced by the `WebDAV` files client.
#[derive(Debug, Error)]
pub enum TransportError {
    /// The configured server base URL is not a usable http(s) URL.
    #[error("invalid Nextcloud base URL")]
    InvalidBaseUrl,
    /// Network-level failure (connection refused, DNS, TLS, timeout).
    #[error("transport failure")]
    Transport(#[from] reqwest::Error),
    /// A JSON failure in either direction (the document codec over a
    /// GET/PUT body).
    #[error("json encode/decode failure")]
    Json(#[from] serde_json::Error),
    /// Authentication failed (HTTP 401).
    #[error("authentication failed")]
    Unauthorized,
    /// The account lacks access to the requested resource (HTTP 403).
    #[error("access forbidden")]
    Forbidden,
    /// The requested resource does not exist (HTTP 404).
    #[error("resource not found")]
    NotFound,
    /// The request was rejected as invalid (HTTP 400).
    #[error("invalid request")]
    BadRequest,
    /// The request conflicts with server state (HTTP 409), e.g. an
    /// MKCOL whose parent collection is missing.
    #[error("conflict with server state")]
    Conflict,
    /// A precondition on the request failed (HTTP 412): the create-once
    /// guard (`If-None-Match: *`) lost a race, or the lost-update guard
    /// (`If-Match`) found a different version — including a *missing*
    /// file, which RFC 4918 also answers 412. A *result* the cycle
    /// interprets (adoption / fork alarm), never a retryable error.
    #[error("precondition failed")]
    PreconditionFailed,
    /// Rate limited (HTTP 429); retries exhausted. Carries the server's
    /// `Retry-After` delay-seconds hint when present and parseable.
    #[error("rate limited")]
    RateLimited {
        /// Advertised wait before the next attempt, if the server sent one.
        retry_after: Option<std::time::Duration>,
    },
    /// A non-retryable 5xx server error carrying the status code.
    #[error("server error (status {0})")]
    Server(u16),
    /// Service unavailable (HTTP 503); retries exhausted.
    #[error("service unavailable")]
    Unavailable,
}

impl TransportError {
    /// Whether this is a network-level retryable failure — the
    /// `Transport` / `RateLimited` / `Unavailable` triple. The client's
    /// retry loop and the actor's failure-streak classification key off
    /// this membership (the `DeckError::is_transport` contract).
    #[must_use]
    pub fn is_transport(&self) -> bool {
        matches!(
            self,
            Self::Transport(_) | Self::RateLimited { .. } | Self::Unavailable
        )
    }
}
