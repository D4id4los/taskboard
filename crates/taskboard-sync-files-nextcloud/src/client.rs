// SPDX-License-Identifier: MIT OR Apache-2.0
//! The production `WebDAV` files transport over the workspace reqwest
//! stack (decision D-FT2), mirroring `DeckClient`'s shape: Basic auth,
//! bounded retries on the transport class only (`429`/`503`/transport
//! failures, `Retry-After` floor), typed status mapping, and a
//! credential-redacting `Debug`. Preconditions (`412`) are results,
//! never retried.
//!
//! The request path space: the account's DAV file home
//! (`<server>/remote.php/dav/files/<username>/`) plus the substrate
//! root (`taskboard/`) plus the substrate-relative path. Substrate
//! paths are UUIDs and fixed names — already URL-safe; the username
//! is percent-encoded by `Url::set_path` when it joins, nothing else
//! encodes.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Method, StatusCode, Url, header};
use taskboard_sync_files::layout::ROOT_FOLDER_NAME;

use crate::error::TransportError;
use crate::etag::Etag;
use crate::extract::{PROPFIND_DEPTH1_BODY, extract_207};
use crate::transport::{FileContent, FilesTransport, MkcolOutcome, PropfindListing, PutOutcome};

const DAV_FILES_PATH: &str = "remote.php/dav/files";

/// The `WebDAV` extension methods, parsed once at each call site
/// (infallible for these fixed literals; `http` has no `const`
/// constructor).
fn mkcol_method() -> Method {
    Method::from_bytes(b"MKCOL").expect("MKCOL is a valid method")
}

fn propfind_method() -> Method {
    Method::from_bytes(b"PROPFIND").expect("PROPFIND is a valid method")
}
const XML_CONTENT_TYPE: &str = "application/xml";

/// Seam for retry-backoff waiting (the `DeckClient::RetrySleep`
/// contract; production uses tokio timers, tests inject an instant
/// sleeper so retry-timing assertions are exact under paused time).
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

/// Bounded retry policy: 3 attempts total, delays doubling from
/// 500 ms, capped at 8 s (the Deck client's budget; the policy's own
/// laws stay law-tested there — this crate consumes the shape, it does
/// not re-test it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffPolicy {
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
}

impl BackoffPolicy {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(8),
        }
    }

    /// Delay before the retry with the given zero-based index; `None`
    /// once the budget is exhausted.
    #[must_use]
    pub fn delay(&self, retry_index: u32) -> Option<Duration> {
        if retry_index + 1 >= self.max_attempts {
            return None;
        }
        let mut d = self.initial_delay;
        for _ in 0..retry_index {
            d = d
                .checked_mul(2)
                .filter(|doubled| *doubled <= self.max_delay)
                .unwrap_or(self.max_delay);
        }
        Some(d)
    }
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self::new()
    }
}

/// Minimal `WebDAV` client over the account's DAV file home.
#[derive(Clone)]
pub struct WebDavFilesTransport {
    http: Client,
    base_url: Url,
    credentials: String,
    backoff: BackoffPolicy,
    sleeper: Arc<dyn RetrySleep>,
}

impl std::fmt::Debug for WebDavFilesTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The credentials field *is* the secret (`user:app-password`);
        // a `tracing::debug!(?transport)` must never leak it (ADR 0008).
        f.debug_struct("WebDavFilesTransport")
            .field("http", &self.http)
            .field("base_url", &self.base_url)
            .field("credentials", &"[redacted]")
            .field("backoff", &self.backoff)
            .field("sleeper", &self.sleeper)
            .finish_non_exhaustive()
    }
}

/// One raw response for the caller's verb to interpret.
struct Raw {
    status: StatusCode,
    /// `Retry-After` header, parsed when in the delay-seconds form.
    retry_after: Option<Duration>,
    etag: Option<Etag>,
    body: Vec<u8>,
}

/// Parses a `Retry-After` header in the delay-seconds form: strict
/// ASCII digits only (the Deck client's rule; the HTTP-date form reads
/// as absent).
fn parse_retry_after(header: &str) -> Option<Duration> {
    if header.is_empty() || !header.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    header.parse::<u64>().ok().map(Duration::from_secs)
}

/// Maps an HTTP status to a typed [`TransportError`]; `Ok(())` means
/// "the verb interprets this one" (201/204/207/405/412/404 are verb
/// outcomes).
fn map_status(raw: &Raw) -> Result<(), TransportError> {
    match raw.status {
        StatusCode::UNAUTHORIZED => Err(TransportError::Unauthorized),
        StatusCode::FORBIDDEN => Err(TransportError::Forbidden),
        StatusCode::NOT_FOUND => Err(TransportError::NotFound),
        StatusCode::BAD_REQUEST => Err(TransportError::BadRequest),
        StatusCode::CONFLICT => Err(TransportError::Conflict),
        StatusCode::TOO_MANY_REQUESTS => Err(TransportError::RateLimited {
            retry_after: raw.retry_after,
        }),
        StatusCode::SERVICE_UNAVAILABLE => Err(TransportError::Unavailable),
        s if s.is_server_error() => Err(TransportError::Server(s.as_u16())),
        _ => Ok(()),
    }
}

