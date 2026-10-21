// SPDX-License-Identifier: AGPL-3.0-only
//! The web kiosk dashboard (K11): an opt-in, read-only HTTP surface on
//! the shared headless boot. One hyper accept loop per process; every
//! request is answered from one lock-free `ArcSwap` load plus one
//! metrics read — the server never subscribes to anything (no
//! broadcast receiver, no watcher, no wake source). The route table is
//! the pure `(method, path)` resolution in [`routes`]; the handlers
//! are the pure renderers in [`render`]; this module is the dumb
//! transporter: bind, accept, dispatch, drain.
//!
//! Auth is the capability-token URL: `/d/<token>/…`. A wrong token is
//! indistinguishable from a nonexistent path, and the loopback bind is
//! the trusted default arm — a non-loopback bind requires a *resolvable*
//! token (explicit or persisted), never a freshly minted one the
//! operator has not yet seen in a config or on disk.

pub mod render;
pub mod routes;
pub mod token;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as ConnBuilder;
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::config::{AppConfig, AppMode, WebSection};
use crate::web::render::{RenderCtx, Rendered};
use crate::web::routes::Resolution;
use crate::web::token::TokenOrigin;

pub use token::TokenError;

/// Boot/bind failure classes (typed; surfaced by the daemon lifecycle).
#[derive(Debug, thiserror::Error)]
pub enum WebBindError {
    /// The listener could not be bound (address in use, permission…).
    #[error("web listener could not be bound")]
    Bind(#[source] std::io::Error),
    /// A non-loopback bind whose token had to be freshly generated —
    /// the operator was never shown the capability, so the LAN surface
    /// refuses instead of warn-then-serving.
    #[error("non-loopback web bind requires a resolvable capability token")]
    UnsecuredLanBind,
    /// The token ceremony failed.
    #[error(transparent)]
    Token(#[from] TokenError),
}

/// Everything the request dispatcher needs besides the per-request
/// snapshot reads: the URL space's token and the render context's
/// process-stable facts.
struct Dashboard {
    token: String,
    refresh_secs: u64,
    backend: crate::config::SyncBackend,
    mode: &'static str,
}

/// Handle over the running dashboard server — the [`crate::ipc::server::
/// IpcServerHandle`] shape (stop watch + accept task + bounded join),
/// so the drain choreography treats both surfaces alike.
pub struct DashboardServerHandle {
    stop: watch::Sender<bool>,
    accept: tokio::task::JoinHandle<()>,
    local_addr: SocketAddr,
}

impl DashboardServerHandle {
    /// The bound address (an ephemeral-port bind resolves here).
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Requests the graceful drain (stop accepting, finish in-flight).
    /// Idempotent.
    pub fn request_shutdown(&self) {
        let _ = self.stop.send(true);
    }

    /// Resolves once a shutdown was requested.
    pub async fn stopped(&self) {
        Self::stopped_from(self.stop.subscribe()).await;
    }

    /// The owned-receiver form of [`Self::stopped`].
    pub async fn stopped_from(mut rx: watch::Receiver<bool>) {
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                return;
            }
        }
    }

    /// Drains: stops accepting, waits for every connection to finish
    /// (the per-connection tasks turn keep-alives off at the stop
    /// signal), and always completes. `drain_budget` bounds the wait;
    /// past it the accept task is *aborted* — a wedged peer must never
    /// hold the daemon's shutdown hostage.
    pub async fn join(self, drain_budget: Duration) {
        self.request_shutdown();
        let mut accept = self.accept;
        if tokio::time::timeout(drain_budget, &mut accept)
            .await
            .is_err()
        {
            tracing::warn!("web drain did not finish in time; dropping connections");
            accept.abort();
            if !accept.is_finished() {
                let _ = accept.await;
            }
        }
    }
}

/// Resolves the token, enforces the LAN-bind rule, binds, and spawns
/// the accept loop. The full serving URL (token included — the
/// accepted leakage, design Q2) is logged once at boot.
///
/// # Errors
///
/// [`WebBindError`] — typed, see the variant docs.
pub async fn spawn_dashboard(
    app: &AppConfig,
    engine: taskboard_state::EngineHandle,
    web: WebSection,
) -> Result<DashboardServerHandle, WebBindError> {
    let (token, origin) = token::resolve_token(web.token.as_deref(), app.db_path())?;
    // Config validation already type-checked the parse; treat a
    // hand-built config's bad bind as the typed bind failure.
    let addr: SocketAddr = web.bind.parse().map_err(|err| {
        WebBindError::Bind(std::io::Error::new(std::io::ErrorKind::InvalidInput, err))
    })?;
    if !addr.ip().is_loopback() && origin == TokenOrigin::Generated {
        return Err(WebBindError::UnsecuredLanBind);
    }

    let listener = TcpListener::bind(addr).await.map_err(WebBindError::Bind)?;
    let local_addr = listener.local_addr().map_err(WebBindError::Bind)?;
    let dashboard = Arc::new(Dashboard {
        token,
        refresh_secs: web.refresh_secs,
        backend: app.resolved_default_backend(),
        mode: mode_word(app.app.mode),
    });
    let serving_url = format!("http://{local_addr}/d/{}/", dashboard.token);
    let (stop, stop_rx) = watch::channel(false);
    let accept = tokio::spawn(accept_loop(
        listener,
        dashboard,
        engine,
        stop.clone(),
        stop_rx,
    ));
    tracing::info!(
        url = %serving_url,
        bind = %local_addr,
        "web dashboard serving"
    );
    Ok(DashboardServerHandle {
        stop,
        accept,
        local_addr,
    })
}

