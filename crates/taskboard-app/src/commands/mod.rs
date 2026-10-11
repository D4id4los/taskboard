// SPDX-License-Identifier: AGPL-3.0-only
//! The operator-facing command handlers (ADR 0009): one module per
//! subcommand group plus [`output`] — typed handlers returning typed
//! outcomes, all lib code so the L/V/K/O-series test suites drive them
//! hermetically. `main` stays a thin dispatch + exit-code shell.
//!
//! Two command classes:
//!
//! - **Pre-core** (`login`, `boards list`): config + a [`DeckClient`]
//!   only — no actor graph, no database, no instance lock.
//! - **Core-booting** (`boards select`, `tasks *`, `sync`): one full
//!   [`bootstrap`](crate::bootstrap) per invocation — boot, work,
//!   `shutdown()`, exit (roadmap §2.4: one bootstrap, N frontends).

pub mod boards;
pub mod login;
pub mod output;
pub mod stacks;
pub mod sync;
pub mod tasks;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::SyncErrorKind;
use taskboard_sync_nextcloud::DeckError;

use crate::bootstrap::BootstrapError;
use crate::cli::{BoardsCmd, Command, StacksCmd, TasksCmd};
use crate::config::{AppConfig, CredentialStoreKind, SyncBackend};
use crate::secrets::{CredentialError, CredentialStore};

/// The connector-only gate (ADR 0010 decision 9): runs at handler entry
/// — after config load, before any core boot or client construction —
/// so connector verbs fail loudly under `standalone`.
/// # Errors
///
/// [`CommandError::SyncDisabled`] when the resolved backend is
/// `standalone`.
pub fn require_connector(config: &AppConfig) -> Result<(), CommandError> {
    if config.sync_backend() == SyncBackend::Standalone {
        return Err(CommandError::SyncDisabled);
    }
    Ok(())
}

/// Everything a handler needs; built by `main` once per invocation.
pub struct CommandCtx {
    /// `--json`: machine-readable payloads on stdout, error JSON on
    /// stderr.
    pub json: bool,
    /// The resolved config path (by [`crate::config::config_path`]).
    pub config_path: Option<PathBuf>,
}

