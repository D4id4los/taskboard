// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed errors for the Nextcloud Deck client.

use thiserror::Error;

/// Errors surfaced by [`crate::client::DeckClient`].
///
/// Retryable conditions (`429`, `503`, transport failures) are reported only
/// after the [`crate::backoff::BackoffPolicy`] budget is exhausted.
#[derive(Debug, Error)]
pub enum DeckError {
    /// The configured server base URL is not a usable http(s) URL.
    #[error("invalid Nextcloud base URL")]
    InvalidBaseUrl,
    /// An attachment upload payload failed local validation: the client
    /// filename is empty or the MIME type is unparseable. Raised before any
    /// request is sent.
    #[error("invalid attachment upload")]
    InvalidUpload,
    /// Network-level failure (connection refused, DNS, TLS, timeout).
    #[error("transport failure")]
    Transport(#[from] reqwest::Error),
    /// The response body could not be decoded as the expected OCS JSON.
    #[error("unexpected response payload")]
    Envelope(#[from] serde_json::Error),
    /// Authentication failed (HTTP 401).
    #[error("authentication failed")]
    Unauthorized,
    /// The account lacks access to the requested resource (HTTP 403).
    #[error("access forbidden")]
    Forbidden,
    /// The requested resource does not exist (HTTP 404).
    #[error("resource not found")]
    NotFound,
    /// The request was rejected as invalid (HTTP 400), e.g. a title longer
    /// than the server's limit.
    #[error("invalid request")]
    BadRequest,
    /// The request conflicts with server state (HTTP 409).
    #[error("conflict with server state")]
    Conflict,
    /// A precondition on the request failed (HTTP 412).
    #[error("precondition failed")]
    PreconditionFailed,
    /// Rate limited (HTTP 429); retries exhausted. Carries the server's
    /// `Retry-After` delay-seconds hint when present and parseable —
    /// `None` when absent, malformed, or in the HTTP-date form.
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

impl DeckError {
    /// Whether this is a network-level retryable failure — the
    /// `Transport` / `RateLimited` / `Unavailable` triple. The retry
    /// loop and the push executor's abort checks key off this
    /// membership. The `mapping.rs` classifiers deliberately do not:
    /// their matches must stay compiler-exhaustive, so a fifth
    /// retryable class edits here *and* is forced into both
    /// classifiers' arms by the build.
    pub(crate) fn is_transport(&self) -> bool {
        matches!(
            self,
            Self::Transport(_) | Self::RateLimited { .. } | Self::Unavailable
        )
    }
}
