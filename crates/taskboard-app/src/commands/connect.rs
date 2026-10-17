// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard connect` (ADR 0009, design §3.1): the one-shot
//! connector-introduction ceremony — prompt/flag → guards → verify
//! against the server (`boards()` is the credential-verification call)
//! → persist non-secrets to the config TOML (comment-preserving via
//! `toml_edit`), the secret to the credential store. Nothing is
//! persisted unless verification passed. `taskboard login` is its
//! deprecated alias (dispatch-level; a strictly narrower flag surface).

use std::io::{ErrorKind as IoErrorKind, IsTerminal as _, Write as _};
use std::path::Path;
use std::sync::Arc;

use taskboard_sync_nextcloud::{DeckClient, DeckError};

use crate::commands::output::{ConnectReceipt, DaemonAttachState};
use crate::commands::{CommandError, map_deck_error};
use crate::config::{AppConfig, SyncBackend};
use crate::secrets::{AppPassword, CredentialError, CredentialStore};

/// The connect ceremony's flag bundle. `login`'s dispatch arm builds
/// the same bundle with `switch_backend = false, force = false,
/// confirmable = false` — the alias is parse/dispatch-level, not a
/// second body (one behavior, one test home).
#[derive(Debug, Clone, Default)]
pub struct ConnectArgs {
    /// Server URL; prompted when `None`.
    pub server: Option<String>,
    /// Login name; prompted when `None`.
    pub user: Option<String>,
    /// D2: the scripted loud backend flip (`sync.backend` explicit
    /// `standalone` → `nextcloud`).
    pub switch_backend: bool,
    /// D3: the scripted re-point escape (a configured different
    /// identity is replaced without the interactive confirm).
    pub force: bool,
    /// Whether the guards may escalate to an interactive y/N confirm.
    /// `connect` passes `true`; the deprecated `login` alias passes
    /// `false` — its surface is strictly narrower than its replacement
    /// (design D1): under an explicit `standalone` backend or a
    /// configured different identity it refuses, naming `connect`,
    /// instead of walking the user through a flip its own flags cannot
    /// express.
    pub confirmable: bool,
}

/// Input seam for interactive values (fakes over mocks; the E2E pipes
/// stdin through the production impl). `prompt` is the label the
/// production impl displays; the flag-supplied values skip prompting
/// entirely.
///
/// The methods are async-signatured (RPITIT; impls use `async fn`):
/// the session's [`crate::repl::ReplPrompts`] impl is a channel
/// round-trip to the editor thread — genuinely awaitable — and a sync
/// trait would force that impl to block a runtime worker for
/// unbounded user-thinking time (ADR 0011's founding anti-pattern on
/// a 1-core kiosk). Not dyn-compatible by design: the three call
/// sites (one-shot connect, login alias, session ceremony) all know
/// their concrete reader.
pub trait PromptReader: Send + Sync {
    /// One trimmed line (server URL / username).
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when stdin cannot be read.
    fn read_line(
        &mut self,
        prompt: &str,
    ) -> impl std::future::Future<Output = Result<String, std::io::Error>> + Send;

    /// A secret: on a terminal via `rpassword` with `*` feedback per
    /// keystroke (common CLI practice); on piped stdin a plain line
    /// (scripts and the E2E pipe the password).
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when the secret cannot be read.
    fn read_secret(
        &mut self,
        prompt: &str,
    ) -> impl std::future::Future<Output = Result<String, std::io::Error>> + Send;

    /// A y/N decision (D2/D3 guards). On a terminal: the label on
    /// stderr, one line read, `y`/`yes` (case-insensitive, trimmed) is
    /// consent. On non-terminal stdin: `Ok(false)` immediately — a
    /// piped or `--json` run must never block on an interactive
    /// decision, and the typed refusal (which names the flag) is the
    /// scripted answer.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when the confirm line cannot be read —
    /// mapped by the ceremony to [`CommandError::Prompt`] exactly like
    /// a failed identity prompt (a cancelled prompt is not a declined
    /// one; the REPL's Ctrl-C disposition relies on the distinction).
    fn read_confirm(
        &mut self,
        prompt: &str,
    ) -> impl std::future::Future<Output = Result<bool, std::io::Error>> + Send;
}

