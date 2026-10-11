// SPDX-License-Identifier: AGPL-3.0-only
//! The daemon-side IPC server (ADR 0012): bind the per-database socket,
//! accept connections concurrently, execute requests **serially** (the
//! same engine-fairness model the REPL loop and the sync actor get —
//! the dispatcher task is the only `&App` holder), and drain gracefully
//! on shutdown.
//!
//! Lifecycle: `spawn_server` binds (`<db>.sock`, recovering a stale
//! artifact from a crashed daemon), spawns the accept loop and the
//! dispatcher, and returns an [`IpcServerHandle`]. A `Shutdown` request
//! (or [`IpcServerHandle::request_shutdown`]) stops the accept loop;
//! [`IpcServerHandle::join`] waits for the in-flight work and removes
//! the socket file. One request per connection — connect, frame,
//! response frame, close (keep-alive is a protocol-v2 concern).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use interprocess::local_socket::{
    ListenerOptions, Name,
    tokio::{Listener, Stream},
    traits::tokio::{Listener as _, Stream as _},
};
use tokio::sync::{mpsc, oneshot, watch};

use crate::App;
use crate::commands::{BasicSession, CommandIo, dispatch_line};
use crate::ipc::protocol::{PROTOCOL_VERSION, Request, Response};
use crate::ipc::{codec, socket_name, socket_sidecar};

/// How long a stale-socket connect probe may take before the artifact
/// is declared dead (local sockets answer or refuse instantly; the
/// budget only guards pathological filesystems).
const STALE_PROBE_BUDGET: Duration = Duration::from_millis(200);

/// One accepted request on its way to the serial dispatcher, with the
/// one-shot channel the answer travels back through.
struct InboundRequest {
    req: Request,
    reply: oneshot::Sender<Response>,
}

/// Bind failure classes (typed; surfaced by the daemon lifecycle).
#[derive(Debug, thiserror::Error)]
pub enum IpcBindError {
    /// The socket could not be created (and stale recovery did not
    /// apply or did not help).
    #[error("ipc socket could not be created")]
    Create(#[source] std::io::Error),
    /// Another live server already answers on this name. Impossible
    /// while the lock discipline holds (we hold the instance lock);
    /// surfaced loudly rather than looped on.
    #[error("another server already answers on the ipc socket")]
    AddrInUse,
}

/// Handle over the running IPC server: the graceful-shutdown control
/// surface of the daemon lifecycle.
pub struct IpcServerHandle {
    stop: watch::Sender<bool>,
    stop_rx: watch::Receiver<bool>,
    accept: tokio::task::JoinHandle<()>,
    dispatcher: tokio::task::JoinHandle<()>,
    /// The unix sidecar artifact to remove at join; `None` on Windows
    /// (namespaced names have no filesystem artifact).
    sidecar: Option<PathBuf>,
}

impl IpcServerHandle {
    /// Requests the graceful drain (stop accepting, finish in-flight).
    /// Idempotent; also fired by a `Shutdown` request.
    pub fn request_shutdown(&self) {
        let _ = self.stop.send(true);
    }

    /// Resolves once a shutdown was requested (remotely via a
    /// `Shutdown` request, or locally via [`Self::request_shutdown`]).
    pub async fn stopped(&self) {
        let mut rx = self.stop_rx.clone();
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                return;
            }
        }
    }

    /// Drains: stops accepting, waits for every accepted connection to
    /// be answered and for the dispatcher to go idle, then removes the
    /// socket artifact (unix).
    ///
    /// # Panics
    ///
    /// Never on the happy path; an internal task panicking (a bug)
    /// surfaces here as a join error, logged and swallowed — the
    /// daemon's own shutdown must proceed regardless.
    pub async fn join(self) {
        // Stop accepting (idempotent — a `Shutdown` request may have
        // fired already); then drain what was accepted.
        self.request_shutdown();
        let _ = self.accept.await;
        let _ = self.dispatcher.await;
        if let Some(sidecar) = &self.sidecar
            && std::fs::remove_file(sidecar).is_ok()
        {
            tracing::debug!(socket = %sidecar.display(), "ipc socket removed");
        }
    }
}

