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

use std::sync::Arc;
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

    // 3. Detach-first when the default connector is live (D3, D5-16):
    //    a same-identity re-connect stores a *new* secret, and only
    //    detach + re-attach rebuilds the client; a re-point needs the
    //    swap anyway. The named swap detaches only the default — a
    //    second live connector survives the rewrite of entry 0.
    let default_id = default_connector_id(&session.config)?;
    if let Some(id) = &default_id
        && app.connector_attached()
    {
        let outcome = app.detach_connector_named(id).await;
        tracing::info!(?outcome, connector = %id, "connect: swapped out the default connector");
    }

    // 4. Attach under the refreshed config's default id: the client is
    //    built from the default spec + the session store; a typed
    //    attach refusal is recoverable (another `connect` line; the
    //    config is already refreshed).
    let spec = default_spec(&session.config)?;
    let client = deck_client_from_spec(&spec, &session.store)?;
    let id = spec.name.clone();
    app.attach_connector(id, client)
        .await
        .map_err(CommandError::from)?;

    // 5. Settle (D8): the boot wait's exact mirror — bounded,
    //    offline-first, degrade-to-local.
    let timeout = Duration::from_secs(app.config.app.shutdown_timeout_secs.max(1));
    crate::bootstrap::wait_for_boot_pull(&app.engine, timeout).await;

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

/// The session's default connector id off the refreshed config (the
/// first spec; `None` in a zero-connector world — a `connect` that left
/// the core standalone has nothing to swap).
fn default_connector_id(
    config: &AppConfig,
) -> Result<Option<taskboard_domain::ConnectorId>, CommandError> {
    Ok(crate::connectors::configured_connectors(config)
        .map_err(CommandError::from)?
        .first()
        .map(|spec| spec.name.clone()))
}

/// The refreshed config's default connector spec (D5-14: the session's
/// `build_client` and the daemon-side attach share it).
pub(crate) fn default_spec(
    config: &AppConfig,
) -> Result<crate::connectors::ConnectorSpec, CommandError> {
    crate::connectors::configured_connectors(config)
        .map_err(CommandError::from)?
        .into_iter()
        .next()
        .ok_or(CommandError::NotConfigured)
}

/// The frontend-agnostic client construction (identity off the
/// connector spec, secret off the credential store): the REPL builder's
/// core, shared with the daemon-side attach — one behavior, one home.
pub(crate) fn deck_client_from_spec(
    spec: &crate::connectors::ConnectorSpec,
    store: &Arc<dyn crate::secrets::CredentialStore>,
) -> Result<DeckClient, CommandError> {
    let server = spec.server_url.clone();
    let user = spec.username.clone();
    let password = store
        .load(&server, &user)
        .map_err(CommandError::Credential)?;
    DeckClient::new(&server, &user, password.expose()).map_err(map_deck_error)
}

/// The daemon-side attach from persisted state (ADR 0014 decision 7,
/// the IPC `AttachConnector` verb's body): re-read the daemon's own
/// config, match the requested identity, build the client off the
/// daemon's credential store, swap-first (the session's detach-first
/// rule — a same-identity re-connect stores a *new* secret, and only
/// detach + re-attach rebuilds the client), attach, and settle with the
/// boot pull's exact mirror. Every failure is a typed wire outcome; a
/// pre-swap failure never leaves the daemon detached.
pub(crate) async fn attach_from_persisted(
    app: &crate::App,
    config_path: Option<&std::path::Path>,
    store: &Arc<dyn crate::secrets::CredentialStore>,
    server_url: &str,
    username: &str,
    name: Option<&str>,
) -> crate::ipc::protocol::AttachWireOutcome {
    use crate::bootstrap::AttachError;
    use crate::ipc::protocol::AttachWireOutcome as W;

    // 1. Re-read: the daemon's boot config path, env overlays resolved
    //    against the daemon's own environment.
    let config = match AppConfig::load(config_path.map(std::path::Path::to_path_buf)) {
        Ok(config) => config,
        Err(err) => {
            tracing::warn!(?err, "attach: the daemon's config did not re-read");
            return W::NotConfigured;
        }
    };

    // 2. Target resolution (D5-14): the entry named `name`, else the
    //    first (the default connector). No such name ⇒ `NotConfigured`.
    //    The request's identity must equal the target entry's pair —
    //    the identity check survives as the guard that the client
    //    connected *that* connector.
    let specs = match crate::connectors::configured_connectors(&config) {
        Ok(specs) => specs,
        Err(err) => {
            tracing::warn!(
                ?err,
                "attach: the daemon's config resolves no connector set"
            );
            return W::NotConfigured;
        }
    };
    let target = match name {
        Some(name) => specs
            .iter()
            .find(|spec| spec.name.as_str() == name)
            .or_else(|| {
                tracing::warn!(connector = %name, "attach: the config names no such connector");
                None
            }),
        None => specs.first(),
    };
    let Some(target) = target else {
        tracing::warn!("attach: the daemon's config resolves no connector");
        return W::NotConfigured;
    };
    if target.server_url != server_url || target.username != username {
        tracing::warn!(
            requested = %server_url,
            persisted = %target.server_url,
            "attach requested for a foreign identity; refusing"
        );
        return W::IdentityMismatch;
    }
    let target_id = target.name.clone();
    let target = target.clone();

    // 3. Build before any swap: the only post-swap failures are a raced
    //    shutdown — a failure the daemon's own state could have
    //    prevented never strands it detached. A credential miss is the
    //    env-store caveat's typed surface; every other construction
    //    failure is malformed persisted state (the identity was already
    //    extracted), logged for diagnosis.
    let client = match deck_client_from_spec(&target, store) {
        Ok(client) => client,
        Err(CommandError::Credential(err)) => {
            tracing::warn!(
                ?err,
                "attach: the daemon's store has no secret for the identity"
            );
            return W::CredentialUnavailable;
        }
        Err(err) => {
            tracing::warn!(?err, "attach: the daemon's persisted state built no client");
            return W::NotConfigured;
        }
    };

    // 4. Swap-first (the detach-first session rule, applied daemon-side,
    //    named): only the target connector stops — a second live
    //    connector survives the swap.
    if app.connector_attached() {
        let outcome = app.detach_connector_named(&target_id).await;
        tracing::info!(?outcome, connector = %target_id, "attach: swapped out the target connector");
    }

    // 5. Attach under the target's id: the name-in-use guard is
    //    unreachable here by construction (the dispatcher is serial and
    //    this handler just swapped); carried for wire honesty if
    //    dispatch ever widens.
    if let Err(err) = app.attach_connector(target_id.clone(), client).await {
        tracing::warn!(?err, "attach: the primitive refused the connector");
        return match err {
            AttachError::ConnectorNameInUse(_) => W::AlreadyAttached,
            AttachError::Engine(_) => W::Engine,
            AttachError::Draining => W::Draining,
            // The attach primitive builds no client (D-7): the module
            // factory is the boot path's constructor, so these build
            // failures cannot originate here.
            AttachError::Credential(_) | AttachError::Client(_) => {
                unreachable!("attach spawns over a ceremony-built client")
            }
        };
    }

    // 6. Settle: the REPL's exact mirror — bounded, offline-first,
    //    degrade-to-local; freshness rides later poll cycles.
    let timeout = Duration::from_secs(app.config.app.shutdown_timeout_secs.max(1));
    crate::bootstrap::wait_for_boot_pull(&app.engine, timeout).await;
    W::Attached
}
