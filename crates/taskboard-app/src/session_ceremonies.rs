// SPDX-License-Identifier: AGPL-3.0-only
//! The session-layer connector ceremonies (ADR 0014, design §4.4/§4.6):
//! a `connect` (or deprecated-alias `login`) line runs the chunk-2
//! ceremony through the session's prompt reader, refreshes the session
//! config, and attaches chunk-1's `App::attach_connector` to the
//! running core; a `logout` line runs the mirror (local teardown →
//! `detach_connector` → config refresh). These are *session-layer
//! actions*, not `dispatch_line` arms: they mutate the session itself
//! (`&mut ReplSession`) — the rule the ADR records is "verbs that
//! mutate the session itself live in the session layer; the verb bodies
//! stay frontend-agnostic functions."
//!
//! Ordering is the design's: the ceremony completes before anything
//! about the running core changes, every step is typed-failable, and a
//! failed line leaves a recoverable state (another `connect`/`logout`
//! line — the session survives, ADR 0011). No daemon nudge on logout:
//! the session *is* the lock holder, so no daemon can serve this
//! database (D10).

use std::time::Duration;

use taskboard_sync_nextcloud::DeckClient;

use crate::commands::connect::ConnectArgs;
use crate::commands::output::{ConnectReceipt, LogoutReceipt, SessionConnectorState};
use crate::commands::{CommandError, CommandIo, emit_to, map_deck_error, report_to};
use crate::config::AppConfig;
use crate::repl::ReplSession;

/// The in-session `connect` line (ADR 0014): ceremony → config refresh
/// → detach-first → attach → settle → receipt with `attached: true`.
/// Never exits the session; every failure is a typed envelope.
pub(crate) async fn connect_line(
    session: &mut ReplSession,
    app: &crate::App,
    args: ConnectArgs,
    io: &mut CommandIo<'_>,
) -> u8 {
    match connect_session(session, app, args, io).await {
        Ok(receipt) => {
            emit_to(
                io,
                &receipt,
                &crate::commands::connect::render_text(&receipt),
            );
            0
        }
        Err(err) => report_to(io, &err),
    }
}

/// The in-session `logout` line (ADR 0014, design §4.6): local
/// teardown → detach the session's own connector → config refresh →
/// receipt with the session connector outcome.
pub(crate) async fn logout_line(
    session: &mut ReplSession,
    app: &crate::App,
    io: &mut CommandIo<'_>,
) -> u8 {
    match logout_session(session, app, io).await {
        Ok(receipt) => {
            emit_to(
                io,
                &receipt,
                &crate::commands::logout::render_text(&receipt),
            );
            0
        }
        Err(err) => report_to(io, &err),
    }
}

/// The connect choreography, step by step (named steps; the orchestration
/// reads in design order). Until the ceremony succeeds, *nothing* about
/// the running core has changed.
async fn connect_session(
    session: &mut ReplSession,
    app: &crate::App,
    args: ConnectArgs,
    io: &mut CommandIo<'_>,
) -> Result<ConnectReceipt, CommandError> {
    let path = AppConfig::write_path(session.config_path.clone());

    // 1. The ceremony (chunk 2's body verbatim): guards → gather (the
    //    session's prompt reader answers through the editor handshake)
    //    → verify → persist → store. Prompts before this point have
    //    already ridden the handshake; a failure here is just a failed
    //    line.
    let raw = AppConfig::extract_raw(Some(&path)).map_err(CommandError::from)?;
    let mut receipt =
        crate::commands::connect::run(&path, raw, args, &mut session.prompts, &session.store)
            .await?;

    // 2. Refresh the session config view: the full validated figment
    //    resolution — the exact path that booted the session (env
    //    overlays included). The file is verified content, so a
    //    read-back failure is the degenerate case: the session stays
    //    honest-and-stale and refuses the attach.
    refresh_session_config(session, io)?;

    // 3. Detach-first when a connector is live (D3): a same-identity
    //    re-connect stores a *new* secret, and only detach + re-attach
    //    rebuilds the client; a re-point needs the swap anyway.
    //    `AttachError::AlreadyAttached` stays the primitive's guard for
    //    non-session callers — unreachable from here by construction.
    if app.connector_attached() {
        let outcome = app.detach_connector().await;
        tracing::info!(?outcome, "connect: swapped out the live connector");
    }

    // 4. Attach: build the client from the *refreshed* config + the
    //    session store; a typed attach refusal is recoverable (another
    //    `connect` line; the config is already refreshed).
    let client = build_client(session)?;
    app.attach_connector(client)
        .await
        .map_err(CommandError::from)?;

    // 5. Settle (D8): the boot wait's exact mirror — bounded,
    //    offline-first, degrade-to-local.
    let timeout = Duration::from_secs(app.config.app.shutdown_timeout_secs.max(1));
    crate::bootstrap::wait_for_boot_pull(app, timeout).await;

    receipt.attached = Some(true);
    Ok(receipt)
}

