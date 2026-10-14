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
    /// `drain_budget` bounds the wait. Past it the drain is declared
    /// over — the accept loop and dispatcher are *aborted* (in-flight
    /// connections are dropped; typically one wedged peer) so this
    /// future always completes and the caller's `Arc<App>` refcount is
    /// released either way. The daemon's shutdown must never depend on
    /// a peer's behavior.
    ///
    /// # Panics
    ///
    /// Never on the happy path; an internal task panicking (a bug)
    /// surfaces here as a join error, logged and swallowed — the
    /// daemon's own shutdown must proceed regardless.
    pub async fn join(self, drain_budget: Duration) {
        // Stop accepting (idempotent — a `Shutdown` request may have
        // fired already); then drain what was accepted.
        self.request_shutdown();
        let mut accept = self.accept;
        let mut dispatcher = self.dispatcher;
        let drain = async {
            // By reference: the handles must survive the timeout so the
            // abort path below can still reach them.
            let _ = (&mut accept).await;
            let _ = (&mut dispatcher).await;
        };
        if tokio::time::timeout(drain_budget, drain).await.is_err() {
            tracing::warn!("ipc drain did not finish in time; dropping in-flight connections");
            accept.abort();
            dispatcher.abort();
            // Await only the handles the timed-out drain did *not*
            // consume: a JoinHandle polled past its completion panics.
            if !accept.is_finished() {
                let _ = accept.await;
            }
            if !dispatcher.is_finished() {
                let _ = dispatcher.await;
            }
        }
        if let Some(sidecar) = &self.sidecar
            && tokio::fs::remove_file(sidecar).await.is_ok()
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
            tracing::debug!(?first, "ipc bind failed; probing for a stale artifact");
            let probe = tokio::time::timeout(STALE_PROBE_BUDGET, Stream::connect(name.clone()))
                .await
                .ok();
            if matches!(probe, Some(Ok(_))) {
                // A live server owns the name: give up loudly.
                return Err(IpcBindError::AddrInUse);
            }
            // Nobody answered (or the probe failed): stale artifact —
            // remove and rebind once. A failed removal keeps the first
            // bind error as the typed cause.
            if let Some(sidecar) = sidecar {
                match tokio::fs::remove_file(sidecar).await {
                    Ok(()) => {
                        tracing::warn!(socket = %sidecar.display(), "stale ipc socket removed");
                    }
                    Err(_) => return Err(IpcBindError::Create(first)),
                }
            }
            match ListenerOptions::new().name(name).create_tokio() {
                Ok(listener) => {
                    harden(sidecar);
                    Ok(listener)
                }
                Err(second) => {
                    tracing::warn!(?second, "ipc bind failed after stale recovery");
                    Err(IpcBindError::Create(second))
                }
            }
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
/// already-accepted connections (their handles are awaited here). The
/// `JoinSet` reaps finished tasks as they are awaited, so a long-lived
/// daemon's per-connection bookkeeping does not accumulate.
async fn accept_loop(
    listener: Listener,
    in_tx: mpsc::Sender<InboundRequest>,
    stop: watch::Sender<bool>,
) {
    let mut stop_rx = stop.subscribe();
    let mut connections = tokio::task::JoinSet::new();
    loop {
        // Biased, accept first: a connection completed in the same poll
        // cycle as the stop signal is still served (the stop is noticed
        // on the next iteration), so `daemon stop` racing one client
        // cannot strand an accepted connection.
        let accepted = tokio::select! {
            biased;
            accepted = listener.accept() => accepted.ok(),
            () = wait_stopped(&mut stop_rx) => None,
        };
        match accepted {
            Some(stream) => {
                let tx = in_tx.clone();
                connections.spawn(serve_connection(stream, tx));
            }
            None => break,
        }
    }
    drop(listener);
    // Drain: every accepted connection is answered (or dies) before the
    // accept task — and therefore `join` — completes.
    while connections.join_next().await.is_some() {}
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
        Request::Exec { v, .. }
        | Request::Status { v }
        | Request::Shutdown { v }
        | Request::DetachConnector { v, .. } => *v,
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
        Request::DetachConnector {
            server_url,
            username,
            ..
        } => {
            // Identity matching (WI-H decision D3): the daemon detaches
            // only the connector whose credential is exactly the one the
            // client just revoked. A standalone daemon has no connector
            // to drop regardless of the identity.
            use crate::config::SyncBackend;
            use crate::ipc::protocol::DetachWireOutcome;
            let outcome = if app.config.sync_backend() == SyncBackend::Standalone {
                DetachWireOutcome::NoConnector
            } else if app.config.nextcloud.server_url.as_deref() == Some(server_url.as_str())
                && app.config.nextcloud.username.as_deref() == Some(username.as_str())
            {
                match app.detach_connector().await {
                    crate::bootstrap::DetachOutcome::Detached => DetachWireOutcome::Detached,
                    crate::bootstrap::DetachOutcome::NoConnector => DetachWireOutcome::NoConnector,
                    crate::bootstrap::DetachOutcome::TimedOut => DetachWireOutcome::TimedOut,
                }
            } else {
                tracing::warn!(
                    requested = %server_url,
                    "detach requested for a foreign identity; refusing"
                );
                DetachWireOutcome::IdentityMismatch
            };
            Response::DetachConnector {
                v: PROTOCOL_VERSION,
                outcome,
            }
        }
    }
}

