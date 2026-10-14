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

use crate::commands::output::ConnectReceipt;
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
    if receipt.repointed {
        format!("{base} — re-pointed")
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

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("taskboard-tmp");
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(doc.to_string().as_bytes())?;
        file.sync_all().ok();
    }
    std::fs::rename(&tmp, path)
}

/// The connect-while-daemon heads-up (design §6/D11): whether a daemon
/// serves this database — a best-effort per-database IPC probe; a
/// transport failure answers `false` (a dead socket sidecar means
/// nothing serves — silent, not degraded; the same reading as
/// `NudgeOutcome::NoDaemon`).
pub(crate) async fn daemon_serves(db_path: &Path) -> bool {
    crate::ipc::client::try_connect(db_path).await.is_some()
}

/// The stderr warning for a served database (D11): the connect *is*
/// done; this tells the operator the daemon keeps running
/// connector-less until restarted. `None` when nothing serves —
/// `--json` runs included (stdout stays one payload; warnings are the
/// established stderr channel).
#[must_use]
pub fn daemon_warning(present: bool) -> Option<&'static str> {
    present.then_some(
        "warning: a daemon is serving this database; it keeps running without \
         the new connection until restarted (or `taskboard daemon stop`)",
    )
}

/// The bare variant name of a `DeckError` (log-only; never the secret,
/// never a wire payload).
fn deck_variant(err: &DeckError) -> &'static str {
    match err {
        DeckError::InvalidBaseUrl => "InvalidBaseUrl",
        DeckError::Transport(_) => "Transport",
        DeckError::Envelope(_) => "Envelope",
        DeckError::Unauthorized => "Unauthorized",
        DeckError::Forbidden => "Forbidden",
        DeckError::NotFound => "NotFound",
        DeckError::BadRequest => "BadRequest",
        DeckError::Conflict => "Conflict",
        DeckError::PreconditionFailed => "PreconditionFailed",
        DeckError::RateLimited => "RateLimited",
        DeckError::Server(_) => "Server",
        DeckError::Unavailable => "Unavailable",
    }
}

#[cfg(test)]
mod tests {
    use super::{PromptReader as _, StdinPrompts, parse_confirm};

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

    /// D11: the daemon heads-up warns only when a daemon actually
    /// serves the database (structural both-arms pin; the text is UX,
    /// never asserted).
    #[test]
    fn daemon_warning_covers_both_arms() {
        assert!(super::daemon_warning(true).is_some());
        assert!(super::daemon_warning(false).is_none());
    }
}
