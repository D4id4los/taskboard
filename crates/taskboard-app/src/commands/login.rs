// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard login` (ADR 0009): the first-run bootstrap command —
//! prompt/flag → verify against the server (`boards()` is the
//! credential-verification call) → persist non-secrets to the config
//! TOML (comment-preserving via `toml_edit`), the secret to the
//! credential store. Nothing is persisted unless verification passed.

use std::io::{ErrorKind as IoErrorKind, IsTerminal as _, Write as _};
use std::path::Path;
use std::sync::Arc;

use taskboard_sync_nextcloud::{DeckClient, DeckError};

use crate::commands::output::LoginReceipt;
use crate::commands::{CommandError, map_deck_error};
use crate::config::AppConfig;
use crate::secrets::{AppPassword, CredentialError, CredentialStore};

/// Input seam for interactive values (fakes over mocks; the E2E pipes
/// stdin through the production impl). `prompt` is the label the
/// production impl displays; the flag-supplied values skip prompting
/// entirely.
pub trait PromptReader: Send + Sync {
    /// One trimmed line (server URL / username).
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when stdin cannot be read.
    fn read_line(&mut self, prompt: &str) -> Result<String, std::io::Error>;

    /// A secret: on a terminal via `rpassword` with `*` feedback per
    /// keystroke (common CLI practice); on piped stdin a plain line
    /// (scripts and the E2E pipe the password).
    ///
    /// # Errors
    ///
    /// [`std::io::Error`] when the secret cannot be read.
    fn read_secret(&mut self, prompt: &str) -> Result<String, std::io::Error>;
}

/// The production prompt reader. Prompts go to stderr, keeping stdout
/// a clean output channel for payloads.
#[derive(Debug, Default)]
pub struct StdinPrompts;

/// The masked-feedback config: one `*` per typed character.
fn masked_password_config() -> rpassword::Config {
    rpassword::ConfigBuilder::new()
        .password_feedback_mask('*')
        .build()
}

impl PromptReader for StdinPrompts {
    fn read_line(&mut self, prompt: &str) -> Result<String, std::io::Error> {
        let mut stderr = std::io::stderr();
        write!(stderr, "{prompt}")?;
        stderr.flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        Ok(line.trim().to_owned())
    }

    fn read_secret(&mut self, prompt: &str) -> Result<String, std::io::Error> {
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
}

/// Runs the login ceremony. `raw` is the pre-read, unvalidated config at
/// `config_path` (the dispatcher reads it once; `extract_raw` tolerates
/// a not-yet-valid config — a missing file feeds the empty default).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs. Nothing is
/// persisted (no config write, no store call) unless the server
/// verified the credentials.
pub async fn run(
    config_path: &Path,
    raw: AppConfig,
    server: Option<String>,
    user: Option<String>,
    prompts: &mut dyn PromptReader,
    store: &Arc<dyn CredentialStore>,
) -> Result<LoginReceipt, CommandError> {
    let server = match server {
        Some(s) => s,
        None => prompts
            .read_line("Server URL (https://cloud.example.com): ")
            .map_err(CommandError::Prompt)?,
    };
    let user = match user {
        Some(u) => u,
        None => prompts
            .read_line("Username: ")
            .map_err(CommandError::Prompt)?,
    };
    let password = prompts
        .read_secret("App password: ")
        .map_err(CommandError::Prompt)?;

    // Verify before anything is persisted.
    let client = DeckClient::new(&server, &user, &password).map_err(map_deck_error)?;
    match client.boards().await {
        Ok(_) => {}
        Err(err) => {
            tracing::info!(variant = deck_variant(&err), "login verification failed");
            return Err(map_deck_error(err));
        }
    }

    // Persist the non-secrets: overlay into the existing file,
    // comment- and unknown-table-preserving, then re-validate.
    let mut merged = raw;
    merged.nextcloud.server_url = Some(server.clone());
    merged.nextcloud.username = Some(user.clone());
    merged.validate().map_err(CommandError::from)?;
    write_config_overlay(config_path, &server, &user).map_err(CommandError::ConfigWrite)?;

    // Then the secret. A read-only store (env) is a *successful* login
    // with `stored: "env"`; no secret is ever written to any file.
    let stored = match store.store(&server, &user, AppPassword::new(password)) {
        Ok(()) => "keyring",
        Err(CredentialError::Unsupported) => "env",
        Err(err) => return Err(CommandError::Credential(err)),
    };

    tracing::info!(stored, "login verified and persisted");
    Ok(LoginReceipt {
        server_url: server,
        username: user,
        verified: true,
        stored: stored.to_owned(),
    })
}

/// The human-readable one-line receipt.
#[must_use]
pub fn render_text(receipt: &LoginReceipt) -> String {
    match receipt.stored.as_str() {
        "env" => format!(
            "verified {} as {} (export TASKBOARD_APP_PASSWORD to store the secret)",
            receipt.server_url, receipt.username
        ),
        _ => format!(
            "verified {} as {} (secret stored in the OS keyring)",
            receipt.server_url, receipt.username
        ),
    }
}

/// Overlays `nextcloud.server_url`/`username` into the config file via
/// `toml_edit` read-modify-write (preserves comments, formatting, and
/// unknown tables — the file is operator territory), writing
/// atomically (temp file + rename). Creates the file when absent.
fn write_config_overlay(path: &Path, server: &str, user: &str) -> Result<(), std::io::Error> {
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

/// Contract sketch: scripted prompts drive the same code path the
/// binary uses; nothing is persisted unless the server verified the
/// credentials.
///
/// ```
/// use std::sync::Arc;
///
/// use taskboard_app::commands::login::{PromptReader, StdinPrompts};
/// use taskboard_app::secrets::CredentialStore;
///
/// // A scripted [`PromptReader`] (the test fake answers from a queue);
/// // production uses [`StdinPrompts`].
/// fn scripted(answers: Vec<String>) -> impl PromptReader {
///     struct S(std::sync::Mutex<std::vec::IntoIter<String>>);
///     impl PromptReader for S {
///         fn read_line(&mut self, _: &str) -> Result<String, std::io::Error> {
///             Ok(self.0.lock().unwrap().next().unwrap_or_default())
///         }
///         fn read_secret(&mut self, _: &str) -> Result<String, std::io::Error> {
///             Ok(self.0.lock().unwrap().next().unwrap_or_default())
///         }
///     }
///     S(std::sync::Mutex::new(answers.into_iter()))
/// }
///
/// // `store: Arc<dyn CredentialStore>` would be a fake in tests; the
/// // binary picks the keyring or env store from the config:
/// // let receipt = run(&config_path, raw_config, None, None,
/// //                   &mut scripted(vec![…]), &store).await?;
/// fn main() {
///     let _ = StdinPrompts;
///     let _ = Arc::new(0u8);
/// }
/// ```
#[allow(dead_code)]
fn prompt_reader_doc() {}