impl WebDavFilesTransport {
    /// Builds the transport over `{server}/remote.php/dav/files/{user}/`.
    ///
    /// # Errors
    ///
    /// [`TransportError::InvalidBaseUrl`] when the server URL is not a
    /// usable http(s) URL; [`TransportError::Transport`] when the HTTP
    /// client itself cannot be built.
    pub fn new(base_url: &str, user: &str, token: &str) -> Result<Self, TransportError> {
        let mut url = Url::parse(base_url).map_err(|_| TransportError::InvalidBaseUrl)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(TransportError::InvalidBaseUrl);
        }
        if url.host_str().is_none_or(str::is_empty) {
            return Err(TransportError::InvalidBaseUrl);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(TransportError::InvalidBaseUrl);
        }
        url.set_query(None);
        url.set_fragment(None);
        // `set_path` percent-encodes the username the same way the
        // server expects it in the DAV home path. The substrate root
        // (`taskboard/`) is *not* part of the base: paths handed to the
        // verbs are root-inclusive substrate paths.
        url.set_path(&format!("/{DAV_FILES_PATH}/{user}/"));
        Ok(Self {
            http: Client::builder().build()?,
            base_url: url,
            credentials: format!("{user}:{token}"),
            backoff: BackoffPolicy::new(),
            sleeper: Arc::new(TokioSleep),
        })
    }

    /// Overrides the retry-backoff sleeper (test seam).
    #[must_use]
    pub fn with_sleeper(mut self, sleeper: Arc<dyn RetrySleep>) -> Self {
        self.sleeper = sleeper;
        self
    }

    /// Joins a root-inclusive substrate path onto the DAV home
    /// (`taskboard/…`; the MKCOL ladder names the root itself).
    fn url_for(&self, relative_path: &str) -> Result<Url, TransportError> {
        self.base_url
            .join(relative_path)
            .map_err(|_| TransportError::InvalidBaseUrl)
    }

    /// Sends one request (with retries on the transport class) and
    /// hands the raw response to `handle`.
    async fn send_with_retries<T>(
        &self,
        method: Method,
        url: Url,
        headers: impl Fn() -> Vec<(&'static str, String)>,
        body: Option<&[u8]>,
        handle: impl Fn(Raw) -> Result<T, TransportError>,
    ) -> Result<T, TransportError> {
        let mut retry_index = 0u32;
        loop {
            let outcome = self
                .attempt(method.clone(), url.clone(), &headers, body)
                .await
                .and_then(&handle);
            match outcome {
                Ok(value) => return Ok(value),
                Err(retryable) if retryable.is_transport() => {
                    let Some(delay) = self.backoff.delay(retry_index) else {
                        return Err(retryable);
                    };
                    tracing::warn!(
                        ?delay,
                        attempt = retry_index,
                        "retryable files-sync failure"
                    );
                    self.sleeper.sleep(delay).await;
                    retry_index += 1;
                }
                Err(other) => return Err(other),
            }
        }
    }

    /// One request/response round trip.
    async fn attempt(
        &self,
        method: Method,
        url: Url,
        headers: &impl Fn() -> Vec<(&'static str, String)>,
        body: Option<&[u8]>,
    ) -> Result<Raw, TransportError> {
        let mut request = self
            .http
            .request(method, url)
            .basic_auth(self.credentials_user(), Some(self.credentials_token()));
        for (name, value) in headers() {
            request = request.header(name, value);
        }
        if let Some(bytes) = body {
            request = request.body(bytes.to_vec());
        }
        let response = request.send().await?;
        let status = response.status();
        let response_headers = response.headers();
        let etag = response_headers
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|v| Etag::new(v.to_owned()));
        let retry_after = response_headers
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_retry_after);
        Ok(Raw {
            status,
            retry_after,
            etag,
            body: response.bytes().await?.to_vec(),
        })
    }

    /// Splits the stored `user:token` back apart for `basic_auth`
    /// (Nextcloud app passwords never contain colons).
    fn credentials_user(&self) -> &str {
        self.credentials
            .split_once(':')
            .map_or(&self.credentials, |(u, _)| u)
    }

    fn credentials_token(&self) -> &str {
        self.credentials.split_once(':').map_or("", |(_, t)| t)
    }
}