/// Typed failure of one CLI invocation. `Display` is UX text (never
/// asserted); the `--json` discriminator comes from
/// [`CommandError::error_kind`].
///
/// The error-crate payload variants are boxed: the CLI handlers all
/// return this error, and the figment/sqlx/reqwest sources inside it
/// would otherwise dominate every `Result` layout (clippy
/// `result_large_err`).
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// The configuration was rejected.
    #[error("configuration rejected")]
    Config(Box<crate::config::ConfigError>),
    /// The in-process core failed to boot.
    #[error("core bootstrap failed")]
    Bootstrap(Box<BootstrapError>),
    /// The credential store failed.
    #[error("credential store failed")]
    Credential(#[from] CredentialError),
    /// The engine rejected a command.
    #[error("engine rejected the command")]
    Engine(Box<taskboard_state::ExecuteError>),
    /// The server rejected the credentials (401/403).
    #[error("credentials rejected by the server")]
    InvalidCredentials,
    /// The server could not be reached.
    #[error("server unreachable")]
    Unreachable,
    /// No such (live, non-archived) board on the server.
    #[error("no such board")]
    UnknownBoard(u64),
    /// A different board is already bound (single-board invariant).
    #[error("a different board is already bound")]
    BoardAlreadyBound {
        /// The currently bound remote board id.
        current: u64,
    },
    /// No task matches the selector.
    #[error("no such task")]
    UnknownTask(String),
    /// The task selector matches several live tasks; carries their
    /// titles (deterministic board order).
    #[error("task selector matches several tasks — pass one of {}", candidates.join(" | "))]
    AmbiguousTask {
        /// The matched tasks' titles.
        candidates: Vec<String>,
    },
    /// No stack matches the selector.
    #[error("no such stack")]
    UnknownStack(String),
    /// The stack selector matches several stacks; carries the live
    /// stack titles the user can pass to `--stack`.
    #[error("stack selector matches several stacks — pass one of --stack <{}>", candidates.join(" | "))]
    AmbiguousStack {
        /// The live stack titles (deterministic board order).
        candidates: Vec<String>,
    },
    /// There is no stack to target.
    #[error("no stacks to target")]
    NoStacks,
    /// The sync cycle failed with the given classification.
    #[error("sync cycle failed")]
    SyncFailed(SyncErrorKind),
    /// The sync cycle (or board adoption) did not finish in time.
    #[error("sync did not finish in time")]
    SyncTimedOut,
    /// The config file could not be written.
    #[error("config file could not be written")]
    ConfigWrite(#[source] std::io::Error),
    /// An interactive prompt could not be read.
    #[error("prompt read failed")]
    Prompt(#[source] std::io::Error),
    /// A Deck API error outside the credential/reachability classes.
    #[error("deck api error")]
    Deck(Box<DeckError>),
    /// No sync connector is configured (resolved `standalone` backend):
    /// the connector-only verbs refuse loudly instead of hanging or
    /// silently doing local work (ADR 0010).
    #[error("no sync connector configured")]
    SyncDisabled,
    /// A live local-only board exists: plain `boards select` would
    /// silently adopt alongside it; the operator must choose the
    /// migration path (`--bind-local-board` or `boards publish`).
    #[error("a local board exists — pass --bind-local-board or use boards publish")]
    LocalBoardExists,
}

impl From<crate::config::ConfigError> for CommandError {
    fn from(err: crate::config::ConfigError) -> Self {
        Self::Config(Box::new(err))
    }
}

impl From<BootstrapError> for CommandError {
    fn from(err: BootstrapError) -> Self {
        Self::Bootstrap(Box::new(err))
    }
}

impl From<taskboard_state::ExecuteError> for CommandError {
    fn from(err: taskboard_state::ExecuteError) -> Self {
        Self::Engine(Box::new(err))
    }
}

impl From<DeckError> for CommandError {
    fn from(err: DeckError) -> Self {
        Self::Deck(Box::new(err))
    }
}

impl CommandError {
    /// The stable kebab-case discriminator for the `--json` error shape.
    /// Exhaustive: the compiler forces every new variant to pick a kind.
    #[must_use]
    pub fn error_kind(&self) -> &'static str {
        match self {
            Self::Config(_) => "config",
            Self::Bootstrap(err) => match **err {
                BootstrapError::InstanceLocked { .. } => "instance-locked",
                _ => "bootstrap",
            },
            Self::Credential(_) => "credential",
            Self::Engine(err) => match &**err {
                taskboard_state::ExecuteError::Rejected(rejected) => domain_error_kind(rejected),
                _ => "engine",
            },
            Self::InvalidCredentials => "invalid-credentials",
            Self::Unreachable => "unreachable",
            Self::UnknownBoard(_) => "unknown-board",
            Self::BoardAlreadyBound { .. } => "board-already-bound",
            Self::UnknownTask(_) => "unknown-task",
            Self::AmbiguousTask { .. } => "ambiguous-task",
            Self::UnknownStack(_) => "unknown-stack",
            Self::AmbiguousStack { .. } => "ambiguous-stack",
            Self::NoStacks => "no-stacks",
            Self::SyncFailed(_) => "sync-failed",
            Self::SyncTimedOut => "sync-timed-out",
            Self::ConfigWrite(_) => "config-write",
            Self::Prompt(_) => "prompt",
            Self::Deck(_) => "deck",
            Self::SyncDisabled => "sync-disabled",
            Self::LocalBoardExists => "local-board-exists",
        }
    }

    /// The optional structural detail (ids, nested kinds) of the
    /// `--json` error shape.
    #[must_use]
    pub fn error_detail(&self) -> Option<String> {
        match self {
            Self::UnknownBoard(id) => Some(id.to_string()),
            Self::BoardAlreadyBound { current } => Some(current.to_string()),
            Self::UnknownTask(sel) | Self::UnknownStack(sel) => Some(sel.clone()),
            Self::AmbiguousTask { candidates } | Self::AmbiguousStack { candidates } => {
                Some(candidates.join(", "))
            }
            Self::SyncFailed(kind) => Some(output::sync_error_kind(*kind).to_owned()),
            _ => None,
        }
    }
}

/// The engine's semantic rejections keep their own kinds in the JSON
/// error shape (`no-board`, `unknown-task`, …); everything else is the
/// generic `engine`.
fn domain_error_kind(err: &taskboard_domain::CommandError) -> &'static str {
    use taskboard_domain::CommandError as D;
    match err {
        D::NoBoard => "no-board",
        D::UnknownTask(_) | D::TaskDeleted(_) => "unknown-task",
        D::UnknownStack(_) | D::StackDeleted(_) => "unknown-stack",
        D::UnknownLabel(_) | D::LabelDeleted(_) => "unknown-label",
        D::BoardExists => "board-exists",
        D::BoardAlreadyBound(_) => "board-already-bound",
    }
}