/// Binds the per-database socket and spawns the accept loop + dispatcher.
///
/// # Errors
///
/// [`IpcBindError`] — typed, see the variant docs.
pub async fn spawn_server(
    app: Arc<App>,
    session: Arc<BasicSession>,
    db_path: &std::path::Path,
) -> Result<IpcServerHandle, IpcBindError> {
    let name = socket_name(db_path).map_err(IpcBindError::Create)?;
    #[cfg(unix)]
    let sidecar: Option<PathBuf> = Some(socket_sidecar(db_path));
    #[cfg(not(unix))]
    let sidecar: Option<PathBuf> = None;

    let listener = bind_with_recovery(name, sidecar.as_deref()).await?;

    let (in_tx, mut in_rx) = mpsc::channel::<InboundRequest>(64);
    let (stop, stop_rx) = watch::channel(false);

    // The dispatcher is the only `&App` holder: one in-flight request,
    // engine-fairness by construction (ADR 0012 §2.8).
    let dispatcher_app = app;
    let stop_for_dispatcher = stop.clone();
    let dispatcher = tokio::spawn(async move {
        while let Some(inbound) = in_rx.recv().await {
            let response = handle(dispatcher_app.as_ref(), session.as_ref(), inbound.req).await;
            if let Response::Shutdown { .. } = response {
                // The ack is written by the connection task; begin the
                // drain now.
                let _ = stop_for_dispatcher.send(true);
            }
            let _ = inbound.reply.send(response);
        }
    });

    let accept = tokio::spawn(accept_loop(listener, in_tx, stop.clone()));
    tracing::info!("ipc server accepting requests");

    Ok(IpcServerHandle {
        stop,
        stop_rx,
        accept,
        dispatcher,
        sidecar,
    })
}

fn clone_first(err: &std::io::Error) -> std::io::Error {
    std::io::Error::new(err.kind(), err.to_string())
}

/// Binds the listener, recovering a stale socket artifact (a crashed
/// daemon's `<db>.sock`) exactly once: on bind failure, probe the name
/// — nobody answers ⇒ remove the file and rebind; somebody answers ⇒ a
/// live server owns the name (impossible while we hold the instance
/// lock) and that is surfaced as [`IpcBindError::AddrInUse`], never
/// looped on.
async fn bind_with_recovery(
    name: Name<'static>,
    sidecar: Option<&std::path::Path>,
) -> Result<Listener, IpcBindError> {
    match ListenerOptions::new().name(name.clone()).create_tokio() {
        Ok(listener) => {
            harden(sidecar);
            Ok(listener)
        }
        Err(first) => {
            let probe = tokio::time::timeout(STALE_PROBE_BUDGET, Stream::connect(name.clone()))
                .await
                .ok();
            if matches!(probe, Some(Ok(_))) {
                // A live server owns the name: give up loudly.
                return Err(IpcBindError::AddrInUse);
            }
            // Nobody answered (or the probe failed): stale artifact —
            // remove and rebind once.
            if let Some(sidecar) = sidecar {
                std::fs::remove_file(sidecar)
                    .map_err(|_| IpcBindError::Create(clone_first(&first)))?;
                tracing::warn!(socket = %sidecar.display(), "stale ipc socket removed");
            }
            ListenerOptions::new()
                .name(name)
                .create_tokio()
                .map_err(|second| {
                    tracing::warn!(?first, ?second, "ipc bind failed after stale recovery");
                    IpcBindError::Create(second)
                })
                .inspect(|_| harden(sidecar))
        }
    }
}

/// Owner-only permissions on the unix socket artifact (the whole v1
/// auth story: OS object permissions, exactly as trusted as the
/// `taskboard.toml` + db + lock files beside it). Best-effort: a
/// chmod failure is logged, not fatal (the pre-chmod window is
/// documented in ADR 0012).
#[cfg(unix)]
fn harden(sidecar: Option<&std::path::Path>) {
    use std::os::unix::fs::PermissionsExt as _;
    if let Some(sidecar) = sidecar {
        let outcome = std::fs::set_permissions(sidecar, std::fs::Permissions::from_mode(0o600));
        if let Err(err) = outcome {
            tracing::warn!(?err, "could not harden the ipc socket permissions");
        }
    }
}