/// The production prompt reader. Prompts go to stderr, keeping stdout
/// a clean output channel for payloads. The `*_sync` cores are the
/// one terminal-impl body, shared with the REPL editor thread's
/// prompt answering (two call shapes, one grammar).
#[derive(Debug, Default)]
pub struct StdinPrompts;

/// The masked-feedback config: one `*` per typed character.
fn masked_password_config() -> rpassword::Config {
    rpassword::ConfigBuilder::new()
        .password_feedback_mask('*')
        .build()
}

impl StdinPrompts {
    /// The sync core of [`PromptReader::read_line`]: the label on
    /// stderr, one cooked line read, trimmed.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when stdin cannot be read.
    pub(crate) fn read_line_sync(prompt: &str) -> Result<String, std::io::Error> {
        let mut stderr = std::io::stderr();
        write!(stderr, "{prompt}")?;
        stderr.flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        Ok(line.trim().to_owned())
    }

    /// The sync core of [`PromptReader::read_secret`]: masked
    /// (`rpassword`) on a terminal, a plain line on piped stdin.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when the secret cannot be read.
    pub(crate) fn read_secret_sync(prompt: &str) -> Result<String, std::io::Error> {
        if std::io::stdin().is_terminal() {
            // `prompt_password_with_config` prompts and reads on the
            // TTY with echo off and `*` per keystroke.
            rpassword::prompt_password_with_config(prompt, masked_password_config())
                .map(|s| s.trim().to_owned())
        } else {
            // Piped stdin: no terminal to hide the echo on. Strip only
            // the line terminator — a secret may contain any other
            // character.
            let mut stderr = std::io::stderr();
            write!(stderr, "{prompt}")?;
            stderr.flush()?;
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            Ok(line.trim_end_matches(['\r', '\n']).to_owned())
        }
    }

    /// The sync core of [`PromptReader::read_confirm`]: the
    /// never-blocks rule (D4) keys on stdin TTY-ness alone — a piped
    /// or cron run gets `Ok(false)` immediately, never a read.
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when the confirm line cannot be read.
    pub(crate) fn read_confirm_sync(prompt: &str) -> Result<bool, std::io::Error> {
        if !std::io::stdin().is_terminal() {
            return Ok(false);
        }
        let mut stderr = std::io::stderr();
        write!(stderr, "{prompt}")?;
        stderr.flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        Ok(parse_confirm(&line))
    }
}

impl PromptReader for StdinPrompts {
    fn read_line(
        &mut self,
        prompt: &str,
    ) -> impl std::future::Future<Output = Result<String, std::io::Error>> + Send {
        std::future::ready(Self::read_line_sync(prompt))
    }

    fn read_secret(
        &mut self,
        prompt: &str,
    ) -> impl std::future::Future<Output = Result<String, std::io::Error>> + Send {
        std::future::ready(Self::read_secret_sync(prompt))
    }

    fn read_confirm(
        &mut self,
        prompt: &str,
    ) -> impl std::future::Future<Output = Result<bool, std::io::Error>> + Send {
        std::future::ready(Self::read_confirm_sync(prompt))
    }
}

