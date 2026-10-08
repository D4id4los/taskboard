// SPDX-License-Identifier: MIT OR Apache-2.0
//! Typed Deck OCS REST client.
//!
//! Read surface for every Deck resource (`docs/testing_strategy.org` §8),
//! plus the boards write slice: envelope decoding, typed errors, and bounded
//! retries. The write surface for stacks/cards/labels grows here next.

use std::future::Future;
use std::time::Duration;

use reqwest::{Client, Method, StatusCode, Url, header::ACCEPT};
use serde::Serialize;

use crate::backoff::BackoffPolicy;
use crate::color::DeckColor;
use crate::error::DeckError;
use crate::model::{Attachment, Board, Card, Label, Stack, StackFilter};
use crate::ocs::OcsEnvelope;

const DECK_API_PATH: &str = "index.php/apps/deck/api/v1.0";

/// Seam for retry backoff waiting.
///
/// Production uses tokio timers ([`TokioSleep`]); tests inject a recording
/// or instant sleeper so retry-timing assertions are exact regardless of
/// real socket behavior.
pub trait RetrySleep: Send + Sync + std::fmt::Debug {
    fn sleep(&self, delay: Duration) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>>;
}

/// The production sleeper: `tokio::time::sleep`.
#[derive(Debug, Clone, Copy)]
pub struct TokioSleep;

impl RetrySleep for TokioSleep {
    fn sleep(&self, delay: Duration) -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(tokio::time::sleep(delay))
    }
}

/// Minimal REST client for the Nextcloud Deck OCS API.
///
/// Requests carry Basic auth (`user:app_password`), the `OCS-APIRequest`
/// header Nextcloud requires, and `Accept: application/json`. Retries on
/// `429`, `503`, and transport errors using [`BackoffPolicy`].
#[derive(Debug, Clone)]
pub struct DeckClient {
    http: Client,
    base_url: Url,
    credentials: String,
    backoff: BackoffPolicy,
    sleeper: std::sync::Arc<dyn RetrySleep>,
}

#[derive(Serialize)]
struct CreateBoardBody<'a> {
    title: &'a str,
    color: &'a DeckColor,
}

impl DeckClient {
    /// Builds a client for `{base_url}/index.php/apps/deck/api/v1.0`.
    ///
    /// `base_url` must be an absolute http(s) URL pointing at the Nextcloud
    /// root (a trailing slash is tolerated). Authentication is Nextcloud
    /// Basic auth: the login user plus an *app password*.
    ///
    /// # Errors
    ///
    /// Returns [`DeckError::InvalidBaseUrl`] when `base_url` is not a usable
    /// http(s) URL; [`DeckError::Transport`] when the HTTP client itself
    /// cannot be built.
    pub fn new(base_url: &str, user: &str, token: &str) -> Result<Self, DeckError> {
        let mut url = Url::parse(base_url).map_err(|_| DeckError::InvalidBaseUrl)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(DeckError::InvalidBaseUrl);
        }
        if url.host_str().is_none_or(str::is_empty) {
            return Err(DeckError::InvalidBaseUrl);
        }
        url.set_query(None);
        url.set_fragment(None);
        let path = url.path().trim_end_matches('/').to_owned();
        if path.is_empty() {
            url.set_path("/");
        } else {
            url.set_path(&path);
        }