fn mode_word(mode: AppMode) -> &'static str {
    match mode {
        AppMode::Daemon => "daemon",
        AppMode::Desktop => "desktop",
        AppMode::Kiosk => "kiosk",
    }
}

/// The accept loop: spawn one task per connection (1–2 pollers at kiosk
/// scale — no executor machinery), stop on the signal, and drain every
/// accepted connection before the task — and therefore `join` — ends.
async fn accept_loop(
    listener: TcpListener,
    dashboard: Arc<Dashboard>,
    engine: taskboard_state::EngineHandle,
    stop: watch::Sender<bool>,
    mut stop_rx: watch::Receiver<bool>,
) {
    // Created by the spawner, not here: a `Sender::subscribe()` after
    // the stop was already sent marks the new value seen, and the loop
    // would wait for a change that never comes (the boot-vs-shutdown
    // race).
    let mut connections = tokio::task::JoinSet::new();
    loop {
        let accepted = tokio::select! {
            biased;
            accepted = listener.accept() => accepted.ok(),
            () = wait_stopped(&mut stop_rx) => None,
        };
        match accepted {
            Some((stream, _peer)) => {
                let dashboard = Arc::clone(&dashboard);
                let engine = engine.clone();
                let mut stop_rx_conn = stop.subscribe();
                connections.spawn(async move {
                    let service = service_fn(move |req: Request<Incoming>| {
                        let dashboard = Arc::clone(&dashboard);
                        let engine = engine.clone();
                        async move {
                            Ok::<_, std::convert::Infallible>(answer(&dashboard, &engine, &req))
                        }
                    });
                    let builder = ConnBuilder::new(TokioExecutor::new());
                    let conn = builder.serve_connection(TokioIo::new(stream), service);
                    // `Connection` is `!Unpin`; pin in place to poll it
                    // alongside the stop watch.
                    tokio::pin!(conn);
                    tokio::select! {
                        outcome = &mut conn => {
                            if let Err(err) = outcome {
                                tracing::debug!(?err, "web connection ended with an error");
                            }
                        }
                        () = wait_stopped(&mut stop_rx_conn) => {
                            // Keep-alives off, in-flight response finished.
                            conn.as_mut().graceful_shutdown();
                            // The remainder resolves once the
                            // in-flight response finished; errors
                            // here are the client's disconnects.
                            let _ = (&mut conn).await;
                        }
                    }
                });
            }
            None => break,
        }
    }
    drop(listener);
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

/// One request → one [`Rendered`], through the pure route table and the
/// pure handlers. Two reads per request, both lock-free: one `ArcSwap`
/// load, one metrics snapshot.
fn answer(
    dashboard: &Dashboard,
    engine: &taskboard_state::EngineHandle,
    req: &Request<Incoming>,
) -> Response<Full<Bytes>> {
    let resolution = routes::resolve(req.method().as_str(), req.uri().path(), &dashboard.token);
    let rendered = match resolution {
        Resolution::Serve(route) => {
            let state = engine.shared_state().load();
            let ctx = RenderCtx {
                token: dashboard.token.clone(),
                refresh_secs: dashboard.refresh_secs,
                backend: dashboard.backend,
                mode: dashboard.mode,
                version: crate::VERSION,
                now: chrono::Utc::now(),
            };
            match route {
                routes::Route::Index => render::render_index(&state, engine.metrics(), &ctx),
                routes::Route::Board(board_id) => {
                    let board = taskboard_domain::BoardId::from_uuid(board_id);
                    render::render_board(&state, board, &ctx)
                        .unwrap_or_else(render::render_not_found)
                }
                routes::Route::Stylesheet => render::render_stylesheet(),
            }
        }
        Resolution::Redirect => render::render_index_redirect(&dashboard.token),
        Resolution::NotFound => render::render_not_found(),
        Resolution::MethodNotAllowed => render::render_method_not_allowed(),
    };
    build_response(&rendered)
}

fn build_response(rendered: &Rendered) -> Response<Full<Bytes>> {
    let mut builder = Response::builder()
        .status(rendered.status)
        .header(hyper::header::CONTENT_TYPE, rendered.content_type)
        .header(hyper::header::CACHE_CONTROL, rendered.cache.value());
    if let Some(location) = &rendered.location {
        builder = builder.header(hyper::header::LOCATION, location);
    }
    if rendered.status == 405 {
        builder = builder.header(hyper::header::ALLOW, "GET");
    }
    builder
        .body(Full::new(Bytes::from(rendered.body.clone())))
        .expect("response construction is total for static parts")
}

#[cfg(test)]
mod tests {
    use super::{DashboardServerHandle, Duration, watch};

    /// The drain budget is a hard bound (the `IpcServerHandle`
    /// contract, mirrored): an accept task that never finishes is
    /// aborted at the budget and `join` still completes.
    #[tokio::test]
    async fn join_bounds_the_drain_even_with_a_task_that_never_finishes() {
        let (stop, _stop_rx) = watch::channel(false);
        let handle = DashboardServerHandle {
            stop,
            accept: tokio::spawn(std::future::pending::<()>()),
            local_addr: std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
        };
        tokio::time::timeout(
            Duration::from_secs(2),
            handle.join(Duration::from_millis(100)),
        )
        .await
        .expect("join completes although the accept task never does");
    }
}