/// Scans the relayed argv for a `--config` flag: an identical config
/// file is stripped (both the `--config value` and `--config=value`
/// shapes), a different one is refused. No flag → the argv unchanged.
/// Identity is by *canonicalized* path on both sides: the daemon may
/// have been pointed at its config through a relative path (or a
/// symlink), and the client through another spelling of the same file.
fn strip_matching_config(
    argv: &[String],
    session_config_path: Option<&std::path::Path>,
) -> Result<Vec<String>, ()> {
    let session_canon = session_config_path.and_then(|path| path.canonicalize().ok());
    let mut cleaned = Vec::with_capacity(argv.len());
    let mut index = 0;
    while index < argv.len() {
        let word = &argv[index];
        let (value, consumed_next): (Option<&str>, bool) = if word == "--config" {
            (argv.get(index + 1).map(String::as_str), true)
        } else {
            (word.strip_prefix("--config="), false)
        };
        match value {
            None => cleaned.push(word.clone()),
            Some(value) => {
                let same = matches!(
                    (session_canon.as_deref(), std::path::Path::new(value).canonicalize()),
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
            // `connect`/`login`/`logout` are terminal-bound ceremonies
            // (ADR 0014, lane `Local`): they elicit masked secrets on
            // the caller's terminal, which the socket is not. The
            // usage-class refusal keeps the wire behavior of the
            // parse-refusal era (code 2); the guidance text is
            // server-local UX, never asserted.
            if matches!(
                command,
                crate::cli::Command::Connect { .. }
                    | crate::cli::Command::Login { .. }
                    | crate::cli::Command::Logout
            ) {
                return (
                    2,
                    String::new(),
                    "connect, login, and logout are terminal-bound ceremonies; run them in the \
                     CLI, not against the daemon socket\n"
                        .to_owned(),
                );
            }
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

#[cfg(test)]
mod config_binding_tests {
    use super::strip_matching_config;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    /// The `--config` binding (ADR 0012 §2.10): the daemon's own config
    /// file is stripped in both flag shapes and across path *spellings*
    /// (a relative or `..`-laden boot path vs a different spelling on
    /// the wire — identity is by canonicalized path); any other file is
    /// refused, and a flag-less argv passes through untouched.
    #[test]
    fn the_daemons_own_config_is_stripped_across_spellings() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        std::fs::write(&config, "[app]\n").expect("write config");
        let abs = config.to_string_lossy().into_owned();
        let roundabout = dir
            .path()
            .join("..")
            .join(dir.path().file_name().expect("dir name"))
            .join("taskboard.toml")
            .to_string_lossy()
            .into_owned();

        // Absolute flag over an absolute session path: stripped.
        let cleaned =
            strip_matching_config(&argv(&["--config", &abs, "tasks", "list"]), Some(&config))
                .expect("the same file");
        assert_eq!(cleaned, argv(&["tasks", "list"]));

        // A non-canonical spelling of the same file: stripped too (the
        // review regression — identity is not by raw string).
        let cleaned = strip_matching_config(
            &argv(&["--config", &roundabout, "tasks", "list"]),
            Some(&config),
        )
        .expect("the same file under another spelling");
        assert_eq!(cleaned, argv(&["tasks", "list"]));

        // The inline shape: stripped as well.
        let inline = format!("--config={abs}");
        let cleaned = strip_matching_config(&argv(&[&inline, "--json", "status"]), Some(&config))
            .expect("the same file");
        assert_eq!(cleaned, argv(&["--json", "status"]));

        // No flag: the argv is unchanged.
        let cleaned =
            strip_matching_config(&argv(&["tasks", "list"]), Some(&config)).expect("no flag");
        assert_eq!(cleaned, argv(&["tasks", "list"]));

        // A different file: refused — also when the session has no
        // config path at all.
        let other = dir.path().join("other.toml");
        std::fs::write(&other, "").expect("write other");
        assert!(
            strip_matching_config(
                &argv(&["--config", &other.to_string_lossy()]),
                Some(&config)
            )
            .is_err()
        );
        assert!(strip_matching_config(&argv(&["--config", &abs]), None).is_err());
    }
}

#[cfg(test)]
mod drain_tests {
    use super::{Duration, IpcServerHandle, watch};

    /// The drain budget is a hard bound: a task that never finishes
    /// (here: a dispatcher stuck forever) is aborted at the budget and
    /// `join` still completes — the daemon's own shutdown must never
    /// depend on a peer's or an in-flight request's behavior (the
    /// WI-5 review regression).
    #[tokio::test]
    async fn join_bounds_the_drain_even_with_a_task_that_never_finishes() {
        let (stop, stop_rx) = watch::channel(false);
        let handle = IpcServerHandle {
            stop,
            stop_rx,
            accept: tokio::spawn(async {}),
            dispatcher: tokio::spawn(std::future::pending::<()>()),
            sidecar: None,
        };
        tokio::time::timeout(
            Duration::from_secs(2),
            handle.join(Duration::from_millis(100)),
        )
        .await
        .expect("join completes although the dispatcher never does");
    }
}
