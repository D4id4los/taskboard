// SPDX-License-Identifier: AGPL-3.0-only
//! The CLI-side IPC client (ADR 0012): connect to the daemon serving a
//! database, run one request, and hand the rendered answer back — the
//! binary relays `out`/`err` verbatim and exits with the daemon's code,
//! so the observable contract is byte-identical to the in-process path.
//!
//! Budget policy (ADR 0012 §4.8, as amended 2026-10-27): local sockets
//! answer or refuse instantly. Concrete refusals (absent socket,
//! refused connection, permissions) return `None` immediately so the
//! caller falls through to the in-process path without latency; only
//! ambiguous failures retry within a 1 s connect budget. Every verb
//! still self-bounds; the bounded stance adds a client read deadline
//! that catches only the *wedged daemon* case the old no-deadline
//! policy name-checked: `exec`/`status`/`shutdown` wrap their whole
//! exchange in a budget derived from the verb's own `--timeout-secs`
//! plus a fixed margin (or the flat flag-less ceiling), overridable
//! wholesale (and disable-able with 0) by
//! `[ipc] client_read_timeout_secs`. Expiry is typed
//! [`CommandError::IpcTimeout`] — never transport garbage.

use std::path::Path;
use std::time::{Duration, Instant};

use interprocess::local_socket::{tokio::Stream, traits::tokio::Stream as _};

use crate::commands::CommandError;
use crate::commands::status::StatusView;
use crate::ipc::codec;
use crate::ipc::protocol::{
    AttachWireOutcome, DetachWireOutcome, PROTOCOL_VERSION, Request, Response,
};
use crate::ipc::socket_name;

/// The connect budget for ambiguous transport failures only (see the
/// module docs): on expiry the caller falls through to the in-process
/// path — an honest signal, not a hang.
const CONNECT_BUDGET: Duration = Duration::from_secs(1);
/// The ambiguous-failure retry cadence (well under the budget).
const CONNECT_RETRY: Duration = Duration::from_millis(50);
/// The detach read budget (WI-H): the daemon's graceful join is bounded
/// server-side by its own shutdown timeout; this generous ceiling only
/// bounds pathological hangs, so it must comfortably exceed the largest
/// legitimate bounded join.
const DETACH_BUDGET: Duration = Duration::from_secs(30);
/// The attach read budget (the daemon-side attach): covers the
/// daemon's worst path — serial-queue wait + a detach join + the
/// bounded settle — with margin; on expiry the daemon still completes
/// its attach and the caller degrades to the warning (the detach
/// budget's reading).
const ATTACH_BUDGET: Duration = Duration::from_secs(30);

/// The flag-less read budget (ADR 0012, amended): a verb with no
/// `--timeout-secs` of its own gets this ceiling — it catches only the
/// wedged-daemon case (a healthy daemon's ack is millisecond-scale).
pub(crate) const DEFAULT_READ_BUDGET_SECS: u64 = 30;
/// The fixed margin over a verb's own `--timeout-secs`: the derived
/// deadline is `verb budget + margin`, so it can never fire before the
/// verb's own honest timeout.
pub(crate) const READ_BUDGET_MARGIN_SECS: u64 = 10;

