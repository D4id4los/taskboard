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
    /// Payloads (or human text) — stdout in the binary. `Send` so a
    /// dispatch can run inside a spawned task (the IPC dispatcher).
    pub out: &'a mut (dyn std::io::Write + Send),
    /// Error reports — stderr in the binary.
    pub err: &'a mut (dyn std::io::Write + Send),
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
    /// No daemon serves this database (`daemon stop` without a daemon).
    #[error("no daemon is serving this database")]
    NoDaemon,
    /// The IPC transport failed mid-request (connection death, frame
    /// garbage) — the daemon holds the instance lock, so in-process
    /// fallback would only report `instance-locked`; the typed failure
    /// is the honest signal (ADR 0012).
    #[error("daemon ipc transport failed")]
    IpcTransport,
    /// The daemon speaks a different wire-protocol version.
    #[error("daemon ipc protocol version mismatch")]
    IpcVersion {
        /// The server's protocol version.
        server_v: u32,
    },
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
            Self::NoDaemon => "no-daemon",
            Self::IpcTransport => "ipc-transport",
            Self::IpcVersion { .. } => "ipc-version",
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
            Self::IpcVersion { server_v } => Some(server_v.to_string()),
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
            login_command(&mut ctx.io, ctx.config_path, server, user).await
        }
        Command::Logout => logout_command(&mut ctx.io, ctx.config_path.clone()),
        Command::Boards {
            cmd: BoardsCmd::List,
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                |entries: &Vec<_>| boards::render_text_list(entries),
                |config, store| async move { boards::list(&config, &store).await },
            )
            .await
        }
        Command::Boards {
            cmd:
                BoardsCmd::Select {
                    board,
                    timeout_secs,
                    bind_local_board,
                },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                boards::render_text_select,
                move |config, store| async move {
                    boards::select(
                        config,
                        store,
                        board,
                        Duration::from_secs(timeout_secs),
                        bind_local_board,
                    )
                    .await
                },
            )
            .await
        }
        Command::Boards {
            cmd: BoardsCmd::Create { title, color },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                boards::render_text_create,
                move |config, store| async move {
                    boards::create(config, store, &title, color.as_deref()).await
                },
            )
            .await
        }
        Command::Boards {
            cmd: BoardsCmd::Rename { title },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                boards::render_text_rename,
                move |config, store| async move { boards::rename(config, store, &title).await },
            )
            .await
        }
        Command::Boards {
            cmd: BoardsCmd::Publish { timeout_secs },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                boards::render_text_select,
                move |config, store| async move {
                    boards::publish(config, store, Duration::from_secs(timeout_secs)).await
                },
            )
            .await
        }
        Command::Stacks {
            cmd: StacksCmd::Add { title, order },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                stacks::render_text_add,
                move |config, store| async move { stacks::add(config, store, &title, order).await },
            )
            .await
        }
        Command::Stacks {
            cmd: StacksCmd::Rename { stack, title },
        } => run_core(
            &mut ctx.io,
            ctx.config_path.clone(),
            stacks::render_text_add,
            move |config, store| async move { stacks::rename(config, store, &stack, &title).await },
        )
        .await,
        Command::Stacks {
            cmd: StacksCmd::Delete { stack },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                stacks::render_text_delete,
                move |config, store| async move { stacks::delete(config, store, &stack).await },
            )
            .await
        }
        Command::Stacks {
            cmd: StacksCmd::Move { stack, order },
        } => run_core(
            &mut ctx.io,
            ctx.config_path.clone(),
            stacks::render_text_add,
            move |config, store| async move { stacks::move_(config, store, &stack, order).await },
        )
        .await,
        Command::Labels {
            cmd: LabelsCmd::List,
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                |rows: &Vec<_>| labels::render_text_list(rows),
                |config, store| async move { labels::list(config, store).await },
            )
            .await
        }
        Command::Labels {
            cmd: LabelsCmd::Add { title, color },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                labels::render_text_receipt,
                move |config, store| async move {
                    labels::add(config, store, &title, color.as_deref()).await
                },
            )
            .await
        }
        Command::Labels {
            cmd:
                LabelsCmd::Edit {
                    label,
                    title,
                    color,
                },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                labels::render_text_receipt,
                move |config, store| async move {
                    labels::edit(config, store, &label, title, color).await
                },
            )
            .await
        }
        Command::Labels {
            cmd: LabelsCmd::Delete { label },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                labels::render_text_receipt,
                move |config, store| async move { labels::delete(config, store, &label).await },
            )
            .await
        }
        Command::Tasks {
            cmd: TasksCmd::List { stack },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => {
                let standalone = config.sync_backend() == SyncBackend::Standalone;
                emit_result(
                    &mut ctx.io,
                    tasks::list(config, store, stack.as_deref()).await,
                    |list| tasks::render_text_list(list, standalone),
                )
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
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                |receipt: &output::TaskDeleteReceipt| {
                    format!("deleted \"{}\" ({})", receipt.title, receipt.id)
                },
                move |config, store| async move { tasks::delete(config, store, &task).await },
            )
            .await
        }
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
        } => sync_command(&mut ctx.io, ctx.config_path.clone(), no_wait, timeout_secs).await,
        Command::Config { cmd } => config::handle(&cmd, &mut ctx),
        Command::Status => {
            // No core boot: the config resolves for the db path only; a
            // rejected config is the one typed failure status reports.
            let db_path = match AppConfig::load(ctx.config_path.clone()) {
                Ok(config) => config.db_path().to_path_buf(),
                Err(err) => return report_to(&mut ctx.io, &CommandError::from(err)),
            };
            let view = status::StatusView::no_daemon(&db_path);
            status::emit(&mut ctx.io, &view);
            0
        }
        Command::Daemon { .. } => unreachable!("main handles the daemon arms itself"),
    }
}