/// Nothing to harden on Windows: namespaced names have no filesystem
/// artifact (the security boundary is the local machine session).
#[cfg(not(unix))]
fn harden(_sidecar: Option<&std::path::Path>) {}

/// Accept loop: accepts concurrently, reads one frame per connection,
/// forwards the request to the serial dispatcher, and writes the answer.
/// A connection task that dies (transport garbage, peer gone) only ends
/// that connection. On the stop signal: stop accepting, then drain the
/// already-accepted connections (their handles are awaited here).
async fn accept_loop(
    listener: Listener,
    in_tx: mpsc::Sender<InboundRequest>,
    stop: watch::Sender<bool>,
) {
    let mut stop_rx = stop.subscribe();
    let mut connections = Vec::new();
    loop {
        let accepted = tokio::select! {
            () = wait_stopped(&mut stop_rx) => None,
            accepted = listener.accept() => accepted.ok(),
        };
        match accepted {
            Some(stream) => {
                let tx = in_tx.clone();
                connections.push(tokio::spawn(serve_connection(stream, tx)));
            }
            None => break,
        }
    }
    drop(listener);
    // Drain: every accepted connection is answered (or dies) before the
    // accept task — and therefore `join` — completes.
    for connection in connections {
        let _ = connection.await;
    }
}

