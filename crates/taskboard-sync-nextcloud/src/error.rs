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
    /// Rate limited (HTTP 429); retries exhausted.
    #[error("rate limited")]
    RateLimited,
    /// A non-retryable 5xx server error carrying the status code.
    #[error("server error (status {0})")]
    Server(u16),
    /// Service unavailable (HTTP 503); retries exhausted.
    #[error("service unavailable")]
    Unavailable,
}