        Ok(Self {
            http: Client::builder().build()?,
            base_url: url,
            credentials: format!("{user}:{token}"),
            backoff: BackoffPolicy::new(),
            sleeper: std::sync::Arc::new(TokioSleep),
        })
    }

    /// Overrides the retry-backoff sleeper (test seam).
    #[must_use]
    pub fn with_sleeper(mut self, sleeper: std::sync::Arc<dyn RetrySleep>) -> Self {
        self.sleeper = sleeper;
        self
    }

    /// Lists all boards visible to the account.
    ///
    /// # Errors
    ///
    /// See [`DeckError`]: typed outcomes for auth, HTTP, envelope, and
    /// (after retries) transport/rate-limit failures.
    pub async fn boards(&self) -> Result<Vec<Board>, DeckError> {
        tracing::debug!("listing deck boards");
        let result: Result<Vec<Board>, DeckError> = self
            .send_json(Method::GET, "boards", None::<serde_json::Value>)
            .await;
        match &result {
            Ok(boards) => tracing::info!(count = boards.len(), "deck boards listed"),
            Err(err) => tracing::warn!(error = %err, "listing deck boards failed"),
        }
        result
    }

    /// Fetches a single board by id.
    ///
    /// # Errors
    ///
    /// See [`DeckError`].
    pub async fn board(&self, id: u64) -> Result<Board, DeckError> {
        tracing::debug!(board_id = id, "fetching deck board");
        let result: Result<Board, DeckError> = self
            .send_json(
                Method::GET,
                &format!("boards/{id}"),
                None::<serde_json::Value>,
            )
            .await;
        match &result {
            Ok(_) => tracing::info!(board_id = id, "deck board fetched"),
            Err(err) => tracing::warn!(board_id = id, error = %err, "fetching deck board failed"),
        }
        result
    }

    /// Creates a board with a six-digit hex [`DeckColor`].
    ///
    /// # Errors
    ///
    /// See [`DeckError`].
    pub async fn create_board(&self, title: &str, color: &DeckColor) -> Result<Board, DeckError> {
        tracing::debug!(title, color = %color, "creating deck board");
        let result: Result<Board, DeckError> = self
            .send_json(
                Method::POST,
                "boards",
                Some(CreateBoardBody { title, color }),
            )
            .await;
        match &result {
            Ok(board) => tracing::info!(board_id = board.id, "deck board created"),
            Err(err) => tracing::warn!(error = %err, "creating deck board failed"),
        }
        result
    }

    /// Deletes a board by id.
    ///
    /// Returns the deleted board as the server reports it. Deck's DELETE is
    /// a *soft* delete: the response (authoritative, unlike the listing,
    /// which can lag behind Deck's board cache) carries a non-zero
    /// `Board::deleted_at`.
    ///
    /// # Errors
    ///
    /// See [`DeckError`].
    pub async fn delete_board(&self, id: u64) -> Result<Board, DeckError> {
        tracing::debug!(board_id = id, "deleting deck board");
        let result: Result<Board, DeckError> = self
            .send_json(
                Method::DELETE,
                &format!("boards/{id}"),
                None::<serde_json::Value>,
            )
            .await;
        match &result {
            Ok(board) => tracing::info!(
                board_id = id,
                deleted_at = board.deleted_at,
                "deck board deleted"
            ),
            Err(err) => tracing::warn!(board_id = id, error = %err, "deleting deck board failed"),
        }
        result
    }

    /// Lists a board's stacks with their nested cards.
    ///
    /// # Errors
    ///
    /// See [`DeckError`].
    pub async fn stacks(
        &self,
        board_id: u64,
        filter: StackFilter,
    ) -> Result<Vec<Stack>, DeckError> {
        tracing::debug!(board_id, ?filter, "listing deck stacks");
        let resource = format!("boards/{board_id}/{}", filter.path_segment());
        let result: Result<Vec<Stack>, DeckError> = self
            .send_json(Method::GET, &resource, None::<serde_json::Value>)
            .await;
        match &result {
            Ok(stacks) => tracing::info!(count = stacks.len(), "deck stacks listed"),
            Err(err) => tracing::warn!(error = %err, "listing deck stacks failed"),
        }
        result
    }

    /// Fetches a single stack (with its cards).
    ///
    /// # Errors
    ///
    /// See [`DeckError`].
    pub async fn stack(&self, board_id: u64, stack_id: u64) -> Result<Stack, DeckError> {
        tracing::debug!(board_id, stack_id, "fetching deck stack");
        let resource = format!("boards/{board_id}/stacks/{stack_id}");
        let result: Result<Stack, DeckError> = self
            .send_json(Method::GET, &resource, None::<serde_json::Value>)
            .await;
        match &result {
            Ok(_) => tracing::info!(stack_id, "deck stack fetched"),
            Err(err) => tracing::warn!(error = %err, "fetching deck stack failed"),
        }
        result
    }

    /// Fetches a single card.
    ///
    /// # Errors
    ///
    /// See [`DeckError`].
    pub async fn card(
        &self,
        board_id: u64,
        stack_id: u64,
        card_id: u64,
    ) -> Result<Card, DeckError> {
        tracing::debug!(board_id, stack_id, card_id, "fetching deck card");
        let resource = format!("boards/{board_id}/stacks/{stack_id}/cards/{card_id}");
        let result: Result<Card, DeckError> = self
            .send_json(Method::GET, &resource, None::<serde_json::Value>)
            .await;
        match &result {
            Ok(_) => tracing::info!(card_id, "deck card fetched"),
            Err(err) => tracing::warn!(error = %err, "fetching deck card failed"),
        }
        result
    }

    /// Lists a board's labels.
    ///
    /// # Errors
    ///
    /// See [`DeckError`].
    pub async fn labels(&self, board_id: u64) -> Result<Vec<Label>, DeckError> {
        tracing::debug!(board_id, "listing deck labels");
        let resource = format!("boards/{board_id}/labels");
        let result: Result<Vec<Label>, DeckError> = self
            .send_json(Method::GET, &resource, None::<serde_json::Value>)
            .await;
        match &result {
            Ok(labels) => tracing::info!(count = labels.len(), "deck labels listed"),
            Err(err) => tracing::warn!(error = %err, "listing deck labels failed"),
        }
        result
    }

    /// Lists a card's attachment metadata (content download is out of
    /// scope).
    ///
    /// # Errors
    ///
    /// See [`DeckError`].
    pub async fn attachments(
        &self,
        board_id: u64,
        stack_id: u64,
        card_id: u64,
    ) -> Result<Vec<Attachment>, DeckError> {
        tracing::debug!(board_id, stack_id, card_id, "listing deck attachments");
        let resource = format!("boards/{board_id}/stacks/{stack_id}/cards/{card_id}/attachments");
        let result: Result<Vec<Attachment>, DeckError> = self
            .send_json(Method::GET, &resource, None::<serde_json::Value>)
            .await;
        match &result {
            Ok(attachments) => tracing::info!(count = attachments.len(), "deck attachments listed"),
            Err(err) => tracing::warn!(error = %err, "listing deck attachments failed"),
        }
        result
    }

    /// Performs the request (with retries) and decodes the OCS envelope.
    async fn send_json<T: serde::de::DeserializeOwned>(
        &self,
        method: Method,
        resource: &str,
        body: Option<impl Serialize>,
    ) -> Result<T, DeckError> {
        let mut retry_index = 0u32;
        loop {
            let outcome = self.attempt(&method, resource, body.as_ref()).await;
            match outcome {
                Ok(bytes) => return decode_envelope(&bytes),
                Err(
                    retryable @ (DeckError::RateLimited
                    | DeckError::Unavailable
                    | DeckError::Transport(_)),
                ) => {
                    let Some(delay) = self.backoff.delay(retry_index) else {
                        return Err(retryable);
                    };
                    tracing::warn!(retry_index, ?delay, resource, "retryable failure");
                    self.sleeper.sleep(delay).await;
                    retry_index += 1;
                }
                Err(other) => return Err(other),
            }
        }
    }

    /// One request/response round trip; maps statuses to typed outcomes.
    async fn attempt(
        &self,
        method: &Method,
        resource: &str,
        body: Option<&impl Serialize>,
    ) -> Result<Vec<u8>, DeckError> {
        let url = self
            .base_url
            .join(&format!("{DECK_API_PATH}/{resource}"))
            .map_err(|_| DeckError::InvalidBaseUrl)?;

        let mut request = self
            .http
            .request(method.clone(), url)
            .header("OCS-APIRequest", "true")
            .header(ACCEPT, "application/json")
            .basic_auth(self.credentials_user(), Some(self.credentials_token()));
        if let Some(body) = body {
            request = request.json(body);
        }

        let response = request.send().await?;
        match response.status() {
            StatusCode::UNAUTHORIZED => return Err(DeckError::Unauthorized),
            StatusCode::FORBIDDEN => return Err(DeckError::Forbidden),
            StatusCode::NOT_FOUND => return Err(DeckError::NotFound),
            StatusCode::BAD_REQUEST => return Err(DeckError::BadRequest),
            StatusCode::CONFLICT => return Err(DeckError::Conflict),
            StatusCode::PRECONDITION_FAILED => return Err(DeckError::PreconditionFailed),
            StatusCode::TOO_MANY_REQUESTS => return Err(DeckError::RateLimited),
            StatusCode::SERVICE_UNAVAILABLE => return Err(DeckError::Unavailable),
            status if status.is_server_error() => return Err(DeckError::Server(status.as_u16())),
            _ => {}
        }
        Ok(response.bytes().await?.to_vec())
    }

    /// Splits the stored `user:token` back apart for `basic_auth`.
    /// Splitting at the *first* `:` is correct because Nextcloud app
    /// passwords never contain colons.
    fn credentials_user(&self) -> &str {
        self.credentials
            .split_once(':')
            .map_or(&self.credentials, |(u, _)| u)
    }

    fn credentials_token(&self) -> &str {
        self.credentials.split_once(':').map_or("", |(_, t)| t)
    }
}