/// Pure confirm grammar: trimmed, ASCII-case-insensitive `y`/`yes` is
/// consent; everything else (empty line, `n`, garbage) declines. One
/// shot — no re-prompt loop.
fn parse_confirm(line: &str) -> bool {
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// The gathered identity (flags | prompts).
struct Identity {
    server: String,
    user: String,
    password: String,
}

/// Runs the connect ceremony. `raw` is the pre-read, unvalidated config
/// at `config_path` (the dispatcher reads it once; `extract_raw`
/// tolerates a not-yet-valid config — a missing file feeds the empty
/// default).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs. Nothing is
/// persisted (no config write, no store call) unless the server
/// verified the credentials; a declined or unconfirmable guard refuses
/// before any identity is gathered.
pub async fn run<P: PromptReader>(
    config_path: &Path,
    raw: AppConfig,
    args: ConnectArgs,
    prompts: &mut P,
    store: &Arc<dyn CredentialStore>,
) -> Result<ConnectReceipt, CommandError> {
    // 1. D2 guard (before gathering: a declined flip must not cost the
    //    user a typed password; a refusal before prompting is today's
    //    pinned behavior, L9).
    let backend_switched = decide_backend_switch(&raw, &args, prompts).await?;

    // 2. Gather identity (flags | prompts).
    let identity = gather_identity(&args, prompts).await?;

    // 3. D3 guard: needs the gathered identity to compare.
    let repoint = decide_repoint(&raw, &identity, &args, prompts).await?;

    // 4. Verify before anything is persisted.
    verify(&identity).await?;

    // 5. Persist: overlay (server/user, + backend word iff switched),
    //    then the secret (store new; delete the replaced identity's
    //    secret iff re-point).
    persist(config_path, raw, &identity, backend_switched)?;
    let stored = store_secret(store, &identity, repoint.as_ref())?;

    tracing::info!(stored, "connect verified and persisted");
    Ok(ConnectReceipt {
        server_url: identity.server,
        username: identity.user,
        verified: true,
        stored,
        backend_switched,
        repointed: repoint.is_some(),
        attached: None,
        daemon_connector: None,
    })
}

/// The D2 guard (design §3.1): under an explicit
/// `sync.backend = "standalone"`, the flip to `nextcloud` must be
/// loud — the flag, or a confirmed y/N on a terminal, else the typed
/// refusal. Under every other backend word (explicit `nextcloud`,
/// auto) the guard is a no-op returning `false` — and `false` means
/// the overlay writes **no** backend key at all (D6: the auto install
/// stays auto, so the next `logout` can strip the identity keys).
async fn decide_backend_switch<P: PromptReader>(
    raw: &AppConfig,
    args: &ConnectArgs,
    prompts: &mut P,
) -> Result<bool, CommandError> {
    if raw.sync.backend != Some(SyncBackend::Standalone) {
        return Ok(false);
    }
    if args.switch_backend {
        return Ok(true);
    }
    if !args.confirmable {
        return Err(CommandError::BackendExplicitStandalone);
    }
    if prompts
        .read_confirm("Switch the backend from stand-alone to nextcloud? [y/N] ")
        .await
        .map_err(CommandError::Prompt)?
    {
        Ok(true)
    } else {
        Err(CommandError::BackendExplicitStandalone)
    }
}

/// Gathers the identity: the flags when present, else the three
/// labeled prompts.
async fn gather_identity<P: PromptReader>(
    args: &ConnectArgs,
    prompts: &mut P,
) -> Result<Identity, CommandError> {
    let server = match &args.server {
        Some(s) => s.clone(),
        None => prompts
            .read_line("Server URL (https://cloud.example.com): ")
            .await
            .map_err(CommandError::Prompt)?,
    };
    let user = match &args.user {
        Some(u) => u.clone(),
        None => prompts
            .read_line("Username: ")
            .await
            .map_err(CommandError::Prompt)?,
    };
    let password = prompts
        .read_secret("App password: ")
        .await
        .map_err(CommandError::Prompt)?;
    Ok(Identity {
        server,
        user,
        password,
    })
}

/// The D3 guard (design §3.1): a *complete* configured identity that
/// differs from the gathered one is replaced only loudly — `--force`,
/// or a confirmed y/N on a terminal, else the typed refusal. A
/// same-identity connect returns `None` and never prompts (the
/// idempotent re-verify); a half-configured file is treated as
/// unconfigured (D8).
async fn decide_repoint<P: PromptReader>(
    raw: &AppConfig,
    identity: &Identity,
    args: &ConnectArgs,
    prompts: &mut P,
) -> Result<Option<(String, String)>, CommandError> {
    let (Some(old_server), Some(old_user)) = (&raw.nextcloud.server_url, &raw.nextcloud.username)
    else {
        return Ok(None);
    };
    if *old_server == identity.server && *old_user == identity.user {
        return Ok(None);
    }
    if args.force {
        return Ok(Some((old_server.clone(), old_user.clone())));
    }
    if !args.confirmable {
        return Err(CommandError::ConnectionExists);
    }
    if prompts
        .read_confirm("A different connection is configured; re-point it? [y/N] ")
        .await
        .map_err(CommandError::Prompt)?
    {
        Ok(Some((old_server.clone(), old_user.clone())))
    } else {
        Err(CommandError::ConnectionExists)
    }
}

/// Verifies the credentials against the server (`boards()` is the
/// verification call) before anything is persisted.
async fn verify(identity: &Identity) -> Result<(), CommandError> {
    let client = DeckClient::new(&identity.server, &identity.user, &identity.password)
        .map_err(map_deck_error)?;
    match client.boards().await {
        Ok(_) => Ok(()),
        Err(err) => {
            tracing::info!(variant = deck_variant(&err), "connect verification failed");
            Err(map_deck_error(err))
        }
    }
}

/// Persists the non-secrets: the in-memory merge (backend word iff
/// switched, so the validated shape matches the file about to be
/// written), then the comment-preserving overlay.
fn persist(
    config_path: &Path,
    raw: AppConfig,
    identity: &Identity,
    backend_switched: bool,
) -> Result<(), CommandError> {
    let mut merged = raw;
    merged.nextcloud.server_url = Some(identity.server.clone());
    merged.nextcloud.username = Some(identity.user.clone());
    if backend_switched {
        merged.sync.backend = Some(SyncBackend::Nextcloud);
    }
    merged.validate().map_err(CommandError::from)?;
    write_config_overlay(
        config_path,
        &identity.server,
        &identity.user,
        backend_switched,
    )
    .map_err(CommandError::ConfigWrite)
}

/// Stores the new secret; on a re-point, deletes the replaced
/// identity's secret **after** the new store (D7 — a crash between
/// leaves an orphan, never a broken install). The deletion tolerates
/// `NotFound` (idempotent) and `Unsupported` (the env store has no
/// secret to orphan); any other failure is typed.
fn store_secret(
    store: &Arc<dyn CredentialStore>,
    identity: &Identity,
    repoint: Option<&(String, String)>,
) -> Result<String, CommandError> {
    // A read-only store (env) is a *successful* connect with
    // `stored: "env"`; no secret is ever written to any file.
    let stored = match store.store(
        &identity.server,
        &identity.user,
        AppPassword::new(identity.password.clone()),
    ) {
        Ok(()) => "keyring",
        Err(CredentialError::Unsupported) => "env",
        Err(err) => return Err(CommandError::Credential(err)),
    };
    if let Some((old_server, old_user)) = repoint {
        match store.delete(old_server, old_user) {
            Ok(()) | Err(CredentialError::NotFound | CredentialError::Unsupported) => {}
            Err(err) => return Err(CommandError::Credential(err)),
        }
    }
    Ok(stored.to_owned())
}

/// The human-readable one-line receipt.
#[must_use]
pub fn render_text(receipt: &ConnectReceipt) -> String {
    let base = match receipt.stored.as_str() {
        "env" => format!(
            "verified {} as {} (export TASKBOARD_APP_PASSWORD to store the secret)",
            receipt.server_url, receipt.username
        ),
        _ => format!(
            "verified {} as {} (secret stored in the OS keyring)",
            receipt.server_url, receipt.username
        ),
    };
    let base = if receipt.backend_switched {
        format!("{base} — backend switched from stand-alone")
    } else {
        base
    };
    let base = if receipt.repointed {
        format!("{base} — re-pointed")
    } else {
        base
    };
    if receipt.attached == Some(true) {
        format!("{base} — connector attached to the running session")
    } else if receipt.daemon_connector == Some(DaemonAttachState::Attached) {
        format!("{base} — the serving daemon attached it")
    } else {
        base
    }
}

/// Overlays `nextcloud.server_url`/`username` (and, iff
/// `switch_backend`, sets `sync.backend = "nextcloud"`) into the
/// config file via `toml_edit` read-modify-write (preserves comments,
/// formatting, and unknown tables — the file is operator territory),
/// writing atomically (temp file + rename). Creates the file when
/// absent; `toml_edit` creates the `[sync]` table when the rewrite
/// meets a file without one.
fn write_config_overlay(
    path: &Path,
    server: &str,
    user: &str,
    switch_backend: bool,
) -> Result<(), std::io::Error> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == IoErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| std::io::Error::new(IoErrorKind::InvalidData, err))?;
    doc["nextcloud"]["server_url"] = toml_edit::value(server);
    doc["nextcloud"]["username"] = toml_edit::value(user);
    if switch_backend {
        doc["sync"]["backend"] = toml_edit::value("nextcloud");
    }

    crate::config::write_owner_only_atomic(path, doc.to_string().as_str())
}