/// The production credential store for a config-selected kind. Tests
/// inject fakes by calling the handlers directly.
#[must_use]
pub fn production_store(kind: CredentialStoreKind) -> Arc<dyn CredentialStore> {
    match kind {
        CredentialStoreKind::Keyring => Arc::new(crate::secrets::KeyringStore),
        CredentialStoreKind::Env => Arc::new(crate::secrets::EnvCredentialStore),
    }
}

/// Maps a [`DeckError`] onto the CLI failure classes: 401/403 are the
/// bad-credential signals (some servers answer 403 for bad tokens);
/// transport failures are reachability; everything else passes through
/// by variant.
pub(crate) fn map_deck_error(err: DeckError) -> CommandError {
    match err {
        DeckError::Unauthorized | DeckError::Forbidden => CommandError::InvalidCredentials,
        DeckError::Transport(_) => CommandError::Unreachable,
        other => CommandError::Deck(Box::new(other)),
    }
}

/// One subcommand run to completion. Returns the process exit code:
/// `0` success (including idempotent `NoOp` repeats), `1` runtime
/// failure. Exit `2` (clap usage errors) and `130` (second-signal force
/// exit) never flow through here.
#[allow(clippy::too_many_lines)] // one arm per subcommand, kept flat for review
pub async fn dispatch(command: Command, ctx: CommandCtx) -> u8 {
    let json = ctx.json;
    tracing::info!(?command, config = ?ctx.config_path, json, "running command");
    match command {
        Command::Login { server, user } => {
            let path = AppConfig::config_path(ctx.config_path)
                .unwrap_or_else(|| PathBuf::from(crate::config::DEFAULT_CONFIG_PATH));
            match AppConfig::extract_raw(Some(&path)) {
                Ok(raw) => {
                    // Explicit `standalone` refuses before any prompting
                    // or store touch; auto behaves as today (and after
                    // login writes `server_url`, auto resolves
                    // `nextcloud`).
                    if raw.sync.backend == Some(SyncBackend::Standalone) {
                        return report(json, &CommandError::SyncDisabled);
                    }
                    let store = production_store(raw.nextcloud.credential_store);
                    let mut prompts = login::StdinPrompts;
                    match login::run(&path, raw, server, user, &mut prompts, &store).await {
                        Ok(receipt) => {
                            emit(json, &receipt, &login::render_text(&receipt));
                            0
                        }
                        Err(err) => report(json, &err),
                    }
                }
                // A not-yet-valid config must not block login (first-run
                // bootstrap command); undecodable ones still fail.
                Err(err) => report(json, &CommandError::from(err)),
            }
        }
        Command::Boards {
            cmd: BoardsCmd::List,
        } => match core_ctx(&ctx) {
            Ok((config, store)) => match boards::list(&config, &store).await {
                Ok(entries) => {
                    emit(json, &entries, &boards::render_text_list(&entries));
                    0
                }
                Err(err) => report(json, &err),
            },
            Err(err) => report(json, &err),
        },
        Command::Boards {
            cmd:
                BoardsCmd::Select {
                    board,
                    timeout_secs,
                    bind_local_board,
                },
        } => match core_ctx(&ctx) {
            Ok((config, store)) => {
                match boards::select(
                    config,
                    store,
                    board,
                    Duration::from_secs(timeout_secs),
                    bind_local_board,
                )
                .await
                {
                    Ok(receipt) => {
                        emit(json, &receipt, &boards::render_text_select(&receipt));
                        0
                    }
                    Err(err) => report(json, &err),
                }
            }
            Err(err) => report(json, &err),
        },
        Command::Boards {
            cmd: BoardsCmd::Create { title, color },
        } => match core_ctx(&ctx) {
            Ok((config, store)) => {
                match boards::create(config, store, &title, color.as_deref()).await {
                    Ok(receipt) => {
                        emit(json, &receipt, &boards::render_text_create(&receipt));
                        0
                    }
                    Err(err) => report(json, &err),
                }
            }
            Err(err) => report(json, &err),
        },
        Command::Boards {
            cmd: BoardsCmd::Rename { title },
        } => match core_ctx(&ctx) {
            Ok((config, store)) => match boards::rename(config, store, &title).await {
                Ok(receipt) => {
                    emit(json, &receipt, &boards::render_text_create(&receipt));
                    0
                }
                Err(err) => report(json, &err),
            },
            Err(err) => report(json, &err),
        },
        Command::Boards {
            cmd: BoardsCmd::Publish { timeout_secs },
        } => match core_ctx(&ctx) {
            Ok((config, store)) => {
                match boards::publish(config, store, Duration::from_secs(timeout_secs)).await {
                    Ok(receipt) => {
                        emit(json, &receipt, &boards::render_text_select(&receipt));
                        0
                    }
                    Err(err) => report(json, &err),
                }
            }
            Err(err) => report(json, &err),
        },
        Command::Stacks {
            cmd: StacksCmd::Add { title, order },
        } => match core_ctx(&ctx) {
            Ok((config, store)) => match stacks::add(config, store, &title, order).await {
                Ok(receipt) => {
                    emit(json, &receipt, &stacks::render_text_add(&receipt));
                    0
                }
                Err(err) => report(json, &err),
            },
            Err(err) => report(json, &err),
        },
        Command::Tasks {
            cmd: TasksCmd::List { stack },
        } => match core_ctx(&ctx) {
            Ok((config, store)) => {
                let standalone = config.sync_backend() == SyncBackend::Standalone;
                match tasks::list(config, store, stack.as_deref()).await {
                    Ok(list) => {
                        emit(json, &list, &tasks::render_text_list(&list, standalone));
                        0
                    }
                    Err(err) => report(json, &err),
                }
            }
            Err(err) => report(json, &err),
        },
        Command::Tasks {
            cmd: TasksCmd::Add { title, stack },
        } => {
            run_mutation(json, &ctx, move |config, store| async move {
                tasks::add(config, store, &title, stack.as_deref()).await
            })
            .await
        }
        Command::Tasks {
            cmd: TasksCmd::Done { task, undo },
        } => {
            run_mutation(json, &ctx, move |config, store| async move {
                tasks::done(config, store, &task, undo).await
            })
            .await
        }
        Command::Tasks {
            cmd: TasksCmd::Move { task, stack, order },
        } => {
            run_mutation(json, &ctx, move |config, store| async move {
                tasks::move_(config, store, &task, stack.as_deref(), order).await
            })
            .await
        }
        Command::Tasks {
            cmd:
                TasksCmd::Edit {
                    task,
                    title,
                    description,
                    due,
                    archive,
                    unarchive,
                },
        } => {
            let changes = task_changes(title, description, due, archive, unarchive);
            run_mutation(json, &ctx, move |config, store| async move {
                tasks::edit(config, store, &task, changes).await
            })
            .await
        }
        Command::Tasks {
            cmd: TasksCmd::Delete { task },
        } => match core_ctx(&ctx) {
            Ok((config, store)) => match tasks::delete(config, store, &task).await {
                Ok(receipt) => {
                    emit(
                        json,
                        &receipt,
                        &format!("deleted \"{}\" ({})", receipt.title, receipt.id),
                    );
                    0
                }
                Err(err) => report(json, &err),
            },
            Err(err) => report(json, &err),
        },
        Command::Sync {
            no_wait,
            timeout_secs,
        } => match core_ctx(&ctx) {
            Ok((config, store)) => {
                match sync::run(config, store, no_wait, Duration::from_secs(timeout_secs)).await {
                    Ok(outcome) => {
                        emit(json, outcome.view(), &sync::render_text(&outcome));
                        match outcome {
                            sync::SyncOutcome::Succeeded(_) | sync::SyncOutcome::NoWait(_) => 0,
                            sync::SyncOutcome::Failed(kind, _) => {
                                report(json, &CommandError::SyncFailed(kind))
                            }
                            sync::SyncOutcome::TimedOut(_) => {
                                report(json, &CommandError::SyncTimedOut)
                            }
                        }
                    }
                    Err(err) => report(json, &err),
                }
            }
            Err(err) => report(json, &err),
        },
        Command::Daemon => unreachable!("main handles the daemon arm itself"),
    }
}