/// Decodes a Deck API response payload.
///
/// Tolerates both response shapes in the wild: older servers always wrap in
/// the OCS envelope; newer Deck releases return the bare payload for
/// `Accept: application/json` (verified against Nextcloud 35).
pub(crate) fn decode_envelope<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
) -> Result<T, DeckError> {
    if let Ok(envelope) = serde_json::from_slice::<OcsEnvelope<T>>(bytes) {
        return Ok(envelope.ocs.data);
    }
    Ok(serde_json::from_slice(bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err_of(base: &str) -> Result<DeckClient, DeckError> {
        DeckClient::new(base, "user", "token")
    }

    #[test]
    fn accepts_clean_base_url() {
        assert!(err_of("https://cloud.example.com").is_ok());
    }

    #[test]
    fn normalizes_trailing_slash_and_strips_path_query_fragment() {
        let client =
            DeckClient::new("https://cloud.example.com/nextcloud/?utm=x#frag", "u", "t").unwrap();
        assert_eq!(
            client.base_url.as_str(),
            "https://cloud.example.com/nextcloud"
        );
    }

    #[test]
    fn rejects_missing_scheme_host_and_bad_scheme() {
        assert!(matches!(
            err_of("cloud.example.com"),
            Err(DeckError::InvalidBaseUrl)
        ));
        assert!(matches!(err_of("https://"), Err(DeckError::InvalidBaseUrl)));
        assert!(matches!(
            err_of("ftp://cloud.example.com"),
            Err(DeckError::InvalidBaseUrl)
        ));
    }

    #[test]
    fn envelope_decode_tolerates_unknown_fields() {
        #[derive(Debug, serde::Deserialize, PartialEq)]
        struct Payload {
            v: u32,
        }
        let bytes =
            br#"{"ocs":{"meta":{"statuscode":200,"unknown":"x"},"data":{"v":7,"extra":[1]}}}"#;
        let decoded: Payload = decode_envelope(bytes).unwrap();
        assert_eq!(decoded, Payload { v: 7 });
    }

    #[test]
    fn envelope_decode_rejects_malformed() {
        let res: Result<serde_json::Value, _> = decode_envelope(br#"{"ocs": {"meta": {}"#);
        assert!(matches!(res, Err(DeckError::Envelope(_))));
    }

    #[test]
    fn bare_payload_without_envelope_decodes() {
        // Nextcloud 35 returns a bare array for Accept: application/json.
        let decoded: Vec<Board> =
            decode_envelope(br#"[{"id": 3, "title": "t", "color": "5c2751"}]"#).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].id, 3);
    }
}