/// Resolves when the stop flag is set (borrows the receiver so the
/// `select!` arms borrow-check).
async fn wait_stopped(stop_rx: &mut watch::Receiver<bool>) {
    while !*stop_rx.borrow_and_update() {
        if stop_rx.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

/// One connection: read the request frame, hand it to the dispatcher,
/// write the response frame, close. A mismatched protocol version is
/// answered here (no engine involvement) with the typed
/// `VersionMismatch` response.
async fn serve_connection(mut stream: Stream, in_tx: mpsc::Sender<InboundRequest>) {
    let bytes = match codec::read_frame(&mut stream, codec::MAX_REQUEST_BYTES).await {
        Ok(bytes) => bytes,
        Err(err) => {
            tracing::debug!(?err, "ipc request frame unreadable; dropping connection");
            return;
        }
    };
    let req: Request = match serde_json::from_slice(&bytes) {
        Ok(req) => req,
        Err(err) => {
            tracing::debug!(?err, "ipc request undecodable; dropping connection");
            return;
        }
    };
    if request_version(&req) != PROTOCOL_VERSION {
        let mismatch = Response::VersionMismatch {
            server_v: PROTOCOL_VERSION,
        };
        let answer = serde_json::to_vec(&mismatch).expect("response serialization is total");
        let _ = codec::write_frame(&mut stream, &answer, codec::MAX_RESPONSE_BYTES).await;
        return;
    }
    let (reply_tx, reply_rx) = oneshot::channel();
    if in_tx
        .send(InboundRequest {
            req,
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        return; // server is shutting down
    }
    let Ok(response) = reply_rx.await else {
        return; // dispatcher gone
    };
    let answer = serde_json::to_vec(&response).expect("response serialization is total");
    if let Err(err) = codec::write_frame(&mut stream, &answer, codec::MAX_RESPONSE_BYTES).await {
        tracing::debug!(?err, "ipc response frame failed; dropping connection");
    }
}

/// The request's protocol version (every versioned variant carries one).
fn request_version(req: &Request) -> u32 {
    match req {
        Request::Exec { v, .. } | Request::Status { v } | Request::Shutdown { v } => *v,
    }
}

/// The serial execution of one request against the booted core.
#[allow(clippy::too_many_lines)] // one arm per request kind, kept flat
async fn handle(app: &App, session: &BasicSession, req: Request) -> Response {
    match req {
        Request::Exec { argv, .. } => {
            let (code, out, err) = run_argv(app, session, &argv).await;
            Response::Exec {
                v: PROTOCOL_VERSION,
                code,
                out,
                err,
            }
        }
        Request::Status { .. } => Response::Status {
            v: PROTOCOL_VERSION,
            status: crate::commands::status::StatusView::server(app),
        },
        Request::Shutdown { .. } => {
            tracing::info!("graceful shutdown requested over ipc");
            Response::Shutdown {
                v: PROTOCOL_VERSION,
            }
        }
    }
}

/// Scans the relayed argv for a `--config` flag: an identical config
/// file is stripped (both the `--config value` and `--config=value`
/// shapes), a different one is refused. No flag → the argv unchanged.
fn strip_matching_config(
    argv: &[String],
    session_config_path: Option<&std::path::Path>,
) -> Result<Vec<String>, ()> {
    let mut cleaned = Vec::with_capacity(argv.len());
    let mut index = 0;
    while index < argv.len() {
        let word = &argv[index];
        let (value, consumed_next): (Option<&String>, bool) = if word == "--config" {
            (argv.get(index + 1), true)
        } else if let Some(inline) = word.strip_prefix("--config=") {
            (Some(&inline.to_owned()), false)
        } else {
            (None, false)
        };
        match value {
            None => cleaned.push(word.clone()),
            Some(value) => {
                let same = matches!(
                    (session_config_path, std::path::Path::new(value.as_str()).canonicalize()),
                    (Some(session_path), Ok(client_path)) if session_path == client_path
                );
                if !same {
                    return Err(());
                }
                if consumed_next {
                    index += 1;
                }
            }
        }
        index += 1;
    }
    Ok(cleaned)
}

/// Runs one argv invocation through the real parse-and-route stack into
/// injected buffers — the exact bytes the in-process dispatch would
/// have printed (ADR 0009's contract), plus the process-shaped code.
async fn run_argv(app: &App, session: &BasicSession, argv: &[String]) -> (u8, String, String) {
    // The socket is the config binding (ADR 0012 §2.10): a relayed
    // `--config` naming the config the daemon itself booted with is
    // honored (stripped — it was already consumed for routing); any
    // *other* file is the degenerate same-db-different-file case and
    // gets the usage-class refusal.
    let Ok(argv) = strip_matching_config(argv, session.config_path.as_deref()) else {
        return (
            2,
            String::new(),
            format!("{}\n", crate::repl::LineRefusal::ConfigFlag),
        );
    };
    match crate::repl::resolve_words(&argv, false) {
        crate::repl::LineAction::Execute(command, json) => {
            let mut out = Vec::new();
            let mut err = Vec::new();
            let mut io = CommandIo {
                out: &mut out,
                err: &mut err,
                json,
            };
            let code = dispatch_line(session, app, command, &mut io).await;
            // Buffer writers are UTF-8 by construction: lossless.
            (
                code,
                String::from_utf8_lossy(&out).into_owned(),
                String::from_utf8_lossy(&err).into_owned(),
            )
        }
        crate::repl::LineAction::Refuse(kind) => {
            // Usage-class refusal: code 2, the guidance text on `err`.
            (2, String::new(), format!("{kind}\n"))
        }
        crate::repl::LineAction::PrintClap(err) => {
            // clap's routing, replicated: help/version to `out` (code 0),
            // usage errors to `err` (code 2) — what the binary does.
            let rendered = err.render().to_string();
            if matches!(
                err.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                (0, format!("{rendered}\n"), String::new())
            } else {
                (2, String::new(), format!("{rendered}\n"))
            }
        }
        // `exit`/`quit` are not subcommands (unit-pinned: argv lands in
        // `PrintClap`), and empty argv cannot arrive over the wire (the
        // CLI always has words or goes to mode resolution).
        crate::repl::LineAction::Exit | crate::repl::LineAction::Reprompt => {
            tracing::debug!("unreachable argv action over ipc");
            (0, String::new(), String::new())
        }
    }
}