/// Maps the `tasks edit` flags onto the domain's sparse changeset:
/// `DueSpec::Clear` becomes `Some(None)` (the clear-vs-untouched
/// distinction), `--archive`/`--unarchive` are mutually exclusive (clap).
fn task_changes(
    title: Option<String>,
    description: Option<String>,
    due: Option<crate::cli::DueSpec>,
    archive: bool,
    unarchive: bool,
) -> taskboard_domain::TaskChanges {
    taskboard_domain::TaskChanges {
        title,
        description,
        duedate: due.map(|spec| match spec {
            crate::cli::DueSpec::Set(dt) => Some(dt),
            crate::cli::DueSpec::Clear => None,
        }),
        archived: if archive {
            Some(true)
        } else {
            unarchive.then_some(false)
        },
    }
}

/// The shared shape of the mutation verbs: boot the core, run the verb,
/// print the receipt (or the error).
async fn run_mutation<F, Fut>(json: bool, ctx: &CommandCtx, body: F) -> u8
where
    F: FnOnce(AppConfig, Arc<dyn CredentialStore>) -> Fut,
    Fut: std::future::Future<Output = Result<output::MutationReceipt, CommandError>>,
{
    match core_ctx(ctx) {
        Ok((config, store)) => match body(config, store).await {
            Ok(receipt) => {
                emit(json, &receipt, &tasks::render_text_receipt(&receipt));
                0
            }
            Err(err) => report(json, &err),
        },
        Err(err) => report(json, &err),
    }
}