/// The logout choreography: the connect ceremony run backwards.
async fn logout_session(
    session: &mut ReplSession,
    app: &crate::App,
    io: &mut CommandIo<'_>,
) -> Result<LogoutReceipt, CommandError> {
    let path = AppConfig::write_path(session.config_path.clone());

    // 1. The local ceremony (chunk-2/WI-H's body verbatim):
    //    `NotConfigured` under standalone, secret delete, auto-key
    //    strip.
    let mut receipt = crate::commands::logout::run(&path, &session.config, session.store.as_ref())?;

    // 2. Detach the session's own connector: idempotent
    //    `NoConnector` on standalone boots — no new machinery.
    let detach = app.detach_connector().await;

    // 3. Refresh: the stripped auto install re-resolves standalone
    //    (the preludes refuse `sync-disabled` again); an explicit
    //    `nextcloud` install keeps its keys and stays `nextcloud`.
    refresh_session_config(session, io)?;

    // 4. Receipt: the honest sibling of the one-shot's
    //    `daemon_connector`, named for the session (D10: no nudge —
    //    the session holds the lock).
    receipt.session_connector = Some(match detach {
        crate::bootstrap::DetachOutcome::Detached => SessionConnectorState::Detached,
        crate::bootstrap::DetachOutcome::NoConnector => SessionConnectorState::NoConnector,
        crate::bootstrap::DetachOutcome::TimedOut => SessionConnectorState::TimedOut,
    });
    Ok(receipt)
}

/// Re-reads the session's config through the boot resolution. On the
/// degenerate read-back failure: the restart guidance (stderr UX —
/// the view is stale until a relaunch) plus the typed config error;
/// nothing rolls back.
fn refresh_session_config(
    session: &mut ReplSession,
    io: &mut CommandIo<'_>,
) -> Result<(), CommandError> {
    match AppConfig::load(session.config_path.clone()) {
        Ok(config) => {
            session.config = config;
            Ok(())
        }
        Err(err) => {
            let _ = writeln!(
                io.err,
                "the session's config view could not be refreshed; exit and relaunch to pick up \
                 the file"
            );
            Err(CommandError::from(err))
        }
    }
}

/// Builds the `DeckClient` for the attach from the refreshed config and
/// the session's credential store (the secret the ceremony just stored).
fn build_client(session: &ReplSession) -> Result<DeckClient, CommandError> {
    let server = session
        .config
        .nextcloud
        .server_url
        .clone()
        .ok_or(CommandError::NotConfigured)?;
    let user = session
        .config
        .nextcloud
        .username
        .clone()
        .ok_or(CommandError::NotConfigured)?;
    let password = session
        .store
        .load(&server, &user)
        .map_err(CommandError::Credential)?;
    DeckClient::new(&server, &user, password.expose()).map_err(map_deck_error)
}