/// The hermetic input of the attach nudge's degradation mapping: the
/// post-connect daemon nudge's outcome, socket-free.
#[derive(Debug, Clone, PartialEq)]
pub enum AttachNudgeOutcome {
    /// No daemon serves this database — nothing to attach there; not a
    /// degradation (no warning).
    NoDaemon,
    /// The daemon answered the attach request.
    Answered(crate::ipc::protocol::AttachWireOutcome),
    /// Transport failure — a dead connection, a pre-`attach-connector`
    /// daemon (it cannot decode the variant and drops the link), or the
    /// client read budget expired (the daemon still completes its
    /// attach in that case).
    Transport,
    /// The daemon speaks a different wire-protocol version.
    Version,
}

/// Maps the nudge outcome onto the receipt's `daemon_connector` field
/// (pure; unit-pinned over every arm).
#[must_use]
pub fn map_attach_nudge(nudge: &AttachNudgeOutcome) -> DaemonAttachState {
    use crate::ipc::protocol::AttachWireOutcome as W;
    match nudge {
        AttachNudgeOutcome::NoDaemon => DaemonAttachState::None,
        AttachNudgeOutcome::Answered(W::Attached) => DaemonAttachState::Attached,
        AttachNudgeOutcome::Answered(W::IdentityMismatch) => DaemonAttachState::Mismatch,
        AttachNudgeOutcome::Transport => DaemonAttachState::Unreachable,
        // Every other answer is a daemon-side state the client cannot
        // remedy from here (the daemon's own config/store governs, or a
        // raced shutdown), and a foreign version is equally opaque: the
        // honest bucket is the warning. `already-attached` folds in —
        // the daemon reports a connector either way.
        AttachNudgeOutcome::Answered(_) | AttachNudgeOutcome::Version => DaemonAttachState::Unknown,
    }
}