/// Derives the client read budget from the verb's own budget and the
/// `[ipc] client_read_timeout_secs` override (ADR 0012, amended): a
/// flagged verb derives its own budget plus the fixed margin (never
/// preempts the verb's honest timeout); a flag-less verb gets the
/// default ceiling flat. `Some(n)` configured replaces the derivation
/// wholesale; `Some(0)` disables the deadline entirely (`None` here —
/// the recorded supervision stance).
#[must_use]
pub fn client_read_budget(verb_secs: Option<u64>, configured: Option<u64>) -> Option<Duration> {
    match configured {
        Some(0) => None,
        Some(secs) => Some(Duration::from_secs(secs)),
        None => Some(Duration::from_secs(match verb_secs {
            // Saturating: a `--timeout-secs` near `u64::MAX` must stay a
            // (preempt-never) budget, not a debug overflow panic.
            Some(verb) => verb.saturating_add(READ_BUDGET_MARGIN_SECS),
            None => DEFAULT_READ_BUDGET_SECS,
        })),
    }
}

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
    /// to [`CommandError::IpcVersion`]. `read_budget` bounds the whole
    /// exchange (write+read+decode — the detach/attach precedent); its
    /// expiry is [`CommandError::IpcTimeout`], never transport garbage.
    ///
    /// # Errors
    ///
    /// [`CommandError::IpcTransport`] on framing/serialization/socket
    /// failure; [`CommandError::IpcTimeout`] when the daemon does not
    /// answer within the budget; [`CommandError::IpcVersion`] when the
    /// server answers `VersionMismatch` or an unparseable version.
    async fn request(
        mut self,
        req: Request,
        response_cap: u32,
        read_budget: Option<Duration>,
    ) -> Result<Response, CommandError> {
        let exchange = async {
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
        };
        match read_budget {
            Some(budget) => tokio::time::timeout(budget, exchange)
                .await
                .unwrap_or(Err(CommandError::IpcTimeout)),
            None => exchange.await,
        }
    }

    /// Runs one argv invocation on the daemon.
    ///
    /// # Errors
    ///
    /// [`CommandError`] — transport, timeout and version classes; the
    /// *command's* own failure is not an error here (it travels in
    /// [`ExecOutcome::code`] and [`ExecOutcome::err`]).
    pub async fn exec(
        self,
        argv: &[String],
        read_budget: Option<Duration>,
    ) -> Result<ExecOutcome, CommandError> {
        match self
            .request(
                Request::Exec {
                    v: PROTOCOL_VERSION,
                    argv: argv.to_vec(),
                },
                codec::MAX_RESPONSE_BYTES,
                read_budget,
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
    /// [`CommandError`] — transport, timeout and version classes.
    pub async fn status(self, read_budget: Option<Duration>) -> Result<StatusView, CommandError> {
        match self
            .request(
                Request::Status {
                    v: PROTOCOL_VERSION,
                },
                codec::MAX_RESPONSE_BYTES,
                read_budget,
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
    /// [`CommandError`] — transport, timeout and version classes.
    pub async fn shutdown(self, read_budget: Option<Duration>) -> Result<(), CommandError> {
        match self
            .request(
                Request::Shutdown {
                    v: PROTOCOL_VERSION,
                },
                codec::MAX_RESPONSE_BYTES,
                read_budget,
            )
            .await?
        {
            Response::Shutdown { .. } => Ok(()),
            _ => Err(CommandError::IpcTransport),
        }
    }

    /// Asks the daemon to drop its sync connector (WI-H): the identity
    /// the caller just logged out. Bounded by [`DETACH_BUDGET`] — a slow
    /// daemon join cannot hang `logout`; on budget expiry the daemon
    /// still completes its detach and the caller degrades to the
    /// warning.
    ///
    /// # Errors
    ///
    /// [`CommandError`] — transport and version classes (including the
    /// budget expiry and a pre-`detach-connector` daemon, which cannot
    /// decode the variant and drops the connection).
    pub async fn detach(
        self,
        server_url: &str,
        username: &str,
    ) -> Result<DetachWireOutcome, CommandError> {
        let request = Request::DetachConnector {
            v: PROTOCOL_VERSION,
            server_url: server_url.to_owned(),
            username: username.to_owned(),
        };
        match tokio::time::timeout(
            DETACH_BUDGET,
            self.request(request, codec::MAX_RESPONSE_BYTES, None),
        )
        .await
        {
            Ok(Ok(Response::DetachConnector { outcome, .. })) => Ok(outcome),
            Ok(Ok(_)) => Err(CommandError::IpcTransport),
            Ok(Err(err)) => Err(err),
            Err(_elapsed) => Err(CommandError::IpcTransport),
        }
    }

    /// Asks the daemon to attach its connector from persisted state
    /// (ADR 0014 decision 7): the identity the caller just connected.
    /// The secret never rides the wire — the daemon re-reads its own
    /// config and loads its own credential store. Bounded by
    /// [`ATTACH_BUDGET`]; on budget expiry the daemon still completes
    /// its attach and the caller degrades to the warning.
    ///
    /// # Errors
    ///
    /// [`CommandError`] — transport and version classes (including the
    /// budget expiry and a pre-`attach-connector` daemon, which cannot
    /// decode the variant and drops the connection).
    pub async fn attach(
        self,
        server_url: &str,
        username: &str,
    ) -> Result<AttachWireOutcome, CommandError> {
        let request = Request::AttachConnector {
            v: PROTOCOL_VERSION,
            server_url: server_url.to_owned(),
            username: username.to_owned(),
            // The client ceremony always targets the default (it
            // rewrote entry 0); a named target is chunk 6's flag.
            name: None,
        };
        match tokio::time::timeout(
            ATTACH_BUDGET,
            self.request(request, codec::MAX_RESPONSE_BYTES, None),
        )
        .await
        {
            Ok(Ok(Response::AttachConnector { outcome, .. })) => Ok(outcome),
            Ok(Ok(_)) => Err(CommandError::IpcTransport),
            Ok(Err(err)) => Err(err),
            Err(_elapsed) => Err(CommandError::IpcTransport),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validate_response;
    use super::{
        CONNECT_BUDGET, DEFAULT_READ_BUDGET_SECS, READ_BUDGET_MARGIN_SECS, client_read_budget,
        try_connect,
    };
    use crate::cli::Command;
    use crate::commands::{CommandError, verb_timeout_secs};
    use crate::ipc::protocol::{PROTOCOL_VERSION, Response};
    use crate::ipc::{socket_name, socket_sidecar};
    use interprocess::local_socket::traits::tokio::Listener as _;

    /// C1: the derivation lattice (ADR 0012, amended) — flag verbs
    /// derive `budget + margin`, flag-less verbs the default ceiling,
    /// a configured value replaces wholesale, and `0` disables.
    #[test]
    fn the_read_budget_derivation_lattice_holds() {
        // A flag verb: its own budget plus the margin.
        assert_eq!(
            client_read_budget(Some(60), None),
            Some(std::time::Duration::from_secs(70))
        );
        // A flag-less verb: the default ceiling, flat (no margin — the
        // margin is only ever over a verb's own honest budget).
        assert_eq!(
            client_read_budget(None, None),
            Some(std::time::Duration::from_secs(DEFAULT_READ_BUDGET_SECS))
        );
        // The override replaces the derivation wholesale, margin
        // included.
        assert_eq!(
            client_read_budget(Some(60), Some(5)),
            Some(std::time::Duration::from_secs(5))
        );
        assert_eq!(
            client_read_budget(None, Some(120)),
            Some(std::time::Duration::from_secs(120))
        );
        // Zero disables entirely.
        assert_eq!(client_read_budget(Some(60), Some(0)), None);
        assert_eq!(client_read_budget(None, Some(0)), None);
    }

    /// C1 (the margin property): for every verb budget the derived
    /// deadline strictly exceeds it — the deadline can never preempt
    /// an honest long verb.
    #[test]
    fn the_derived_deadline_never_fires_before_the_verbs_own_budget() {
        for verb_secs in [1_u64, 60, 3600, u64::MAX - READ_BUDGET_MARGIN_SECS] {
            let Some(budget) = client_read_budget(Some(verb_secs), None) else {
                panic!("no override: the deadline is always derived");
            };
            assert!(budget > std::time::Duration::from_secs(verb_secs));
        }
    }

    /// C2: the verb budget table — exactly the three `--timeout-secs`
    /// flags (sync, boards select, boards publish) yield their value;
    /// every other verb yields `None` (the exhaustive match forces a
    /// future flag to land here).
    #[test]
    fn the_verb_budget_table_names_exactly_the_flagged_verbs() {
        let parse = |argv: &str| -> Command {
            let words: Vec<String> = argv.split_whitespace().map(str::to_owned).collect();
            match crate::repl::resolve_words(&words, false) {
                crate::repl::LineAction::Execute(command, _) => command,
                other => panic!("{argv} parses to a command, got {other:?}"),
            }
        };
        assert_eq!(
            verb_timeout_secs(&parse("sync")),
            Some(60),
            "the sync default"
        );
        assert_eq!(
            verb_timeout_secs(&parse("boards select 42")),
            Some(60),
            "the boards-select default"
        );
        assert_eq!(
            verb_timeout_secs(&parse("boards publish")),
            Some(60),
            "the boards-publish default"
        );
        // `daemon stop` never parses through the one-shot resolver
        // (the REPL refusal) — constructed directly.
        assert_eq!(
            verb_timeout_secs(&crate::cli::Command::Daemon {
                cmd: Some(crate::cli::DaemonCmd::Stop),
            }),
            None,
        );
        for flagless in [
            "status",
            "boards list",
            "boards create Personal",
            "tasks add --stack Todo x",
            "timers list",
            "find x",
            "capture x",
        ] {
            assert_eq!(
                verb_timeout_secs(&parse(flagless)),
                None,
                "{flagless} carries no verb budget"
            );
        }
    }

    /// C5 (the kind pin): the timeout is its own machine contract in
    /// the `--json` envelope, distinct from transport garbage.
    #[test]
    fn ipc_timeout_is_a_kind_distinct_from_transport() {
        assert_eq!(CommandError::IpcTimeout.error_kind(), "ipc-timeout");
        assert_eq!(CommandError::IpcTransport.error_kind(), "ipc-transport");
        assert_eq!(CommandError::IpcTimeout.error_detail(), None);
    }

    /// A listener that accepts and never answers — the scripted
    /// stalling daemon for the expiry rows (bind on the db's socket
    /// name, exactly like the real server would).
    #[cfg(unix)]
    fn stalling_daemon(db: &std::path::Path) -> interprocess::local_socket::tokio::Listener {
        use interprocess::local_socket::ListenerOptions;
        let name = socket_name(db).expect("socket name");
        ListenerOptions::new()
            .name(name)
            .create_tokio()
            .expect("bind")
    }

    #[cfg(unix)]
    fn is_pending<F: std::future::Future>(fut: std::pin::Pin<&mut F>) -> bool {
        use std::task::Poll;
        matches!(
            fut.poll(&mut std::task::Context::from_waker(std::task::Waker::noop())),
            Poll::Pending
        )
    }

    /// C3: the deadline expiry — a stalling daemon accepts the frame
    /// and never answers; the budget fires typed (`IpcTimeout`), and
    /// the outer wrapper (far above the budget under the paused clock)
    /// never does. The deadline can never preempt an honest verb: the
    /// margin property pins the derivation, this pins the wrap.
    #[cfg(unix)]
    #[tokio::test(start_paused = true)]
    async fn a_stalling_daemon_times_out_typed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("stall.db");
        let _staller = stalling_daemon(&db);
        let conn = try_connect(&db).await.expect("connects to the staller");

        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(600),
            conn.exec(
                &["sync".to_owned()],
                Some(std::time::Duration::from_secs(1)),
            ),
        )
        .await
        .expect("the budget fires long before the outer wrapper");
        assert!(matches!(outcome, Err(CommandError::IpcTimeout)));
    }

    /// C4: the disabled deadline never fires — no override (`None`
    /// budget) means no deadline exists; the future stays `Pending`
    /// through an absurd clock advance (noop-waker polls, no wall
    /// clock, no sleeps).
    #[cfg(unix)]
    #[tokio::test(start_paused = true)]
    async fn a_disabled_deadline_never_fires() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("stall-disabled.db");
        let _staller = stalling_daemon(&db);
        let conn = try_connect(&db).await.expect("connects to the staller");

        let argv = vec!["sync".to_owned()];
        let mut fut = std::pin::pin!(conn.exec(&argv, None));
        // One poll drives the write; the read parks on the staller.
        assert!(is_pending(fut.as_mut()));
        tokio::time::advance(std::time::Duration::from_hours(720)).await;
        assert!(is_pending(fut.as_mut()), "disabled: no deadline exists");
    }

    /// C5 (the wrap): a daemon that dies mid-exchange is still
    /// *transport* garbage from the same `request` path — the timeout
    /// kind is reserved for a silent-but-alive daemon.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_daemon_dropping_mid_exchange_is_transport_not_timeout() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("drop.db");
        let listener = stalling_daemon(&db);
        let conn = try_connect(&db).await.expect("connects");
        // Accept, then hang up without answering: the client's read
        // ends in EOF — transport, never a timeout.
        let accepted = listener.accept().await.expect("accept");
        drop(accepted);
        drop(listener);

        let outcome = conn
            .exec(
                &["sync".to_owned()],
                Some(std::time::Duration::from_secs(30)),
            )
            .await;
        assert!(matches!(outcome, Err(CommandError::IpcTransport)));
    }

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
    /// The connect budget is a hard bound: a socket name that answers
    /// every connect attempt with an *ambiguous* failure (here: a
    /// symlink loop where the sidecar would be — the kernel answers
    /// `ELOOP`, which is neither refused/not-found/permission-shaped)
    /// is retried until the budget expires, then answers `None` — the
    /// caller falls through to the in-process path, never hangs.
    #[cfg(unix)]
    #[tokio::test]
    async fn an_ambiguous_socket_name_falls_through_within_the_budget() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("budget.db");
        let sidecar = socket_sidecar(&db);
        std::os::unix::fs::symlink(&sidecar, &sidecar).expect("symlink loop");

        let started = std::time::Instant::now();
        let outcome = try_connect(&db).await;
        assert!(outcome.is_none(), "nothing can answer on a junk file");
        let elapsed = started.elapsed();
        assert!(
            elapsed >= CONNECT_BUDGET,
            "ambiguous failures must consume the budget, took {elapsed:?}"
        );
        assert!(
            elapsed < CONNECT_BUDGET + std::time::Duration::from_secs(5),
            "the budget bounds the wait, took {elapsed:?}"
        );
    }
}
