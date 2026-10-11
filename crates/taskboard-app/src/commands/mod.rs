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
pub mod config;
pub mod labels;
pub mod login;
pub mod logout;
pub mod output;
pub mod stacks;
pub mod status;
pub mod sync;
pub mod tasks;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::{StateCommand, SyncErrorKind};
use taskboard_sync_nextcloud::DeckError;

use crate::bootstrap::BootstrapError;
use crate::cli::{BoardsCmd, Command, LabelsCmd, StacksCmd, TasksCmd};
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

/// Where a dispatch writes its output. The binary wires stdout/stderr;
/// tests inject buffers and the future daemon-IPC frontend (WI-5) will
/// inject socket sinks. `json` rides per invocation (REPL: per line).
pub struct CommandIo<'a> {
    /// Payloads (or human text) — stdout in the binary.
    pub out: &'a mut dyn std::io::Write,
    /// Error reports — stderr in the binary.
    pub err: &'a mut dyn std::io::Write,
    /// `--json`: machine-readable payloads on `out`, error JSON on
    /// `err`.
    pub json: bool,
}

/// Everything a handler needs; built by `main` once per invocation.
pub struct CommandCtx<'a> {
    /// The resolved config path (by [`crate::config::config_path`]).
    pub config_path: Option<PathBuf>,
    /// Where payloads and reports are written.
    pub io: CommandIo<'a>,
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
    /// No label matches the selector.
    #[error("no such label")]
    UnknownLabel(String),
    /// The label selector matches several live labels; carries their
    /// titles.
    #[error("label selector matches several labels — pass one of {}", candidates.join(" | "))]
    AmbiguousLabel {
        /// The matched labels' titles.
        candidates: Vec<String>,
    },
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
    /// No live board exists (the single-board MVP's precondition failed
    /// in a CLI handler before any command reached the engine; the
    /// engine's own rejection keeps the same kind via `domain_error_kind`).
    #[error("no board")]
    NoBoard,
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
    /// A command needs a configured connector that this install lacks
    /// (`logout` with no configured server/username).
    #[error("no Nextcloud connection is configured")]
    NotConfigured,
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
            Self::UnknownLabel(_) => "unknown-label",
            Self::AmbiguousLabel { .. } => "ambiguous-label",
            Self::NoStacks => "no-stacks",
            Self::NoBoard => "no-board",
            Self::SyncFailed(_) => "sync-failed",
            Self::SyncTimedOut => "sync-timed-out",
            Self::ConfigWrite(_) => "config-write",
            Self::Prompt(_) => "prompt",
            Self::Deck(_) => "deck",
            Self::SyncDisabled => "sync-disabled",
            Self::LocalBoardExists => "local-board-exists",
            Self::NotConfigured => "not-configured",
        }
    }

    /// The optional structural detail (ids, nested kinds) of the
    /// `--json` error shape.
    #[must_use]
    pub fn error_detail(&self) -> Option<String> {
        match self {
            Self::UnknownBoard(id) => Some(id.to_string()),
            Self::BoardAlreadyBound { current } => Some(current.to_string()),
            Self::UnknownTask(sel) | Self::UnknownStack(sel) | Self::UnknownLabel(sel) => {
                Some(sel.clone())
            }
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
pub async fn dispatch(command: Command, mut ctx: CommandCtx<'_>) -> u8 {
    let json = ctx.io.json;
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
                        return report_to(&mut ctx.io, &CommandError::SyncDisabled);
                    }
                    let store = production_store(raw.nextcloud.credential_store);
                    let mut prompts = login::StdinPrompts;
                    match login::run(&path, raw, server, user, &mut prompts, &store).await {
                        Ok(receipt) => {
                            emit_to(&mut ctx.io, &receipt, &login::render_text(&receipt));
                            0
                        }
                        Err(err) => report_to(&mut ctx.io, &err),
                    }
                }
                // A not-yet-valid config must not block login (first-run
                // bootstrap command); undecodable ones still fail.
                Err(err) => report_to(&mut ctx.io, &CommandError::from(err)),
            }
        }
        Command::Logout => {
            let path = AppConfig::config_path(ctx.config_path.clone())
                .unwrap_or_else(|| PathBuf::from(crate::config::DEFAULT_CONFIG_PATH));
            match AppConfig::extract_raw(Some(&path)) {
                Ok(raw) => {
                    let store = production_store(raw.nextcloud.credential_store);
                    match logout::run(&path, &raw, store.as_ref()) {
                        Ok(receipt) => {
                            emit_to(&mut ctx.io, &receipt, &logout::render_text(&receipt));
                            0
                        }
                        Err(err) => report_to(&mut ctx.io, &err),
                    }
                }
                Err(err) => report_to(&mut ctx.io, &CommandError::from(err)),
            }
        }
        Command::Boards {
            cmd: BoardsCmd::List,
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => match boards::list(&config, &store).await {
                Ok(entries) => {
                    emit_to(&mut ctx.io, &entries, &boards::render_text_list(&entries));
                    0
                }
                Err(err) => report_to(&mut ctx.io, &err),
            },
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Boards {
            cmd:
                BoardsCmd::Select {
                    board,
                    timeout_secs,
                    bind_local_board,
                },
        } => match core_ctx(ctx.config_path.clone()) {
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
                        emit_to(&mut ctx.io, &receipt, &boards::render_text_select(&receipt));
                        0
                    }
                    Err(err) => report_to(&mut ctx.io, &err),
                }
            }
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Boards {
            cmd: BoardsCmd::Create { title, color },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => {
                match boards::create(config, store, &title, color.as_deref()).await {
                    Ok(receipt) => {
                        emit_to(&mut ctx.io, &receipt, &boards::render_text_create(&receipt));
                        0
                    }
                    Err(err) => report_to(&mut ctx.io, &err),
                }
            }
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Boards {
            cmd: BoardsCmd::Rename { title },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => match boards::rename(config, store, &title).await {
                Ok(receipt) => {
                    emit_to(&mut ctx.io, &receipt, &boards::render_text_rename(&receipt));
                    0
                }
                Err(err) => report_to(&mut ctx.io, &err),
            },
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Boards {
            cmd: BoardsCmd::Publish { timeout_secs },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => {
                match boards::publish(config, store, Duration::from_secs(timeout_secs)).await {
                    Ok(receipt) => {
                        emit_to(&mut ctx.io, &receipt, &boards::render_text_select(&receipt));
                        0
                    }
                    Err(err) => report_to(&mut ctx.io, &err),
                }
            }
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Stacks {
            cmd: StacksCmd::Add { title, order },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => match stacks::add(config, store, &title, order).await {
                Ok(receipt) => {
                    emit_to(&mut ctx.io, &receipt, &stacks::render_text_add(&receipt));
                    0
                }
                Err(err) => report_to(&mut ctx.io, &err),
            },
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Stacks {
            cmd: StacksCmd::Rename { stack, title },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => match stacks::rename(config, store, &stack, &title).await {
                Ok(receipt) => {
                    emit_to(&mut ctx.io, &receipt, &stacks::render_text_add(&receipt));
                    0
                }
                Err(err) => report_to(&mut ctx.io, &err),
            },
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Stacks {
            cmd: StacksCmd::Delete { stack },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => match stacks::delete(config, store, &stack).await {
                Ok(receipt) => {
                    emit_to(&mut ctx.io, &receipt, &stacks::render_text_delete(&receipt));
                    0
                }
                Err(err) => report_to(&mut ctx.io, &err),
            },
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Stacks {
            cmd: StacksCmd::Move { stack, order },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => match stacks::move_(config, store, &stack, order).await {
                Ok(receipt) => {
                    emit_to(&mut ctx.io, &receipt, &stacks::render_text_add(&receipt));
                    0
                }
                Err(err) => report_to(&mut ctx.io, &err),
            },
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Labels {
            cmd: LabelsCmd::List,
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => match labels::list(config, store).await {
                Ok(rows) => {
                    emit_to(&mut ctx.io, &rows, &labels::render_text_list(&rows));
                    0
                }
                Err(err) => report_to(&mut ctx.io, &err),
            },
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Labels {
            cmd: LabelsCmd::Add { title, color },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => {
                match labels::add(config, store, &title, color.as_deref()).await {
                    Ok(receipt) => {
                        emit_to(
                            &mut ctx.io,
                            &receipt,
                            &labels::render_text_receipt(&receipt),
                        );
                        0
                    }
                    Err(err) => report_to(&mut ctx.io, &err),
                }
            }
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Labels {
            cmd:
                LabelsCmd::Edit {
                    label,
                    title,
                    color,
                },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => match labels::edit(config, store, &label, title, color).await {
                Ok(receipt) => {
                    emit_to(
                        &mut ctx.io,
                        &receipt,
                        &labels::render_text_receipt(&receipt),
                    );
                    0
                }
                Err(err) => report_to(&mut ctx.io, &err),
            },
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Labels {
            cmd: LabelsCmd::Delete { label },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => match labels::delete(config, store, &label).await {
                Ok(receipt) => {
                    emit_to(
                        &mut ctx.io,
                        &receipt,
                        &labels::render_text_receipt(&receipt),
                    );
                    0
                }
                Err(err) => report_to(&mut ctx.io, &err),
            },
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Tasks {
            cmd: TasksCmd::List { stack },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => {
                let standalone = config.sync_backend() == SyncBackend::Standalone;
                match tasks::list(config, store, stack.as_deref()).await {
                    Ok(list) => {
                        emit_to(
                            &mut ctx.io,
                            &list,
                            &tasks::render_text_list(&list, standalone),
                        );
                        0
                    }
                    Err(err) => report_to(&mut ctx.io, &err),
                }
            }
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Tasks {
            cmd: TasksCmd::Add { title, stack },
        } => {
            run_mutation(
                &mut ctx.io,
                ctx.config_path.clone(),
                move |config, store| async move {
                    tasks::add(config, store, &title, stack.as_deref()).await
                },
            )
            .await
        }
        Command::Tasks {
            cmd: TasksCmd::Done { task, undo },
        } => {
            run_mutation(
                &mut ctx.io,
                ctx.config_path.clone(),
                move |config, store| async move { tasks::done(config, store, &task, undo).await },
            )
            .await
        }
        Command::Tasks {
            cmd: TasksCmd::Move { task, stack, order },
        } => {
            run_mutation(
                &mut ctx.io,
                ctx.config_path.clone(),
                move |config, store| async move {
                    tasks::move_(config, store, &task, stack.as_deref(), order).await
                },
            )
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
            run_mutation(&mut ctx.io, ctx.config_path.clone(), move |config, store| async move {
                tasks::edit(config, store, &task, changes).await
            })
            .await
        }
        Command::Tasks {
            cmd: TasksCmd::Delete { task },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => match tasks::delete(config, store, &task).await {
                Ok(receipt) => {
                    emit_to(
                        &mut ctx.io,
                        &receipt,
                        &format!("deleted \"{}\" ({})", receipt.title, receipt.id),
                    );
                    0
                }
                Err(err) => report_to(&mut ctx.io, &err),
            },
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Tasks {
            cmd: TasksCmd::Label { task, label },
        } => run_mutation(
            &mut ctx.io,
            ctx.config_path.clone(),
            move |config, store| async move { tasks::label(config, store, &task, &label).await },
        )
        .await,
        Command::Tasks {
            cmd: TasksCmd::Unlabel { task, label },
        } => run_mutation(
            &mut ctx.io,
            ctx.config_path.clone(),
            move |config, store| async move { tasks::unlabel(config, store, &task, &label).await },
        )
        .await,
        Command::Sync {
            no_wait,
            timeout_secs,
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => {
                match sync::run(config, store, no_wait, Duration::from_secs(timeout_secs)).await {
                    Ok(outcome) => {
                        emit_to(&mut ctx.io, outcome.view(), &sync::render_text(&outcome));
                        match outcome {
                            sync::SyncOutcome::Succeeded(_) | sync::SyncOutcome::NoWait(_) => 0,
                            sync::SyncOutcome::Failed(kind, _) => {
                                report_to(&mut ctx.io, &CommandError::SyncFailed(kind))
                            }
                            sync::SyncOutcome::TimedOut(_) => {
                                report_to(&mut ctx.io, &CommandError::SyncTimedOut)
                            }
                        }
                    }
                    Err(err) => report_to(&mut ctx.io, &err),
                }
            }
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Config { cmd } => config::handle(&cmd, &mut ctx),
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

/// The cross-line session state every dispatch needs (ADR 0011/0012):
/// the resolved config and the credential store it selected. A trait so
/// `dispatch_line` routes over *any* frontend's session — the REPL's
/// [`crate::repl::ReplSession`] (which adds its `json_default`) and the
/// daemon IPC server's [`BasicSession`] alike — without either knowing
/// about the other.
pub trait CoreSession {
    /// The resolved, validated config the session booted with.
    fn session_config(&self) -> &AppConfig;
    /// The credential store picked from the config at boot.
    fn session_store(&self) -> &Arc<dyn CredentialStore>;
}

/// The minimal concrete session for frontends holding exactly
/// `{config, store}` — the daemon IPC server's dispatcher.
pub struct BasicSession {
    /// The resolved, validated config the core booted with.
    pub config: AppConfig,
    /// The credential store picked from the config at boot.
    pub store: Arc<dyn CredentialStore>,
}

impl CoreSession for BasicSession {
    fn session_config(&self) -> &AppConfig {
        &self.config
    }
    fn session_store(&self) -> &Arc<dyn CredentialStore> {
        &self.store
    }
}

/// Routes one parsed line against the booted session core (ADR 0011).
/// The session loop has already intercepted `Daemon`/`Login`/`Logout`/
/// `Config`; this is the per-verb table over the shared `*_on` seam.
/// Never exits; the returned code is informational — a failed line never
/// aborts the session nor shapes its exit code.
///
/// Connector verbs run their prelude against the *session* config and
/// store (typed `sync-disabled` under `standalone`, fresh remote reads
/// for the binding ceremonies); everything else hits the session core
/// directly — no boot, no shutdown, no lock churn.
#[allow(clippy::too_many_lines)] // one arm per subcommand, kept flat for review
pub async fn dispatch_line(
    session: &impl CoreSession,
    app: &crate::App,
    command: Command,
    io: &mut CommandIo<'_>,
) -> u8 {
    tracing::info!(?command, json = io.json, "session line");
    match command {
        Command::Login { .. } | Command::Logout | Command::Config { .. } | Command::Daemon => {
            unreachable!(
                "the session loop intercepts daemon, login, logout, and config before routing"
            )
        }
        Command::Boards {
            cmd: BoardsCmd::List,
        } => match boards::list(session.session_config(), session.session_store()).await {
            Ok(entries) => {
                emit_to(io, &entries, &boards::render_text_list(&entries));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Boards {
            cmd:
                BoardsCmd::Select {
                    board,
                    timeout_secs,
                    bind_local_board,
                },
        } => match boards::select_prelude(session.session_config(), session.session_store(), board)
            .await
        {
            Ok(()) => match boards::select_on(
                app,
                board,
                Duration::from_secs(timeout_secs),
                bind_local_board,
            )
            .await
            {
                Ok(receipt) => {
                    emit_to(io, &receipt, &boards::render_text_select(&receipt));
                    0
                }
                Err(err) => report_to(io, &err),
            },
            Err(err) => report_to(io, &err),
        },
        Command::Boards {
            cmd: BoardsCmd::Create { title, color },
        } => {
            let color = boards::resolved_color(color.as_deref());
            let color_str = color.as_str().to_owned();
            match boards::create_on(app, title, color, color_str).await {
                Ok(receipt) => {
                    emit_to(io, &receipt, &boards::render_text_create(&receipt));
                    0
                }
                Err(err) => report_to(io, &err),
            }
        }
        Command::Boards {
            cmd: BoardsCmd::Rename { title },
        } => match boards::rename_on(app, title).await {
            Ok(receipt) => {
                emit_to(io, &receipt, &boards::render_text_rename(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Boards {
            cmd: BoardsCmd::Publish { timeout_secs },
        } => match boards::publish_prelude(session.session_config(), session.session_store()) {
            Ok(client) => {
                match boards::publish_on(app, &client, Duration::from_secs(timeout_secs)).await {
                    Ok(receipt) => {
                        emit_to(io, &receipt, &boards::render_text_select(&receipt));
                        0
                    }
                    Err(err) => report_to(io, &err),
                }
            }
            Err(err) => report_to(io, &err),
        },
        Command::Stacks {
            cmd: StacksCmd::Add { title, order },
        } => match stacks::add_on(app, title, order).await {
            Ok(receipt) => {
                emit_to(io, &receipt, &stacks::render_text_add(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Stacks {
            cmd: StacksCmd::Rename { stack, title },
        } => match stacks::rename_on(app, &stack, title).await {
            Ok(receipt) => {
                emit_to(io, &receipt, &stacks::render_text_add(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Stacks {
            cmd: StacksCmd::Delete { stack },
        } => match stacks::delete_on(app, &stack).await {
            Ok(receipt) => {
                emit_to(io, &receipt, &stacks::render_text_delete(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Stacks {
            cmd: StacksCmd::Move { stack, order },
        } => match stacks::move_on(app, &stack, order).await {
            Ok(receipt) => {
                emit_to(io, &receipt, &stacks::render_text_add(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Labels {
            cmd: LabelsCmd::List,
        } => match labels::list_on(app) {
            Ok(rows) => {
                emit_to(io, &rows, &labels::render_text_list(&rows));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Labels {
            cmd: LabelsCmd::Add { title, color },
        } => {
            // The clap value parser normalizes to lowercase; the
            // session arm normalizes like the one-shot wrapper does.
            let color = color.map_or_else(
                || labels::DEFAULT_LABEL_COLOR.to_owned(),
                |c| c.to_lowercase(),
            );
            match labels::add_on(app, title, color).await {
                Ok(receipt) => {
                    emit_to(io, &receipt, &labels::render_text_receipt(&receipt));
                    0
                }
                Err(err) => report_to(io, &err),
            }
        }
        Command::Labels {
            cmd:
                LabelsCmd::Edit {
                    label,
                    title,
                    color,
                },
        } => match labels::edit_on(app, &label, title, color).await {
            Ok(receipt) => {
                emit_to(io, &receipt, &labels::render_text_receipt(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Labels {
            cmd: LabelsCmd::Delete { label },
        } => match labels::delete_on(app, &label).await {
            Ok(receipt) => {
                emit_to(io, &receipt, &labels::render_text_receipt(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Tasks {
            cmd: TasksCmd::List { stack },
        } => {
            let standalone = session.session_config().sync_backend() == SyncBackend::Standalone;
            match tasks::list_on(app, stack.as_deref()) {
                Ok(list) => {
                    emit_to(io, &list, &tasks::render_text_list(&list, standalone));
                    0
                }
                Err(err) => report_to(io, &err),
            }
        }
        Command::Tasks {
            cmd: TasksCmd::Add { title, stack },
        } => match tasks::add_on(app, title, stack.as_deref()).await {
            Ok(receipt) => {
                emit_to(io, &receipt, &tasks::render_text_receipt(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Tasks {
            cmd: TasksCmd::Done { task, undo },
        } => match tasks::done_on(app, &task, undo).await {
            Ok(receipt) => {
                emit_to(io, &receipt, &tasks::render_text_receipt(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Tasks {
            cmd: TasksCmd::Move { task, stack, order },
        } => match tasks::move_on(app, &task, stack.as_deref(), order).await {
            Ok(receipt) => {
                emit_to(io, &receipt, &tasks::render_text_receipt(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
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
            match tasks::edit_on(app, &task, changes).await {
                Ok(receipt) => {
                    emit_to(io, &receipt, &tasks::render_text_receipt(&receipt));
                    0
                }
                Err(err) => report_to(io, &err),
            }
        }
        Command::Tasks {
            cmd: TasksCmd::Delete { task },
        } => match tasks::delete_on(app, &task).await {
            Ok(receipt) => {
                emit_to(
                    io,
                    &receipt,
                    &format!("deleted \"{}\" ({})", receipt.title, receipt.id),
                );
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Tasks {
            cmd: TasksCmd::Label { task, label },
        } => match tasks::label_on(app, &task, &label, |t, l| StateCommand::AssignLabel {
            task: t,
            label: l,
        })
        .await
        {
            Ok(receipt) => {
                emit_to(io, &receipt, &tasks::render_text_receipt(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Tasks {
            cmd: TasksCmd::Unlabel { task, label },
        } => match tasks::label_on(app, &task, &label, |t, l| StateCommand::UnassignLabel {
            task: t,
            label: l,
        })
        .await
        {
            Ok(receipt) => {
                emit_to(io, &receipt, &tasks::render_text_receipt(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Command::Sync {
            no_wait,
            timeout_secs,
        } => match crate::commands::require_connector(session.session_config()) {
            Ok(()) => match sync::sync_on(app, no_wait, Duration::from_secs(timeout_secs)).await {
                Ok(outcome) => {
                    emit_to(io, outcome.view(), &sync::render_text(&outcome));
                    match outcome {
                        sync::SyncOutcome::Succeeded(_) | sync::SyncOutcome::NoWait(_) => 0,
                        sync::SyncOutcome::Failed(kind, _) => {
                            report_to(io, &CommandError::SyncFailed(kind))
                        }
                        sync::SyncOutcome::TimedOut(_) => {
                            report_to(io, &CommandError::SyncTimedOut)
                        }
                    }
                }
                Err(err) => report_to(io, &err),
            },
            Err(err) => report_to(io, &err),
        },
    }
}

/// The shared shape of the mutation verbs: boot the core, run the verb,
/// print the receipt (or the error).
async fn run_mutation<F, Fut>(io: &mut CommandIo<'_>, config_path: Option<PathBuf>, body: F) -> u8
where
    F: FnOnce(AppConfig, Arc<dyn CredentialStore>) -> Fut,
    Fut: std::future::Future<Output = Result<output::MutationReceipt, CommandError>>,
{
    match core_ctx(config_path) {
        Ok((config, store)) => match body(config, store).await {
            Ok(receipt) => {
                emit_to(io, &receipt, &tasks::render_text_receipt(&receipt));
                0
            }
            Err(err) => report_to(io, &err),
        },
        Err(err) => report_to(io, &err),
    }
}

/// Loads and validates the config and picks the production credential
/// store. Not `Result`-returning so callers can `report` and exit 1.
fn core_ctx(
    config_path: Option<PathBuf>,
) -> Result<(AppConfig, Arc<dyn CredentialStore>), CommandError> {
    let config = AppConfig::load(config_path)?;
    let store = production_store(config.nextcloud.credential_store);
    Ok((config, store))
}

/// Prints a payload to the injected writers: JSON on `out` under
/// `--json`, the human text otherwise. Write failures are ignored: a
/// closed sink (a dead pty, a socket peer) must not panic a dispatch —
/// the process exit code, not the write, carries the outcome.
pub(crate) fn emit_to<T: serde::Serialize>(io: &mut CommandIo<'_>, payload: &T, text: &str) {
    if io.json {
        let line = serde_json::to_string(payload).expect("projection serialization is total");
        let _ = writeln!(io.out, "{line}");
    } else {
        let _ = writeln!(io.out, "{text}");
    }
}

/// Reports a failure to the injected writers: `{"error":{…}}` on `err`
/// under `--json`, the `Display` text otherwise. Always yields exit
/// code 1. Write failures are ignored (see [`emit_to`]).
///
/// # Panics
///
/// Never: the error envelope serialization is total.
#[must_use]
pub fn report_to(io: &mut CommandIo<'_>, err: &CommandError) -> u8 {
    tracing::error!(kind = err.error_kind(), "command failed");
    if io.json {
        let envelope = output::ErrorEnvelope {
            error: output::ErrorBody {
                kind: err.error_kind().to_owned(),
                detail: err.error_detail(),
            },
        };
        let line = serde_json::to_string(&envelope).expect("error envelope serialization is total");
        let _ = writeln!(io.err, "{line}");
    } else {
        let _ = writeln!(io.err, "{err}");
    }
    1
}

/// Reports a failure: `{"error":{…}}` on stderr under `--json`, the
/// `Display` text otherwise. Always yields exit code 1. Also used by
/// `main`'s daemon arm (the standalone refusal) — the stdout/stderr
/// convenience over [`report_to`].
///
/// # Panics
///
/// Never: the error envelope serialization is total.
#[must_use]
pub fn report(json: bool, err: &CommandError) -> u8 {
    let mut out = std::io::stdout();
    let mut errw = std::io::stderr();
    let mut io = CommandIo {
        out: &mut out,
        err: &mut errw,
        json,
    };
    report_to(&mut io, err)
}
