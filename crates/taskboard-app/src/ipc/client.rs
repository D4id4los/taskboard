// SPDX-License-Identifier: AGPL-3.0-only
//! The CLI-side IPC client (ADR 0012): connect to the daemon serving a
//! database, run one request, and hand the rendered answer back — the
//! binary relays `out`/`err` verbatim and exits with the daemon's code,
//! so the observable contract is byte-identical to the in-process path.
//!
//! Budget policy (ADR 0012 §4.8): local sockets answer or refuse
//! instantly. Concrete refusals (absent socket, refused connection,
//! permissions) return `None` immediately so the caller falls through
//! to the in-process path without latency; only ambiguous failures
//! retry within a 1 s connect budget. There is deliberately no read
//! deadline: every verb self-bounds and a wedged daemon is a
//! supervision problem (`status` / `daemon stop`), not a client-timeout
//! problem.

use std::path::Path;
use std::time::{Duration, Instant};

use interprocess::local_socket::{tokio::Stream, traits::tokio::Stream as _};

use crate::commands::CommandError;
use crate::commands::status::StatusView;
use crate::ipc::codec;
use crate::ipc::protocol::{PROTOCOL_VERSION, Request, Response};
use crate::ipc::socket_name;

/// The connect budget for ambiguous transport failures only (see the
/// module docs): on expiry the caller falls through to the in-process
/// path — an honest signal, not a hang.
const CONNECT_BUDGET: Duration = Duration::from_secs(1);
/// The ambiguous-failure retry cadence (well under the budget).
const CONNECT_RETRY: Duration = Duration::from_millis(50);

/// The response-side version gate (pure; unit-pinned): a typed
/// `VersionMismatch` or any wrong-version answer is the `ipc-version`
/// failure; a well-versioned response passes through.
///
/// # Errors
///
/// [`CommandError::IpcVersion`] on any version disagreement.
pub fn validate_response(response: Response) -> Result<Response, CommandError> {
    match response {
        Response::VersionMismatch { server_v } => Err(CommandError::IpcVersion { server_v }),
        other if other.version() != Some(PROTOCOL_VERSION) => Err(CommandError::IpcVersion {
            server_v: other.version().unwrap_or(PROTOCOL_VERSION),
        }),
        other => Ok(other),
    }
}

/// One CLI invocation's relayed outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutcome {
    /// The daemon-side process exit code (0/1/2).
    pub code: u8,
    /// Rendered stdout bytes — written to our stdout verbatim.
    pub out: String,
    /// Rendered stderr bytes — written to our stderr verbatim.
    pub err: String,
}

/// An open connection to the daemon serving a database. One request per
/// connection: the request methods consume `self` (the daemon closes
/// after answering).
pub struct Connection {
    stream: Stream,
}

/// Tries to connect to the daemon serving `db_path`: `Some(connection)`
/// when a daemon answers, `None` when nothing does (concrete refusals
/// return immediately; ambiguous failures retry within the connect
/// budget).
#[must_use]
pub async fn try_connect(db_path: &Path) -> Option<Connection> {
    let name = socket_name(db_path).ok()?;
    let deadline = Instant::now() + CONNECT_BUDGET;
    loop {
        match Stream::connect(name.clone()).await {
            Ok(stream) => {
                tracing::debug!(db = %db_path.display(), "serving over ipc");
                return Some(Connection { stream });
            }
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                // No artifact / stale artifact / not ours: no daemon we
                // can talk to — fall through now.
                return None;
            }
            Err(err) => {
                if Instant::now() >= deadline {
                    tracing::debug!(?err, "ipc connect budget expired");
                    return None;
                }
                tokio::time::sleep(CONNECT_RETRY).await;
            }
        }
    }
}

impl Connection {
    /// Sends one request and reads the answer, mapping every transport
    /// failure to [`CommandError::IpcTransport`] and a version mismatch
    /// to [`CommandError::IpcVersion`].
    ///
    /// # Errors
    ///
    /// [`CommandError::IpcTransport`] on framing/serialization/socket
    /// failure; [`CommandError::IpcVersion`] when the server answers
    /// `VersionMismatch` or an unparseable version.
    async fn request(mut self, req: Request, response_cap: u32) -> Result<Response, CommandError> {
        let bytes = serde_json::to_vec(&req).map_err(|_| CommandError::IpcTransport)?;
        codec::write_frame(&mut self.stream, &bytes, codec::MAX_REQUEST_BYTES)
            .await
            .map_err(|_| CommandError::IpcTransport)?;
        let answer = codec::read_frame(&mut self.stream, response_cap)
            .await
            .map_err(|_| CommandError::IpcTransport)?;
        let response: Response =
            serde_json::from_slice(&answer).map_err(|_| CommandError::IpcTransport)?;
        validate_response(response)
    }

    /// Runs one argv invocation on the daemon.
    ///
    /// # Errors
    ///
    /// [`CommandError`] — transport and version classes; the *command's*
    /// own failure is not an error here (it travels in
    /// [`ExecOutcome::code`] and [`ExecOutcome::err`]).
    pub async fn exec(self, argv: &[String]) -> Result<ExecOutcome, CommandError> {
        match self
            .request(
                Request::Exec {
                    v: PROTOCOL_VERSION,
                    argv: argv.to_vec(),
                },
                codec::MAX_RESPONSE_BYTES,
            )
            .await?
        {
            Response::Exec { code, out, err, .. } => Ok(ExecOutcome { code, out, err }),
            _ => Err(CommandError::IpcTransport),
        }
    }

    /// Asks for the live status view (never triggers a cycle).
    ///
    /// # Errors
    ///
    /// [`CommandError`] — transport and version classes.
    pub async fn status(self) -> Result<StatusView, CommandError> {
        match self
            .request(
                Request::Status {
                    v: PROTOCOL_VERSION,
                },
                codec::MAX_RESPONSE_BYTES,
            )
            .await?
        {
            Response::Status { status, .. } => Ok(status),
            _ => Err(CommandError::IpcTransport),
        }
    }

    /// Requests the graceful shutdown; returns after the ack (the drain
    /// begins server-side).
    ///
    /// # Errors
    ///
    /// [`CommandError`] — transport and version classes.
    pub async fn shutdown(self) -> Result<(), CommandError> {
        match self
            .request(
                Request::Shutdown {
                    v: PROTOCOL_VERSION,
                },
                codec::MAX_RESPONSE_BYTES,
            )
            .await?
        {
            Response::Shutdown { .. } => Ok(()),
            _ => Err(CommandError::IpcTransport),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validate_response;
    use crate::commands::CommandError;
    use crate::ipc::protocol::{PROTOCOL_VERSION, Response};

    /// The response-side version gate: mismatch variants are typed
    /// failures carrying the peer's version; well-versioned responses
    /// pass through untouched.
    #[test]
    fn the_response_gate_types_version_mismatches() {
        assert!(matches!(
            validate_response(Response::VersionMismatch { server_v: 7 }),
            Err(CommandError::IpcVersion { server_v: 7 })
        ));
        assert!(matches!(
            validate_response(Response::Exec {
                v: PROTOCOL_VERSION + 1,
                code: 0,
                out: String::new(),
                err: String::new(),
            }),
            Err(CommandError::IpcVersion { server_v: 2 })
        ));
        assert!(matches!(
            validate_response(Response::Exec {
                v: PROTOCOL_VERSION,
                code: 0,
                out: "out".to_owned(),
                err: String::new(),
            }),
            Ok(Response::Exec { code: 0, .. })
        ));
    }
}