/// The stderr warning for the degraded outcomes: the connect *is* done;
/// this tells the operator the daemon did not pick the connection up.
/// `None` for the clean arms (`None`/`Attached`).
#[must_use]
pub fn attach_nudge_warning(state: DaemonAttachState) -> Option<&'static str> {
    match state {
        DaemonAttachState::Mismatch => Some(
            "warning: the daemon serves this database under a different identity; \
             restart it or run `taskboard daemon stop`",
        ),
        DaemonAttachState::Unreachable => Some(
            "warning: the daemon serving this database could not be asked to attach; \
             restart it or run `taskboard daemon stop`",
        ),
        DaemonAttachState::Unknown => Some(
            "warning: the daemon could not attach the new connection; \
             restart it or run `taskboard daemon stop`",
        ),
        DaemonAttachState::Attached | DaemonAttachState::None => None,
    }
}

/// The best-effort daemon nudge (ADR 0014 decision 7): after the local
/// connect succeeded, ask the daemon serving the same database to
/// attach from persisted state — identity only, never the secret. Never
/// fails the connect; every outcome degrades through
/// [`map_attach_nudge`]/[`attach_nudge_warning`].
pub(crate) async fn nudge_daemon_attach(
    db_path: &Path,
    server: &str,
    username: &str,
) -> AttachNudgeOutcome {
    let Some(conn) = crate::ipc::client::try_connect(db_path).await else {
        return AttachNudgeOutcome::NoDaemon;
    };
    match conn.attach(server, username).await {
        Ok(outcome) => AttachNudgeOutcome::Answered(outcome),
        Err(CommandError::IpcVersion { .. }) => AttachNudgeOutcome::Version,
        Err(_) => AttachNudgeOutcome::Transport,
    }
}