/// Loads and validates the config and picks the production credential
/// store. Not `Result`-returning so callers can `report` and exit 1.
fn core_ctx(ctx: &CommandCtx) -> Result<(AppConfig, Arc<dyn CredentialStore>), CommandError> {
    let config = AppConfig::load(ctx.config_path.clone())?;
    let store = production_store(config.nextcloud.credential_store);
    Ok((config, store))
}

/// Prints a payload: JSON on stdout under `--json`, the human text
/// otherwise.
fn emit<T: serde::Serialize>(json: bool, payload: &T, text: &str) {
    if json {
        println!(
            "{}",
            serde_json::to_string(payload).expect("projection serialization is total")
        );
    } else {
        println!("{text}");
    }
}

/// Reports a failure: `{"error":{…}}` on stderr under `--json`, the
/// `Display` text otherwise. Always yields exit code 1. Also used by
/// `main`'s daemon arm (the standalone refusal).
///
/// # Panics
///
/// Never: the error envelope serialization is total.
pub fn report(json: bool, err: &CommandError) -> u8 {
    tracing::error!(kind = err.error_kind(), "command failed");
    if json {
        let envelope = output::ErrorEnvelope {
            error: output::ErrorBody {
                kind: err.error_kind().to_owned(),
                detail: err.error_detail(),
            },
        };
        eprintln!(
            "{}",
            serde_json::to_string(&envelope).expect("error envelope serialization is total")
        );
    } else {
        eprintln!("{err}");
    }
    1
}