impl FilesTransport for WebDavFilesTransport {
    async fn mkcol(&self, path: &str) -> Result<MkcolOutcome, TransportError> {
        let url = self.url_for(path)?;
        self.send_with_retries(mkcol_method(), url, Vec::new, None, |raw| {
            map_status(&raw)?;
            match raw.status {
                StatusCode::CREATED => Ok(MkcolOutcome::Created),
                StatusCode::METHOD_NOT_ALLOWED => Ok(MkcolOutcome::AlreadyExists),
                other => Err(TransportError::Server(other.as_u16())),
            }
        })
        .await
    }

    async fn propfind_devices(&self) -> Result<PropfindListing, TransportError> {
        let url = self
            .base_url
            .join(&format!("{ROOT_FOLDER_NAME}/devices/"))
            .map_err(|_| TransportError::InvalidBaseUrl)?;
        self.send_with_retries(
            propfind_method(),
            url,
            || {
                vec![
                    ("depth", "1".to_owned()),
                    ("content-type", XML_CONTENT_TYPE.to_owned()),
                ]
            },
            Some(PROPFIND_DEPTH1_BODY.as_bytes()),
            |raw| {
                if raw.status != StatusCode::MULTI_STATUS {
                    map_status(&raw)?;
                    return Err(TransportError::Server(raw.status.as_u16()));
                }
                extract_207(&raw.body).map_err(TransportError::from)
            },
        )
        .await
    }

    async fn get(&self, path: &str) -> Result<FileContent, TransportError> {
        let url = self.url_for(path)?;
        self.send_with_retries(Method::GET, url, Vec::new, None, |raw| {
            map_status(&raw)?;
            if raw.status != StatusCode::OK {
                return Err(TransportError::Server(raw.status.as_u16()));
            }
            Ok(FileContent {
                bytes: raw.body,
                etag: raw.etag,
            })
        })
        .await
    }

    async fn put_create(&self, path: &str, body: &[u8]) -> Result<PutOutcome, TransportError> {
        let url = self.url_for(path)?;
        self.send_with_retries(
            Method::PUT,
            url,
            || vec![("if-none-match", "*".to_owned())],
            Some(body),
            |raw| {
                map_status(&raw)?;
                match raw.status {
                    StatusCode::CREATED => Ok(PutOutcome::Created { etag: raw.etag }),
                    StatusCode::PRECONDITION_FAILED => Err(TransportError::PreconditionFailed),
                    other => Err(TransportError::Server(other.as_u16())),
                }
            },
        )
        .await
    }

    async fn put_if_match(
        &self,
        path: &str,
        body: &[u8],
        etag: &Etag,
    ) -> Result<PutOutcome, TransportError> {
        let url = self.url_for(path)?;
        // The verbatim rule is exactly this line: the served value goes
        // into the header byte-for-byte (no trim, no re-quote).
        let if_match = etag.to_string();
        self.send_with_retries(
            Method::PUT,
            url,
            move || vec![("if-match", if_match.clone())],
            Some(body),
            |raw| {
                map_status(&raw)?;
                match raw.status {
                    s if s == StatusCode::NO_CONTENT || s == StatusCode::OK => {
                        Ok(PutOutcome::Updated { etag: raw.etag })
                    }
                    StatusCode::PRECONDITION_FAILED => Err(TransportError::PreconditionFailed),
                    other => Err(TransportError::Server(other.as_u16())),
                }
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_retry_budget_is_three_attempts_with_doubling_delays() {
        let policy = BackoffPolicy::new();
        assert_eq!(policy.delay(0), Some(Duration::from_millis(500)));
        assert_eq!(policy.delay(1), Some(Duration::from_secs(1)));
        assert_eq!(policy.delay(2), None);
    }

    #[test]
    fn retry_after_parses_only_the_delay_seconds_form() {
        assert_eq!(parse_retry_after("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after(""), None);
        assert_eq!(parse_retry_after(" 30"), None);
        assert_eq!(parse_retry_after("+30"), None);
        assert_eq!(parse_retry_after("abc"), None);
        assert_eq!(parse_retry_after("99999999999999999999"), None);
    }

    #[test]
    fn debug_redacts_credentials() {
        let transport =
            WebDavFilesTransport::new("https://cloud.example.com", "alice", "s3cret-token")
                .expect("valid URL");
        let rendered = format!("{transport:?}");
        assert!(!rendered.contains("s3cret-token"), "token leaked in Debug");
        assert!(rendered.contains("[redacted]"));
    }

    #[test]
    fn invalid_base_urls_are_typed_rejections() {
        for url in [
            "not-a-url",
            "ftp://x.example",
            "https://",
            "https://u:p@host",
        ] {
            assert!(
                matches!(
                    WebDavFilesTransport::new(url, "u", "t"),
                    Err(TransportError::InvalidBaseUrl)
                ),
                "{url} must be rejected"
            );
        }
    }
}