/// The bare variant name of a `DeckError` (log-only; never the secret,
/// never a wire payload).
fn deck_variant(err: &DeckError) -> &'static str {
    match err {
        DeckError::InvalidBaseUrl => "InvalidBaseUrl",
        DeckError::InvalidUpload => "InvalidUpload",
        DeckError::Transport(_) => "Transport",
        DeckError::Envelope(_) => "Envelope",
        DeckError::Unauthorized => "Unauthorized",
        DeckError::Forbidden => "Forbidden",
        DeckError::NotFound => "NotFound",
        DeckError::BadRequest => "BadRequest",
        DeckError::Conflict => "Conflict",
        DeckError::PreconditionFailed => "PreconditionFailed",
        DeckError::RateLimited { .. } => "RateLimited",
        DeckError::Server(_) => "Server",
        DeckError::Unavailable => "Unavailable",
    }
}

#[cfg(test)]
mod tests {
    use super::{PromptReader as _, StdinPrompts, parse_confirm};

    // -- owner-only config modes (F12-01) ------------------------------------

    /// Asserts the mode bits recorded in the metadata view (never the
    /// access-as-another-user view, which root would defeat).
    #[cfg(unix)]
    fn owner_mode_is(path: &std::path::Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .expect("config exists")
            .permissions()
            .mode()
            & 0o777
            == 0o600
    }

    /// F12-01 (#297): a config file written by the connect overlay is
    /// owner read/write only (ADR 0012 — the file sits in the same
    /// trust story as the db and lock beside it).
    #[cfg(unix)]
    #[test]
    fn the_written_config_overlay_is_owner_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");

        super::write_config_overlay(&config, "https://c.example.com", "alice", false)
            .expect("overlay written");