/// The shared shape of a completed verb: print the payload (JSON under
/// `--json`, the rendered text otherwise) and yield 0, or report the
/// typed failure and yield 1. Every `match result { Ok => emit,
/// Err => report }` in the dispatchers routes through here.
fn emit_result<T: serde::Serialize>(
    io: &mut CommandIo<'_>,
    result: Result<T, CommandError>,
    render: impl FnOnce(&T) -> String,
) -> u8 {
    match result {
        Ok(payload) => {
            emit_to(io, &payload, &render(&payload));
            0
        }
        Err(err) => report_to(io, &err),
    }
}

/// The shared shape of the config-backed verbs: resolve `{config,
/// store}`, run the verb, print the payload (or the error). `render`
/// maps the payload to its human text; `body` owns the resolved pair
/// for the duration of its future.
async fn run_core<T, R, F, Fut>(
    io: &mut CommandIo<'_>,
    config_path: Option<PathBuf>,
    render: R,
    body: F,
) -> u8
where
    T: serde::Serialize,
    R: FnOnce(&T) -> String,
    F: FnOnce(AppConfig, Arc<dyn CredentialStore>) -> Fut,
    Fut: std::future::Future<Output = Result<T, CommandError>>,
{
    match core_ctx(config_path) {
        Ok((config, store)) => emit_result(io, body(config, store).await, render),
        Err(err) => report_to(io, &err),
    }
}

/// One-shot `login`: resolves the config path (writing one if this is
/// the first run), refuses an explicit `standalone` before any
/// prompting, and persists the verified connection.
async fn login_command(
    io: &mut CommandIo<'_>,
    config_path: Option<PathBuf>,
    server: Option<String>,
    user: Option<String>,
) -> u8 {
    let path = AppConfig::config_path(config_path)
        .unwrap_or_else(|| PathBuf::from(crate::config::DEFAULT_CONFIG_PATH));
    match AppConfig::extract_raw(Some(&path)) {
        Ok(raw) => {
            // Explicit `standalone` refuses before any prompting
            // or store touch; auto behaves as today (and after
            // login writes `server_url`, auto resolves
            // `nextcloud`).
            if raw.sync.backend == Some(SyncBackend::Standalone) {
                return report_to(io, &CommandError::SyncDisabled);
            }
            let store = production_store(raw.nextcloud.credential_store);
            let mut prompts = login::StdinPrompts;
            emit_result(
                io,
                login::run(&path, raw, server, user, &mut prompts, &store).await,
                login::render_text,
            )
        }
        // A not-yet-valid config must not block login (first-run
        // bootstrap command); undecodable ones still fail.
        Err(err) => report_to(io, &CommandError::from(err)),
    }
}

/// One-shot `logout`: reads the config without validation (the
/// teardown command must survive a not-yet-valid file), deletes the
/// stored secret, and clears the auto keys.
fn logout_command(io: &mut CommandIo<'_>, config_path: Option<PathBuf>) -> u8 {
    let path = AppConfig::config_path(config_path)
        .unwrap_or_else(|| PathBuf::from(crate::config::DEFAULT_CONFIG_PATH));
    match AppConfig::extract_raw(Some(&path)) {
        Ok(raw) => {
            let store = production_store(raw.nextcloud.credential_store);
            emit_result(
                io,
                logout::run(&path, &raw, store.as_ref()),
                logout::render_text,
            )
        }
        Err(err) => report_to(io, &CommandError::from(err)),
    }
}

/// One-shot `sync`: one cycle against the resolved config, with the
/// outcome-specific exit codes (0 succeeded/no-wait, 1 failed/timed
/// out, envelope on `err`).
async fn sync_command(
    io: &mut CommandIo<'_>,
    config_path: Option<PathBuf>,
    no_wait: bool,
    timeout_secs: u64,
) -> u8 {
    match core_ctx(config_path) {
        Ok((config, store)) => {
            match sync::run(config, store, no_wait, Duration::from_secs(timeout_secs)).await {
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
            }
        }
        Err(err) => report_to(io, &err),
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
pub trait CoreSession: Send + Sync {
    /// The resolved, validated config the session booted with.
    fn session_config(&self) -> &AppConfig;
    /// The credential store picked from the config at boot.
    fn session_store(&self) -> &Arc<dyn CredentialStore>;
}

/// The minimal concrete session for frontends holding exactly
/// `{config, store}` — the daemon IPC server's dispatcher. The resolved
/// config path rides along so the server can honor an identical
/// `--config` (the socket *is* the config binding; a *different* file
/// over the same socket is the degenerate case §2.10 refuses).
pub struct BasicSession {
    /// The resolved, validated config the core booted with.
    pub config: AppConfig,
    /// The credential store picked from the config at boot.
    pub store: Arc<dyn CredentialStore>,
    /// The config file the core's config was resolved from (the
    /// daemon's `--config` answer), if one was named.
    pub config_path: Option<PathBuf>,
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
    session: &(impl CoreSession + ?Sized),
    app: &crate::App,
    command: Command,
    io: &mut CommandIo<'_>,
) -> u8 {
    tracing::info!(?command, json = io.json, "session line");
    match command {
        Command::Login { .. }
        | Command::Logout
        | Command::Config { .. }
        | Command::Daemon { .. } => {
            unreachable!(
                "the session loop intercepts daemon, login, logout, and config before routing"
            )
        }
        Command::Status => {
            // The session *is* the core holder; the honest client-side
            // answer names it. Never boots, never syncs.
            let view = status::StatusView::in_session();
            status::emit(io, &view);
            0
        }
        Command::Boards { cmd } => boards_line(session, app, cmd, io).await,
        Command::Stacks { cmd } => stacks_line(app, cmd, io).await,
        Command::Labels { cmd } => labels_line(app, cmd, io).await,
        Command::Tasks { cmd } => tasks_line(session, app, cmd, io).await,
        Command::Sync {
            no_wait,
            timeout_secs,
        } => sync_line(session, app, io, no_wait, timeout_secs).await,
    }
}

/// The boards family over a booted session (ADR 0011): `list` reads the
/// session config/store; `select`/`publish` run their connector
/// preludes against the session before touching the core; `create`/
/// `rename` hit the core directly.
async fn boards_line(
    session: &(impl CoreSession + ?Sized),
    app: &crate::App,
    cmd: BoardsCmd,
    io: &mut CommandIo<'_>,
) -> u8 {
    match cmd {
        BoardsCmd::List => emit_result(
            io,
            boards::list(session.session_config(), session.session_store()).await,
            |entries: &Vec<_>| boards::render_text_list(entries),
        ),
        BoardsCmd::Select {
            board,
            timeout_secs,
            bind_local_board,
        } => {
            match boards::select_prelude(session.session_config(), session.session_store(), board)
                .await
            {
                Ok(()) => emit_result(
                    io,
                    boards::select_on(
                        app,
                        board,
                        Duration::from_secs(timeout_secs),
                        bind_local_board,
                    )
                    .await,
                    boards::render_text_select,
                ),
                Err(err) => report_to(io, &err),
            }
        }
        BoardsCmd::Create { title, color } => {
            let color = boards::resolved_color(color.as_deref());
            let color_str = color.as_str().to_owned();
            emit_result(
                io,
                boards::create_on(app, title, color, color_str).await,
                boards::render_text_create,
            )
        }
        BoardsCmd::Rename { title } => emit_result(
            io,
            boards::rename_on(app, title).await,
            boards::render_text_rename,
        ),
        BoardsCmd::Publish { timeout_secs } => {
            match boards::publish_prelude(session.session_config(), session.session_store()) {
                Ok(client) => emit_result(
                    io,
                    boards::publish_on(app, &client, Duration::from_secs(timeout_secs)).await,
                    boards::render_text_select,
                ),
                Err(err) => report_to(io, &err),
            }
        }
    }
}

/// The stacks family over a booted session: pure core mutations, no
/// prelude.
async fn stacks_line(app: &crate::App, cmd: StacksCmd, io: &mut CommandIo<'_>) -> u8 {
    match cmd {
        StacksCmd::Add { title, order } => emit_result(
            io,
            stacks::add_on(app, title, order).await,
            stacks::render_text_add,
        ),
        StacksCmd::Rename { stack, title } => emit_result(
            io,
            stacks::rename_on(app, &stack, title).await,
            stacks::render_text_add,
        ),
        StacksCmd::Delete { stack } => emit_result(
            io,
            stacks::delete_on(app, &stack).await,
            stacks::render_text_delete,
        ),
        StacksCmd::Move { stack, order } => emit_result(
            io,
            stacks::move_on(app, &stack, order).await,
            stacks::render_text_add,
        ),
    }
}

/// The labels family over a booted session: `list` reads the core's
/// published state; the rest are core mutations.
async fn labels_line(app: &crate::App, cmd: LabelsCmd, io: &mut CommandIo<'_>) -> u8 {
    match cmd {
        LabelsCmd::List => emit_result(io, labels::list_on(app), |rows: &Vec<_>| {
            labels::render_text_list(rows)
        }),
        LabelsCmd::Add { title, color } => {
            // The clap value parser normalizes to lowercase; the
            // session arm normalizes like the one-shot wrapper does.
            let color = color.map_or_else(
                || labels::DEFAULT_LABEL_COLOR.to_owned(),
                |c| c.to_lowercase(),
            );
            emit_result(
                io,
                labels::add_on(app, title, color).await,
                labels::render_text_receipt,
            )
        }
        LabelsCmd::Edit {
            label,
            title,
            color,
        } => emit_result(
            io,
            labels::edit_on(app, &label, title, color).await,
            labels::render_text_receipt,
        ),
        LabelsCmd::Delete { label } => emit_result(
            io,
            labels::delete_on(app, &label).await,
            labels::render_text_receipt,
        ),
    }
}

/// The tasks family over a booted session: `list` renders against the
/// session backend; the rest are core mutations (`label`/`unlabel`
/// share one `*_on` body over the two state commands).
async fn tasks_line(
    session: &(impl CoreSession + ?Sized),
    app: &crate::App,
    cmd: TasksCmd,
    io: &mut CommandIo<'_>,
) -> u8 {
    match cmd {
        TasksCmd::List { stack } => {
            let standalone = session.session_config().sync_backend() == SyncBackend::Standalone;
            emit_result(io, tasks::list_on(app, stack.as_deref()), |list| {
                tasks::render_text_list(list, standalone)
            })
        }
        TasksCmd::Add { title, stack } => emit_result(
            io,
            tasks::add_on(app, title, stack.as_deref()).await,
            tasks::render_text_receipt,
        ),
        TasksCmd::Done { task, undo } => emit_result(
            io,
            tasks::done_on(app, &task, undo).await,
            tasks::render_text_receipt,
        ),
        TasksCmd::Move { task, stack, order } => emit_result(
            io,
            tasks::move_on(app, &task, stack.as_deref(), order).await,
            tasks::render_text_receipt,
        ),
        TasksCmd::Edit {
            task,
            title,
            description,
            due,
            archive,
            unarchive,
        } => {
            let changes = task_changes(title, description, due, archive, unarchive);
            emit_result(
                io,
                tasks::edit_on(app, &task, changes).await,
                tasks::render_text_receipt,
            )
        }
        TasksCmd::Delete { task } => emit_result(
            io,
            tasks::delete_on(app, &task).await,
            |receipt: &output::TaskDeleteReceipt| {
                format!("deleted \"{}\" ({})", receipt.title, receipt.id)
            },
        ),
        TasksCmd::Label { task, label } => emit_result(
            io,
            tasks::label_on(app, &task, &label, |t, l| StateCommand::AssignLabel {
                task: t,
                label: l,
            })
            .await,
            tasks::render_text_receipt,
        ),
        TasksCmd::Unlabel { task, label } => emit_result(
            io,
            tasks::label_on(app, &task, &label, |t, l| StateCommand::UnassignLabel {
                task: t,
                label: l,
            })
            .await,
            tasks::render_text_receipt,
        ),
    }
}

/// The session `sync`: connector-only (`sync-disabled` under
/// `standalone`), one cycle against the booted core, outcome-specific
/// exit codes.
async fn sync_line(
    session: &(impl CoreSession + ?Sized),
    app: &crate::App,
    io: &mut CommandIo<'_>,
    no_wait: bool,
    timeout_secs: u64,
) -> u8 {
    match crate::commands::require_connector(session.session_config()) {
        Ok(()) => match sync::sync_on(app, no_wait, Duration::from_secs(timeout_secs)).await {
            Ok(outcome) => {
                emit_to(io, outcome.view(), &sync::render_text(&outcome));
                match outcome {
                    sync::SyncOutcome::Succeeded(_) | sync::SyncOutcome::NoWait(_) => 0,
                    sync::SyncOutcome::Failed(kind, _) => {
                        report_to(io, &CommandError::SyncFailed(kind))
                    }
                    sync::SyncOutcome::TimedOut(_) => report_to(io, &CommandError::SyncTimedOut),
                }
            }
            Err(err) => report_to(io, &err),
        },
        Err(err) => report_to(io, &err),
    }
}

/// Which transport lane a `Command` takes (ADR 0012): `Local` stays
/// client-side always (pre-core verbs — no lock, no core, unchanged);
/// `IpcOrLocal` is transparently served by a running daemon when one
/// answers, else boots in-process as today; `IpcOnly` exists to talk to
/// a daemon (`status`, `daemon stop`) and reports typed `no-daemon`
/// otherwise. Exhaustive: the compiler forces every future verb to pick
/// a lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcRoute {
    /// Always in-process (pre-core verbs).
    Local,
    /// Daemon when one serves the db, else in-process boot.
    IpcOrLocal,
    /// Only meaningful against a running daemon.
    IpcOnly,
}

/// The per-`Command` routing table (pure; unit-pinned).
#[must_use]
pub fn route_for_ipc(command: &Command) -> IpcRoute {
    match command {
        Command::Login { .. }
        | Command::Logout
        | Command::Config { .. }
        | Command::Boards {
            cmd: BoardsCmd::List,
        } => IpcRoute::Local,
        Command::Status | Command::Daemon { .. } => IpcRoute::IpcOnly,
        Command::Boards { .. }
        | Command::Stacks { .. }
        | Command::Labels { .. }
        | Command::Tasks { .. }
        | Command::Sync { .. } => IpcRoute::IpcOrLocal,
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

#[cfg(test)]
mod ipc_routing_tests {
    use crate::cli::{BoardsCmd, DaemonCmd, TasksCmd};
    use crate::commands::{Command, IpcRoute, route_for_ipc};

    /// The transport lane table (ADR 0012 §2.1): pre-core verbs stay
    /// client-side; core verbs prefer a daemon; the operator verbs only
    /// talk to one. Exhaustive — the compiler forces future verbs here.
    #[test]
    fn the_routing_table_picks_lanes_per_command() {
        let local = [
            Command::Login {
                server: None,
                user: None,
            },
            Command::Logout,
            Command::Config {
                cmd: crate::cli::ConfigCmd::Print,
            },
            Command::Boards {
                cmd: BoardsCmd::List,
            },
        ];
        for command in &local {
            assert_eq!(route_for_ipc(command), IpcRoute::Local, "{command:?}");
        }

        let ipc_only = [
            Command::Status,
            Command::Daemon {
                cmd: Some(DaemonCmd::Stop),
            },
        ];
        for command in &ipc_only {
            assert_eq!(route_for_ipc(command), IpcRoute::IpcOnly, "{command:?}");
        }

        let ipc_or_local = [
            Command::Boards {
                cmd: BoardsCmd::Create {
                    title: "t".to_owned(),
                    color: None,
                },
            },
            Command::Boards {
                cmd: BoardsCmd::Select {
                    board: 3,
                    timeout_secs: 1,
                    bind_local_board: false,
                },
            },
            Command::Stacks {
                cmd: crate::cli::StacksCmd::Add {
                    title: "t".to_owned(),
                    order: None,
                },
            },
            Command::Labels {
                cmd: crate::cli::LabelsCmd::List,
            },
            Command::Tasks {
                cmd: TasksCmd::Add {
                    title: "t".to_owned(),
                    stack: None,
                },
            },
            Command::Sync {
                no_wait: false,
                timeout_secs: 1,
            },
        ];
        for command in &ipc_or_local {
            assert_eq!(route_for_ipc(command), IpcRoute::IpcOrLocal, "{command:?}");
        }
    }
}