        assert!(owner_mode_is(&config), "the written config must be 0600");
    }

    /// F12-01 (#297): every overlay rewrite re-establishes owner-only
    /// modes, so a pre-hardening (or otherwise loosened) config
    /// self-heals at the next ceremony.
    #[cfg(unix)]
    #[test]
    fn an_overlay_rewrite_tightens_a_preexisting_world_readable_config() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let config = dir.path().join("taskboard.toml");
        std::fs::write(&config, "[app]\nmode = \"desktop\"\n").expect("seed config");
        let mut perms = std::fs::metadata(&config).expect("meta").permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&config, perms).expect("loosen");

        super::write_config_overlay(&config, "https://c.example.com", "alice", false)
            .expect("overlay written");

        assert!(owner_mode_is(&config), "the rewrite must tighten to 0600");
    }

    /// The confirm grammar's truth table: y/yes in any case consent;
    /// everything else (empty, n, garbage, whitespace-padded) declines.
    #[test]
    fn parse_confirm_consents_only_to_y_and_yes() {
        for yes in ["y", "Y", "yes", "YES", "Yes", " y ", "\tyes\n"] {
            assert!(parse_confirm(yes), "{yes:?} must consent");
        }
        for no in ["", "n", "no", "N", "garbage", "yy", "0", " y y "] {
            assert!(!parse_confirm(no), "{no:?} must decline");
        }
    }

    /// The never-blocks rule (D4) at the production impl: nextest's
    /// stdin is non-terminal, so `read_confirm` answers `Ok(false)`
    /// immediately without reading — a blocking read would hang the
    /// test binary; the timely return is the proof. A piped `y` is
    /// never consumed and never waits.
    #[tokio::test]
    async fn stdin_prompts_read_confirm_declines_on_non_terminal_stdin() {
        let mut prompts = StdinPrompts;
        assert!(
            !prompts
                .read_confirm("probe: ")
                .await
                .expect("non-tty decline")
        );
    }

    /// The attach nudge's pure mapping covers every arm, and only the
    /// degraded arms warn (the no-daemon row is not a degradation — the
    /// connect is done either way). Socket-free.
    #[test]
    fn the_attach_nudge_mapping_covers_every_arm() {
        use super::{AttachNudgeOutcome as N, attach_nudge_warning, map_attach_nudge};
        use crate::commands::output::DaemonAttachState as S;
        use crate::ipc::protocol::AttachWireOutcome as W;

        assert_eq!(map_attach_nudge(&N::NoDaemon), S::None);
        assert_eq!(map_attach_nudge(&N::Answered(W::Attached)), S::Attached);
        assert_eq!(
            map_attach_nudge(&N::Answered(W::IdentityMismatch)),
            S::Mismatch
        );
        assert_eq!(map_attach_nudge(&N::Transport), S::Unreachable);
        for opaque in [
            W::AlreadyAttached,
            W::NotConfigured,
            W::CredentialUnavailable,
            W::Engine,
            W::Draining,
        ] {
            assert_eq!(
                map_attach_nudge(&N::Answered(opaque)),
                S::Unknown,
                "{opaque:?} is the daemon-side bucket"
            );
        }
        assert_eq!(map_attach_nudge(&N::Version), S::Unknown);

        assert!(attach_nudge_warning(S::Attached).is_none());
        assert!(attach_nudge_warning(S::None).is_none());
        assert!(attach_nudge_warning(S::Mismatch).is_some());
        assert!(attach_nudge_warning(S::Unreachable).is_some());
        assert!(attach_nudge_warning(S::Unknown).is_some());
    }

    /// The receipt's `daemon_connector` field serializes kebab-case and
    /// is absent when no daemon was consulted (the pure-local shape
    /// stays stable).
    #[test]
    fn the_daemon_connector_field_serializes_kebab_case_or_is_absent() {
        use crate::commands::output::DaemonAttachState;
        let base = |state| crate::commands::output::ConnectReceipt {
            server_url: "https://c.example.com".to_owned(),
            username: "alice".to_owned(),
            verified: true,
            stored: "keyring".to_owned(),
            backend_switched: false,
            repointed: false,
            attached: None,
            daemon_connector: state,
        };
        let json =
            serde_json::to_value(base(Some(DaemonAttachState::Attached))).expect("serialize");
        assert_eq!(json["daemon_connector"], "attached");
        let json = serde_json::to_value(base(Some(DaemonAttachState::Unknown))).expect("serialize");
        assert_eq!(json["daemon_connector"], "unknown");
        let json = serde_json::to_value(base(None)).expect("serialize");
        assert!(json.get("daemon_connector").is_none());
    }

    /// ADR 0014 §7: the one-shot receipt's JSON shape carries
    /// `attached` absent (the session ceremony adds `"attached":true`).
    #[test]
    fn the_attached_field_is_absent_for_the_one_shot_and_true_for_the_session() {
        let base = |attached| crate::commands::output::ConnectReceipt {
            server_url: "https://c.example.com".to_owned(),
            username: "alice".to_owned(),
            verified: true,
            stored: "keyring".to_owned(),
            backend_switched: false,
            repointed: false,
            attached,
            daemon_connector: None,
        };
        let json = serde_json::to_value(base(None)).expect("serialize");
        assert!(json.get("attached").is_none());
        let json = serde_json::to_value(base(Some(true))).expect("serialize");
        assert_eq!(json["attached"], true);
    }
}
