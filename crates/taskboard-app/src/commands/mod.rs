// SPDX-License-Identifier: AGPL-3.0-only
//! The operator-facing command handlers (ADR 0009): one module per
//! subcommand group plus [`output`] — typed handlers returning typed
//! outcomes, all lib code so the L/V/K/O-series test suites drive them
//! hermetically. `main` stays a thin dispatch + exit-code shell.
//!
//! Two command classes:
//!
//! - **Pre-core** (`connect`/`login`, `boards list`): config + a [`DeckClient`]
//!   only — no actor graph, no database, no instance lock.
//! - **Core-booting** (`boards select`, `tasks *`, `sync`): one full
//!   [`bootstrap`](crate::bootstrap) per invocation — boot, work,
//!   `shutdown()`, exit (roadmap §2.4: one bootstrap, N frontends).

pub mod boards;
pub mod breaks;
pub mod capture;
pub mod config;
pub mod connect;
pub mod find;
pub mod habits;
pub mod labels;
pub mod logout;
pub mod output;
pub mod reflection;
pub mod reminders;
pub mod repeat;
pub mod stacks;
pub mod status;
pub mod sync;
pub mod tasks;
pub mod timers;
pub mod track;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use taskboard_domain::{StateCommand, SyncErrorKind};
use taskboard_sync_nextcloud::DeckError;

use crate::bootstrap::BootstrapError;
use crate::cli::{BoardsCmd, Command, DaemonCmd, LabelEditFlags, LabelsCmd, StacksCmd, TasksCmd};
use crate::config::{AppConfig, CredentialStoreKind};
use crate::secrets::{CredentialError, CredentialStore};

/// The connector-only gate (ADR 0010 decision 9): runs at handler entry
/// — after config load, before any core boot or client construction —
/// so connector verbs fail loudly when the core has no connector.
/// # Errors
///
/// [`CommandError::SyncDisabled`] when no connector is configured — a
/// standalone resolution or an empty `[[sync.connectors]]` array.
pub fn require_connector(config: &AppConfig) -> Result<(), CommandError> {
    // Pluralised (D5-8): "at least one connector configured" — the
    // array world counts entries (an empty array is a valid
    // zero-connector world), the legacy scalar world resolves through
    // the same set.
    if crate::connectors::configured_connectors(config)
        .map_err(CommandError::from)?
        .is_empty()
    {
        return Err(CommandError::SyncDisabled);
    }
    Ok(())
}

/// Resolves the connector a targeting verb addresses (D6-5): the named
/// entry, else the default (first). Call after [`require_connector`].
///
/// # Errors
///
/// [`CommandError::UnknownConnector`] when the flag names no configured
/// connector. No clap-level value parser: the configured set is config
/// state, so the guard lives here where config lives.
pub(crate) fn resolve_connector(
    config: &AppConfig,
    flag: Option<&str>,
) -> Result<crate::connectors::ConnectorSpec, CommandError> {
    let configured =
        crate::connectors::configured_connectors(config).map_err(CommandError::from)?;
    match flag {
        None => configured
            .into_iter()
            .next()
            .ok_or(CommandError::SyncDisabled),
        Some(name) => configured
            .into_iter()
            .find(|spec| spec.name.as_str() == name)
            .ok_or_else(|| CommandError::UnknownConnector {
                name: name.to_owned(),
            }),
    }
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
    /// A `config set`/`unset` invocation was refused (T-18, #801):
    /// env-locked key, unknown key, malformed value, or a write
    /// failure — matches!-able at the arm.
    #[error("config set refused")]
    ConfigSet(#[from] crate::commands::config::ConfigSetError),
    /// A `config set` *batch* invocation was refused (#910, journey
    /// US-2): the set lattice plus the batch-only classes (bad arity,
    /// duplicate key) — matches!-able at the arm.
    #[error("config batch set refused")]
    ConfigBatch(#[from] crate::commands::config::ConfigBatchEditError),
    /// The in-process core failed to boot.
    #[error("core bootstrap failed")]
    Bootstrap(Box<BootstrapError>),
    /// The credential store failed.
    #[error("credential store failed")]
    Credential(#[from] CredentialError),
    /// The Deck-to-files move ceremony refused or failed a step.
    #[error("board move failed")]
    BoardMove(#[from] crate::board_move_ceremony::MoveError),
    /// The engine rejected a command.
    #[error("engine rejected the command")]
    Engine(Box<taskboard_state::ExecuteError>),
    /// A `habits --day` value's variant mismatches the concurrent
    /// `--weekly/--monthly` cadence flag (US-6: the CLI refuses the
    /// kind mismatch at the parse boundary; the domain would accept
    /// the field inertly).
    #[error("the --day kind mismatches the target cadence")]
    HabitDayKindMismatch,
    /// The server rejected the credentials (401/403).
    #[error("credentials rejected by the server")]
    InvalidCredentials,
    /// The server could not be reached.
    #[error("server unreachable")]
    Unreachable,
    /// No such (live, non-archived) board on the server.
    #[error("no such board")]
    UnknownBoard(u64),
    /// No live board matches the selector (exact uuid | exact title).
    #[error("no live board matches {selector:?}")]
    UnknownBoardSelector {
        /// The rejected selector, verbatim.
        selector: String,
    },
    /// A different board is already bound (single-binding invariant).
    #[error("board {} is already bound to a different remote board", board)]
    BoardAlreadyBound {
        /// The local board's uuid (receipt-side rendering format).
        board: String,
        /// The currently bound remote board id.
        current: u64,
    },
    /// The direct files-publish prelude (`boards publish --to-files`,
    /// #764 journey US-2/S3) names an archived board: the GUI ceremony's
    /// gone ruling refuses non-live boards, and "archived" is neither
    /// the engine's `UnknownBoard` nor `BoardDeleted` class — the engine
    /// deliberately keeps archived boards commandable, so this verb's
    /// live-row check is app-layer. The GUI settings arm folds it into
    /// its `Gone` class verbatim (zero behavior delta).
    #[error("board {} is archived", board.as_uuid())]
    BoardArchived {
        /// The archived board.
        board: taskboard_domain::BoardId,
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
    /// The resolved task is the sidecar anchor card: renaming or
    /// deleting it would break the per-board app-data document. The
    /// guard lives app-layer by ruling — the engine never learns of it.
    /// Re-asserting the identical title is a no-op edit and passes.
    #[error("this task is the app-data anchor card and cannot be renamed or deleted")]
    AnchorTaskReserved,
    /// The resolved task is completed: a repeat *pattern* cannot be set
    /// on a done head (its occurrence closed; the successor carries the
    /// series forward). The guard lives app-layer by ruling — the
    /// engine never learns of it; `--off` deliberately passes (clearing
    /// a dead rule is honest cleanup).
    #[error("task {selector:?} is done — repeating targets an open task")]
    TaskDone {
        /// The rejected selector, verbatim.
        selector: String,
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
    /// No reminder rule of the target task matches the selector.
    #[error("no such reminder rule")]
    UnknownReminderRule(String),
    /// The rule selector matches several of the task's rules; carries
    /// their full uuids (deterministic rule-id order).
    #[error("rule selector matches several rules — pass one of {}", candidates.join(" | "))]
    AmbiguousReminderRule {
        /// The matched rules' full uuids.
        candidates: Vec<String>,
    },
    /// No standalone alarm matches the selector.
    #[error("no such timer/alarm")]
    UnknownAdhocAlarm(String),
    /// No break rule matches the selector.
    #[error("no such break rule")]
    UnknownBreakRule(String),
    /// The alarm selector matches several alarms; carries their full
    /// uuids (deterministic alarm-id order).
    #[error("timer selector matches several alarms — pass one of {}", candidates.join(" | "))]
    AmbiguousAdhocAlarm {
        /// The matched alarms' full uuids.
        candidates: Vec<String>,
    },
    /// No time entry matches the selector (the app-side selector
    /// resolution miss — the *domain*
    /// `CommandError::UnknownTimeEntry(TimeEntryId)` keeps its own kind
    /// via `domain_error_kind`; this variant only serves resolution
    /// before a command is built).
    /// No journal entry matches the `--from` selector (a miss, an
    /// evicted pre-image, or a non-delete/`None`-pre-image target — the
    /// roster is the honest row source, D-5/N5; this variant only
    /// serves resolution before a command is built).
    #[error("no restorable journal entry matches {selector:?}")]
    UnknownJournalEntry {
        /// The refused selector, verbatim.
        selector: String,
    },
    /// The `--from` prefix matches several restorable deletes; carries
    /// their full uuids.
    #[error("journal selector matches several deletes — pass one of {}", candidates.join(" | "))]
    AmbiguousJournalEntry {
        /// The matched journal ids' full uuids.
        candidates: Vec<String>,
    },
    /// No time entry matches the selector (the app-side selector
    /// resolution miss — the *domain*
    /// `CommandError::UnknownTimeEntry(TimeEntryId)` keeps its own kind
    /// via `domain_error_kind`; this variant only serves resolution
    /// before a command is built).
    #[error("no time entry matches {selector:?}")]
    UnknownTimeEntry {
        /// The rejected selector, verbatim.
        selector: String,
    },
    /// The entry selector matches several entries; carries their full
    /// uuids (deterministic entry-id order).
    #[error("entry selector matches several entries — pass one of {}", candidates.join(" | "))]
    AmbiguousTimeEntry {
        /// The matched entries' full uuids.
        candidates: Vec<String>,
    },
    /// No live reflection entry matches the selector (exact uuid |
    /// ≥8-char prefix; tombstoned entries are gone from the family's
    /// own recovery surface — Q-7).
    #[error("no reflection entry matches {selector:?}")]
    UnknownReflectionEntry {
        /// The rejected selector, verbatim.
        selector: String,
    },
    /// The reflection-entry selector matches several live entries;
    /// carries their full uuids (deterministic entry-id order).
    #[error("reflection entry selector matches several entries — pass one of {}", candidates.join(" | "))]
    AmbiguousReflectionEntry {
        /// The matched entries' full uuids.
        candidates: Vec<String>,
    },
    /// `--what journal` received a tracking-only filter (`--task`/
    /// `--board`): the ruling names `--since`/`--until` the journal
    /// leg's only filters, and silently ignoring a passed filter is
    /// the worst outcome for a data command.
    #[error("--task/--board/--since/--until/--format do not apply to this export family")]
    ExportFiltersNotApplicable,
    /// The pdf-only options (`--layout`/`--out`/`--no-open`) were
    /// passed to a data family (`tracking`/`journal`).
    #[cfg(feature = "pdf-export")]
    #[error("--layout/--out/--no-open apply only to --what pdf")]
    ExportOptionsNotApplicable,
    /// The pdf destination was refused (not absolute after resolution,
    /// parent missing, an existing directory, or the write failed) —
    /// typed path outcomes, never a silent stdout dump.
    #[cfg(feature = "pdf-export")]
    #[error("pdf destination refused")]
    ExportPdfPath(#[from] crate::pdf::deliver::PdfPathError),
    /// `[export] pdf_layout` names an unknown layout.
    #[cfg(feature = "pdf-export")]
    #[error("[export] pdf_layout is not one of fold|a4|a5|a6")]
    UnknownPdfLayout,
    /// The krilla renderer refused the display list — a layout bug,
    /// never a runtime contingency.
    #[cfg(feature = "pdf-export")]
    #[error("pdf rendering failed")]
    ExportPdfRender,
    /// No habit matches the selector (exact uuid | ≥8-char prefix |
    /// exact live title).
    #[error("no habit matches {selector:?}")]
    UnknownHabit {
        /// The rejected selector, verbatim.
        selector: String,
    },
    /// The habit selector matches several live habits; carries their
    /// full uuids (deterministic habit-id order).
    #[error("habit selector matches several habits — pass one of {}", candidates.join(" | "))]
    AmbiguousHabit {
        /// The matched habits' full uuids.
        candidates: Vec<String>,
    },
    /// There is no stack to target.
    #[error("no stacks to target")]
    NoStacks,
    /// The board-scoped default resolution failed: zero live boards (no
    /// candidates) or ≥2 (all live candidates listed, `--board` picks
    /// one). The CLI-layer typed ambiguity refusal — the engine's own
    /// named-id refusals keep their kinds via `domain_error_kind`.
    #[error("no single live board to target — pass --board <uuid> (live boards: {})", candidates.join(", "))]
    AmbiguousBoard {
        /// The live boards' uuids (deterministic board-id order; empty
        /// when zero boards are live).
        candidates: Vec<String>,
    },
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
    /// The daemon's sync connector was detached (WI-H: `logout` after
    /// boot): the engine answers `RequestSync` with `NoOp` — the verb
    /// refuses typed instead of lying via `sync-timed-out`.
    #[error("no sync connector is attached")]
    SyncUnavailable,
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
    /// The config pins `sync.backend = "standalone"` and the flip was
    /// neither flagged (`connect --switch-backend`) nor confirmed on a
    /// terminal.
    #[error("backend is explicitly stand-alone — pass --switch-backend to switch it")]
    BackendExplicitStandalone,
    /// The session ceremony could not attach the refreshed connector
    /// to the running core (ADR 0014): already-attached (unreachable
    /// after the ceremony's detach-first step), the engine gone, or
    /// draining — recoverable with another `connect` line.
    #[error("the connector could not be attached to the running session")]
    Attach(#[from] crate::bootstrap::AttachError),
    /// A different connection is already configured and the re-point
    /// was neither forced (`connect --force`) nor confirmed on a
    /// terminal.
    #[error("a different connection is already configured — pass --force to re-point it")]
    ConnectionExists,
    /// The IPC transport failed mid-request (connection death, frame
    /// garbage) — the daemon holds the instance lock, so in-process
    /// fallback would only report `instance-locked`; the typed failure
    /// is the honest signal (ADR 0012).
    #[error("daemon ipc transport failed")]
    IpcTransport,
    /// The daemon did not answer within the client's derived read
    /// budget (ADR 0012, amended): verbs still self-bound, the deadline
    /// exists for the wedged-daemon case. Distinct from
    /// [`CommandError::IpcTransport`] — a timeout is a *silent* daemon,
    /// not a broken exchange. Exit 1 via `report` like every failure.
    #[error("daemon ipc read timed out")]
    IpcTimeout,
    /// A destructive/repair ceremony (`boards delete`/`boards reset`)
    /// ran without `--confirm`; the payload states what would go so the
    /// refusal is actionable. Confirmation is CLI-layer policy only —
    /// the domain command stays pure and total.
    #[error("pass --confirm to {verb}: {stacks} stacks, {tasks} tasks, {labels} labels and {pending_ops} pending ops would be affected{}", bound.then(|| " (the bound remote board's content is destroyed server-side by delete)").unwrap_or(""))]
    ConfirmationRequired {
        /// The ceremony being confirmed (`delete` | `reset`).
        verb: String,
        /// Live stacks affected.
        stacks: usize,
        /// Live tasks affected.
        tasks: usize,
        /// Live labels affected.
        labels: usize,
        /// Pending ops affected.
        pending_ops: usize,
        /// Whether the board is bound (delete destroys remote content).
        bound: bool,
    },
    /// `boards unbind` ran without `--confirm`; the payload states the
    /// scope so the refusal is actionable (the bound remote, the ops
    /// that will strand, the refs a future different-remote rebind
    /// would wipe). Confirmation is CLI-layer policy only.
    #[error(
        "pass --confirm to unbind board {}: bound remote {:?}, {} pending op(s) would strand, {} descendant ref(s) kept",
        board,
        bound_remote,
        pending_ops,
        descendant_refs
    )]
    UnbindConfirmationRequired {
        /// The board's uuid (receipt-side rendering format).
        board: String,
        /// The currently bound remote board id, if any.
        bound_remote: Option<u64>,
        /// Outbox ops that would strand.
        pending_ops: usize,
        /// Live descendant refs kept (wiped by a future different-remote
        /// rebind).
        descendant_refs: usize,
    },
    /// `boards reconcile` ran without `--confirm`; the payload states
    /// the match counts the pass would land (computed by the pure
    /// planner — a count, not a dry-run mode).
    #[error(
        "pass --confirm to reconcile board {}: {} stack(s), {} label(s), {} task(s) would merge, {} duplicate create-op(s) would be cancelled",
        board,
        stacks,
        labels,
        tasks,
        cancelled_ops
    )]
    ReconcileConfirmationRequired {
        /// The board's uuid (receipt-side rendering format).
        board: String,
        /// Merged stack pairs.
        stacks: usize,
        /// Merged label pairs.
        labels: usize,
        /// Merged task pairs.
        tasks: usize,
        /// Cancelled duplicate create-ops.
        cancelled_ops: usize,
    },
    /// The daemon speaks a different wire-protocol version.
    #[error("daemon ipc protocol version mismatch")]
    IpcVersion {
        /// The server's protocol version.
        server_v: u32,
    },
    /// A `--connector` flag named no configured connector (D6-5): the
    /// guard lives handler-side where the configured set lives.
    #[error("no connector named {:?}", name)]
    UnknownConnector {
        /// The offending name.
        name: String,
    },
    /// `boards move --to-connection` named a configured connector
    /// that is not Deck-typed: the rebind is a Deck-to-Deck move
    /// (the files-sync-status plan US-10, D-11's typed refusal).
    #[error("connector {:?} is not a Deck connection", name)]
    RebindTargetNotDeck {
        /// The named (configured, non-Deck) connector.
        name: String,
    },
    /// `boards move --to-connection` on a board whose binding has no
    /// Deck-to-Deck move (unbound, files-bound, or a same-connector
    /// pick — a typed refusal, not a no-op; D-11's refusal set).
    #[error("the board has no Deck-to-Deck move")]
    RebindRefused,
    /// A REPL-only line (`boards use`) was invoked one-shot: the
    /// selection is session state; there is nothing to select for a
    /// single invocation.
    #[error("boards use is an interactive-session verb — run it inside the taskboard session")]
    SessionOnly,
    /// SIGINT cancelled the running line (ADR 0011's amendment): the
    /// line's future was dropped, the session continues. A failed line
    /// never aborts the session — a cancelled one is exactly that.
    #[error("cancelled")]
    Cancelled,
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
    #[allow(clippy::too_many_lines)] // one kind table, kept flat for review
    pub fn error_kind(&self) -> &'static str {
        match self {
            Self::Config(err) => err.kind(),
            Self::ConfigSet(err) => match err {
                crate::commands::config::ConfigSetError::EnvLocked { .. } => "config-env-locked",
                crate::commands::config::ConfigSetError::UnknownKey { .. } => "config-unknown-key",
                crate::commands::config::ConfigSetError::BadValue { .. } => "config-bad-value",
                crate::commands::config::ConfigSetError::Io(_) => "config-write-failed",
                crate::commands::config::ConfigSetError::Rejected(_) => "config-write-rejected",
            },
            Self::ConfigBatch(err) => match err {
                crate::commands::config::ConfigBatchEditError::DuplicateKey { .. } => {
                    "config-duplicate-key"
                }
                crate::commands::config::ConfigBatchEditError::EnvLocked { .. } => {
                    "config-env-locked"
                }
                crate::commands::config::ConfigBatchEditError::UnknownKey { .. } => {
                    "config-unknown-key"
                }
                // `BadArity` is `BadValue`-class per the plan (US-2/S1):
                // the even-pair grammar's refusal shares the kind.
                crate::commands::config::ConfigBatchEditError::BadArity
                | crate::commands::config::ConfigBatchEditError::BadValue { .. } => {
                    "config-bad-value"
                }
                crate::commands::config::ConfigBatchEditError::Io(_) => "config-write-failed",
                crate::commands::config::ConfigBatchEditError::Rejected(_) => {
                    "config-write-rejected"
                }
            },
            Self::Bootstrap(err) => match **err {
                BootstrapError::InstanceLocked { .. } => "instance-locked",
                _ => "bootstrap",
            },
            Self::Credential(_) => "credential",
            Self::Engine(err) => match &**err {
                taskboard_state::ExecuteError::Rejected(rejected) => domain_error_kind(rejected),
                _ => "engine",
            },
            Self::AnchorTaskReserved => "anchor-task-reserved",
            Self::TaskDone { .. } => "task-done",
            Self::InvalidCredentials => "invalid-credentials",
            Self::Unreachable => "unreachable",
            Self::UnknownBoard(_) | Self::UnknownBoardSelector { .. } => "unknown-board",
            Self::BoardAlreadyBound { .. } => "board-already-bound",
            Self::BoardArchived { .. } => "board-archived",
            Self::UnknownTask(_) => "unknown-task",
            Self::AmbiguousTask { .. } => "ambiguous-task",
            Self::UnknownStack(_) => "unknown-stack",
            Self::AmbiguousStack { .. } => "ambiguous-stack",
            Self::UnknownLabel(_) => "unknown-label",
            Self::AmbiguousLabel { .. } => "ambiguous-label",
            Self::UnknownReminderRule(_) => "unknown-reminder-rule",
            Self::AmbiguousReminderRule { .. } => "ambiguous-reminder-rule",
            Self::UnknownAdhocAlarm(_) => "unknown-adhoc-alarm",
            Self::UnknownBreakRule(_) => "unknown-break-rule",
            Self::AmbiguousAdhocAlarm { .. } => "ambiguous-adhoc-alarm",
            Self::UnknownJournalEntry { .. } => "unknown-journal-entry",
            Self::AmbiguousJournalEntry { .. } => "ambiguous-journal-entry",
            Self::UnknownTimeEntry { .. } => "unknown-time-entry",
            Self::AmbiguousTimeEntry { .. } => "ambiguous-time-entry",
            Self::UnknownReflectionEntry { .. } => "unknown-reflection-entry",
            Self::AmbiguousReflectionEntry { .. } => "ambiguous-reflection-entry",
            Self::ExportFiltersNotApplicable => "filters-not-applicable",
            #[cfg(feature = "pdf-export")]
            Self::ExportOptionsNotApplicable => "options-not-applicable",
            #[cfg(feature = "pdf-export")]
            Self::ExportPdfPath(_) => "pdf-path",
            #[cfg(feature = "pdf-export")]
            Self::UnknownPdfLayout => "unknown-pdf-layout",
            #[cfg(feature = "pdf-export")]
            Self::ExportPdfRender => "pdf-render",
            Self::HabitDayKindMismatch => "habit-day-kind-mismatch",
            Self::UnknownHabit { .. } => "unknown-habit",
            Self::AmbiguousHabit { .. } => "ambiguous-habit",
            Self::NoStacks => "no-stacks",
            Self::AmbiguousBoard { .. } => "ambiguous-board",
            Self::SyncFailed(_) => "sync-failed",
            Self::SyncTimedOut => "sync-timed-out",
            Self::ConfigWrite(_) => "config-write",
            Self::Prompt(_) => "prompt",
            Self::Deck(_) => "deck",
            Self::SyncDisabled => "sync-disabled",
            Self::SyncUnavailable => "sync-unavailable",
            Self::LocalBoardExists => "local-board-exists",
            Self::NotConfigured => "not-configured",
            Self::NoDaemon => "no-daemon",
            Self::BackendExplicitStandalone => "backend-explicit-standalone",
            Self::Attach(_) => "attach-failed",
            Self::ConnectionExists => "connection-exists",
            Self::ConfirmationRequired { .. } => "confirmation-required",
            Self::UnbindConfirmationRequired { .. } => "unbind-confirmation-required",
            Self::BoardMove(err) => err.kind(),
            Self::ReconcileConfirmationRequired { .. } => "reconcile-confirmation-required",
            Self::IpcTransport => "ipc-transport",
            Self::IpcTimeout => "ipc-timeout",
            Self::IpcVersion { .. } => "ipc-version",
            Self::UnknownConnector { .. } => "unknown-connector",
            Self::RebindTargetNotDeck { .. } => "rebind-target-not-deck",
            Self::RebindRefused => "rebind-refused",
            Self::SessionOnly => "session-only",
            Self::Cancelled => "cancelled",
        }
    }

    /// The optional structural detail (ids, nested kinds) of the
    /// `--json` error shape.
    #[must_use]
    pub fn error_detail(&self) -> Option<String> {
        match self {
            Self::Engine(err)
                if matches!(
                    &**err,
                    taskboard_state::ExecuteError::Rejected(
                        taskboard_domain::CommandError::BoardAlreadyBound { .. }
                    )
                ) =>
            {
                let taskboard_state::ExecuteError::Rejected(
                    taskboard_domain::CommandError::BoardAlreadyBound { board, bound },
                ) = &**err
                else {
                    unreachable!("guarded above")
                };
                // The Deck leg keeps the pre-widening bare-number
                // detail (observed CLI output is unchanged); a Files
                // payload has no wire number and shows its text form.
                let bound_detail = match bound.as_deck() {
                    Some(num) => num.to_string(),
                    None => bound.to_string(),
                };
                Some(format!(
                    "{} (bound remote {})",
                    board.as_uuid(),
                    bound_detail
                ))
            }
            Self::UnknownBoard(id) => Some(id.to_string()),
            Self::UnknownConnector { name } | Self::RebindTargetNotDeck { name } => {
                Some(name.clone())
            }
            Self::Engine(err)
                if matches!(
                    &**err,
                    taskboard_state::ExecuteError::Rejected(
                        taskboard_domain::CommandError::TargetNotUndoable(_)
                            | taskboard_domain::CommandError::TargetNoLongerInvertible { .. }
                            | taskboard_domain::CommandError::UndoTargetHasDependents { .. }
                            | taskboard_domain::CommandError::TargetNotInLine,
                    )
                ) =>
            {
                let taskboard_state::ExecuteError::Rejected(rejected) = &**err else {
                    unreachable!("guarded above")
                };
                match rejected {
                    taskboard_domain::CommandError::TargetNotUndoable(reason) => {
                        Some(not_undoable_reason_kind(*reason).to_owned())
                    }
                    taskboard_domain::CommandError::TargetNoLongerInvertible { entity, field } => {
                        Some(format!("{} {}", journal_entity_id(*entity), undo_field_kind(*field)))
                    }
                    taskboard_domain::CommandError::UndoTargetHasDependents { stack } => {
                        Some(stack.as_uuid().to_string())
                    }
                    taskboard_domain::CommandError::TargetNotInLine => {
                        // No payload: the stale-row refusal names no
                        // entity (the head is the fact).
                        None
                    }
                    _ => unreachable!("guarded above"),
                }
            }
            Self::BoardAlreadyBound { board, current } => {
                Some(format!("{board} (bound remote {current})"))
            }
            Self::TaskDone { selector } => Some(selector.clone()),
            Self::UnknownTask(sel)
            | Self::UnknownStack(sel)
            | Self::UnknownLabel(sel)
            | Self::UnknownReminderRule(sel)
            | Self::UnknownAdhocAlarm(sel)
            | Self::UnknownBreakRule(sel)
            | Self::UnknownJournalEntry { selector: sel }
            | Self::UnknownTimeEntry { selector: sel }
            | Self::UnknownReflectionEntry { selector: sel } => Some(sel.clone()),
            Self::AmbiguousTask { candidates }
            | Self::AmbiguousStack { candidates }
            | Self::AmbiguousReminderRule { candidates }
            | Self::AmbiguousAdhocAlarm { candidates }
            | Self::AmbiguousJournalEntry { candidates }
            | Self::AmbiguousTimeEntry { candidates }
            | Self::AmbiguousReflectionEntry { candidates }
            | Self::AmbiguousBoard { candidates } => Some(candidates.join(", ")),
            Self::SyncFailed(kind) => Some(output::sync_error_kind(*kind).to_owned()),
            Self::IpcVersion { server_v } => Some(server_v.to_string()),
            Self::ConfirmationRequired { verb, .. } => Some(verb.clone()),
            Self::UnbindConfirmationRequired { board, .. }
            | Self::ReconcileConfirmationRequired { board, .. } => Some(board.clone()),
            // UX text (never asserted): the orphan-completion rule —
            // the cancel drops the line's wait, not an engine command
            // already delivered.
            Self::Cancelled => Some(
                "the operation was interrupted; a command already handed to the engine may still have taken effect"
                    .to_owned(),
            ),
            _ => None,
        }
    }
}

/// The engine's semantic rejections keep their own kinds in the JSON
/// error shape (`unknown-board`, `unknown-task`, …); everything else is
/// the generic `engine`.
fn domain_error_kind(err: &taskboard_domain::CommandError) -> &'static str {
    use taskboard_domain::CommandError as D;
    match err {
        D::UnknownBoard(_) | D::BoardDeleted(_) => "unknown-board",
        // The rebind verb's refusal (US-10, D-11): no Deck binding to
        // move (or a raced same-connector pick).
        D::NotDeckBound => "not-deck-bound",
        D::UnknownTask(_) | D::TaskDeleted(_) => "unknown-task",
        D::UnknownStack(_) | D::StackDeleted(_) => "unknown-stack",
        D::UnknownLabel(_) | D::LabelDeleted(_) => "unknown-label",
        D::BoardAlreadyBound { .. } => "board-already-bound",
        D::RemoteBoardInUse { .. } => "remote-board-in-use",
        D::UnknownReminderRule(_) => "unknown-reminder-rule",
        D::UnknownAdhocAlarm(_) => "unknown-adhoc-alarm",
        D::UnknownBreakRule(_) => "unknown-break-rule",
        D::BreakRuleBoardMissing => "break-rule-board-missing",
        D::InvalidAdhocWhen => "invalid-adhoc-when",
        D::InvalidReminderRuleAt => "invalid-reminder-rule-at",
        D::InvalidSnoozePostpone => "invalid-snooze-postpone",
        D::UnknownTimeEntry(_) => "unknown-time-entry",
        D::UnknownJournalEntry(_) => "unknown-journal-entry",
        D::JournalEntryNotTimeEntryDelete(_) => "journal-entry-not-a-delete",
        D::JournalEntryPreImageMissing(_) => "journal-entry-pre-image-missing",
        D::InvalidSpan => "invalid-span",
        D::TrackerTaskNotDoneable(_) => "tracker-not-doneable",
        D::NotRecurring(_) => "not-recurring",
        D::UnknownHabit(_) | D::HabitDeleted(_) => "unknown-habit",
        D::InvalidHabitWindow { .. } => "invalid-habit-window",
        D::InvalidHabitPause { .. } => "invalid-habit-pause",

        // HF4 R-6: the daily habit's typed refusal (the field is a
        // weekly/monthly extension by construction).
        D::ScheduledDaysOnDailyHabit(_) => "scheduled-days-on-daily-habit",
        // The undo line's refusals are their own surface family
        // (ADR 0019); `EngineRouted` is the catalogue's totality arm.
        D::EngineRouted => "engine-routed",
        D::NothingToUndo => "nothing-to-undo",
        D::NothingToRedo => "nothing-to-redo",
        // The #355 scoped refusals: their own kinds so a JSON consumer
        // can tell "the line is exhausted" from "the frontier belongs
        // to another board" (the D-5 legibility rule).
        D::NothingToUndoForBoard { .. } => "nothing-to-undo-for-board",
        D::NothingToRedoForBoard { .. } => "nothing-to-redo-for-board",
        D::TargetNotUndoable(_) => "target-not-undoable",
        D::TargetNoLongerInvertible { .. } => "target-not-invertible",
        // UF2 (#353): the branch-picker's stale-row refusal.
        D::TargetNotInLine => "target-not-in-line",
        D::UndoTargetHasDependents { .. } => "undo-target-has-dependents",
        // The #901 restore-family divergence refusal (ids are minted
        // once — a present same-id row is divergence, never clobbered).
        D::TimeEntryAlreadyPresent(_) => "time-entry-already-present",
        // DR-7 chunk 1: the reflection verbs exist as engine commands
        // only; the CLI's `journal` verb family (chunk 2, #637) will map
        // its own rejections — until then these kinds surface the typed
        // facts in the JSON error shape.
        D::UnknownReflection(_) => "unknown-reflection",
        D::InvalidReviewWindow => "invalid-review-window",
        D::InvalidRating => "invalid-rating",
        // The Today compact-move boundary (US-14, #1031): the pair
        // refuses typed at the stack edge — its own JSON kind (the
        // legibility rule; a consumer can tell the boundary from an
        // unknown task).
        D::TaskAtBoundary(_) => "task-at-boundary",
    }
}

/// The kebab discriminators of the undo refusals' payload enums (the
/// `--json` detail field names structural facts a script can branch
/// on — ids and enum kinds, never prose; ADR 0019's typed surface).
fn not_undoable_reason_kind(reason: taskboard_domain::NotUndoableReason) -> &'static str {
    use taskboard_domain::NotUndoableReason as R;
    match reason {
        R::RestoreClass => "restore-class",
        R::BoardCeremony => "board-ceremony",
        R::ReminderSurface => "reminder-surface",
        R::TrackingSurface => "tracking-surface",
        R::HabitSurface => "habit-surface",
        R::ReflectionSurface => "reflection-surface",
    }
}

fn undo_field_kind(field: taskboard_domain::UndoField) -> &'static str {
    use taskboard_domain::UndoField as F;
    match field {
        F::Title => "title",
        F::Description => "description",
        F::Duedate => "duedate",
        F::Scheduled => "scheduled",
        F::Done => "done",
        F::Position => "position",
        F::Effort => "effort",
        F::Importance => "importance",
        F::Urgency => "urgency",
        F::Recur => "recur",
        F::Kind => "kind",
        F::Archived => "archived",
        F::Deleted => "deleted",
        F::Order => "order",
        F::Color => "color",
        F::Membership => "membership",
        F::UpdatedAt => "updated-at",
        F::HabitTarget => "habit-target",
        F::HabitAnchorTime => "habit-anchor-time",
        F::HabitLogCount => "habit-log-count",
    }
}

fn journal_entity_id(entity: taskboard_domain::JournalEntity) -> String {
    use taskboard_domain::JournalEntity as E;
    match entity {
        E::Board(id) => id.as_uuid().to_string(),
        E::Stack(id) => id.as_uuid().to_string(),
        E::Task(id) => id.as_uuid().to_string(),
        E::Label(id) => id.as_uuid().to_string(),
        E::Habit(id) => id.as_uuid().to_string(),
        E::TimeEntry(id) => id.as_uuid().to_string(),
        E::HabitLog(key) => key.wire_form(),
        E::ReminderRule(id) => id.as_uuid().to_string(),
    }
}

/// The production credential store for a config-selected kind. Tests
/// inject fakes by calling the handlers directly.
#[must_use]
pub fn production_store(
    kind: CredentialStoreKind,
    base_dir: &std::path::Path,
) -> Arc<dyn CredentialStore> {
    match kind {
        CredentialStoreKind::Keyring => Arc::new(crate::secrets::KeyringStore),
        CredentialStoreKind::Env => Arc::new(crate::secrets::EnvCredentialStore),
        CredentialStoreKind::File => Arc::new(crate::secrets::FileCredentialStore::new(base_dir)),
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

/// The reply contract pins one outcome per verb (plus the accepted
/// `NoOp` where the planner allows it); any other reply is an engine
/// contract bug — surfaced as the generic engine kind, never
/// misreported as a user error.
///
/// # Mutant disposition (plan `2026-11-22-commands-predicate-coverage`
/// §4 D4, #1231)
///
/// The `_ => Err(outcome_mismatch())` arms at the call sites are
/// *engine-contract guards*: they are reachable only when the engine
/// violates its own reply catalogue, and the real engine over the
/// sanctioned in-memory fake cannot produce those outcomes (an engine
/// that lies would need a mock, which the fakes-over-mocks law
/// forbids). They are deliberately excluded from the cargo-mutants
/// kill-target set — documented survivors by construction, per the
/// recorded-disposition law; the *reachable* outcome arms (the
/// Applied/NoOp/HabitCreated mappings) are kill targets and pinned by
/// the predicate tables.
pub(crate) fn outcome_mismatch() -> CommandError {
    CommandError::Engine(Box::new(taskboard_state::ExecuteError::EngineGone))
}

/// Resolves the board a board-scoped verb targets: the `--board` flag
/// when given (typed lookup refusals when the id names nothing / a
/// tombstone — the same kinds the engine would refuse with, surfaced
/// before construction), else the single-live-board default (typed
/// refusal when zero or ≥2 live boards make it ambiguous).
///
/// # Errors
///
/// [`CommandError::AmbiguousBoard`] for the zero/≥2 default, the
/// engine's named-id rejection kinds for a flag that does not resolve.
pub(crate) fn resolve_board(
    engine: &taskboard_state::EngineHandle,
    flag: Option<taskboard_domain::BoardId>,
) -> Result<taskboard_domain::BoardId, CommandError> {
    use taskboard_domain::CommandError as D;
    let state = engine.shared_state().load_full();
    if let Some(id) = flag {
        return match state.boards.get(&id) {
            None => Err(CommandError::Engine(Box::new(
                taskboard_state::ExecuteError::Rejected(D::UnknownBoard(id)),
            ))),
            Some(b) if b.deleted => Err(CommandError::Engine(Box::new(
                taskboard_state::ExecuteError::Rejected(D::BoardDeleted(id)),
            ))),
            Some(_) => Ok(id),
        };
    }
    let mut live: Vec<(taskboard_domain::BoardId, String)> = state
        .boards
        .values()
        .filter(|b| !b.deleted)
        .map(|b| (b.id, b.id.as_uuid().to_string()))
        .collect();
    if live.len() == 1 {
        Ok(live.remove(0).0)
    } else {
        // Zero and ≥2 share the ambiguity refusal; candidates stay in
        // board-id order (BTreeMap iteration order) — the tokens `--board`
        // accepts.
        Err(CommandError::AmbiguousBoard {
            candidates: live.into_iter().map(|(_, uuid)| uuid).collect(),
        })
    }
}

/// [`resolve_board`]'s bind-ceremony sibling (D3-7): zero live boards is
/// the *fresh path* (`Ok(None)`) — a board-less install mints and binds,
/// it is not ambiguous. `--board` rides the typed lookup refusals; ≥2
/// live boards without a flag still refuse [`CommandError::AmbiguousBoard`].
///
/// # Errors
///
/// [`CommandError::AmbiguousBoard`] for ≥2 live boards without a flag,
/// the engine's named-id rejection kinds for a flag that does not
/// resolve.
pub(crate) fn resolve_board_or_none_for_bind(
    app: &crate::App,
) -> Result<Option<taskboard_domain::BoardId>, CommandError> {
    let state = app.engine.shared_state().load_full();
    let mut live: Vec<taskboard_domain::BoardId> = state
        .boards
        .values()
        .filter(|b| !b.deleted)
        .map(|b| b.id)
        .collect();
    match live.len() {
        0 => Ok(None),
        1 => Ok(Some(live.remove(0))),
        _ => Err(CommandError::AmbiguousBoard {
            candidates: live.iter().map(|id| id.as_uuid().to_string()).collect(),
        }),
    }
}

/// One subcommand run to completion. Returns the process exit code:
/// `0` success (including idempotent `NoOp` repeats), `1` runtime
/// failure. Exit `2` (clap usage errors) and `130` (second-signal force
/// exit) never flow through here.
///
/// # Panics
///
/// Never on a validated config: `core_ctx` yields `AppConfig::load`ed
/// configs only, so the connector-name mint in the `tasks list` arm
/// cannot fail (same rule as `AppConfig::server_url`).
#[allow(clippy::too_many_lines)] // one arm per subcommand, kept flat for review
pub async fn dispatch(command: Command, mut ctx: CommandCtx<'_>) -> u8 {
    let json = ctx.io.json;
    tracing::info!(?command, config = ?ctx.config_path, json, "running command");
    match command {
        Command::Connect {
            server,
            user,
            switch_backend,
            force,
        } => {
            connect_command(
                &mut ctx.io,
                ctx.config_path,
                connect::ConnectArgs {
                    server,
                    user,
                    switch_backend,
                    force,
                    confirmable: true,
                },
            )
            .await
        }
        Command::Login { server, user } => {
            login_command(&mut ctx.io, ctx.config_path, server, user).await
        }
        Command::Logout => logout_command(&mut ctx.io, ctx.config_path.clone()).await,
        Command::Boards {
            cmd: BoardsCmd::List { connector },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                |entries: &Vec<_>| boards::render_text_list(entries),
                move |config, store| async move {
                    boards::list(&config, &store, connector.as_deref()).await
                },
            )
            .await
        }
        Command::Boards {
            cmd:
                BoardsCmd::Select {
                    board,
                    local_board,
                    timeout_secs,
                    bind_local_board,
                    connector,
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
                        local_board,
                        Duration::from_secs(timeout_secs),
                        bind_local_board,
                        connector.as_deref(),
                    )
                    .await
                },
            )
            .await
        }
        Command::Boards {
            cmd: BoardsCmd::Use { .. },
        } => {
            // Session state has no one-shot home (D6-6): the selection
            // is the REPL's; the daemon socket refuses it routing-level.
            report_to(&mut ctx.io, &CommandError::SessionOnly)
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
            cmd: BoardsCmd::Rename { board, title },
        } => run_core(
            &mut ctx.io,
            ctx.config_path.clone(),
            boards::render_text_rename,
            move |config, store| async move { boards::rename(config, store, board, &title).await },
        )
        .await,
        Command::Boards {
            cmd:
                BoardsCmd::Publish {
                    board,
                    to_files,
                    timeout_secs,
                    connector,
                },
        } => {
            if to_files {
                // The direct local-to-Files bind (#764): the same
                // receipt family, a different ceremony.
                run_core(
                    &mut ctx.io,
                    ctx.config_path.clone(),
                    boards::render_text_files_publish,
                    move |config, store| async move {
                        boards::publish_to_files(
                            config,
                            store,
                            board,
                            Duration::from_secs(timeout_secs),
                        )
                        .await
                    },
                )
                .await
            } else {
                run_core(
                    &mut ctx.io,
                    ctx.config_path.clone(),
                    boards::render_text_select,
                    move |config, store| async move {
                        boards::publish(
                            config,
                            store,
                            board,
                            Duration::from_secs(timeout_secs),
                            connector.as_deref(),
                        )
                        .await
                    },
                )
                .await
            }
        }
        Command::Boards {
            cmd: BoardsCmd::Color { board, color },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                boards::render_text_board_edit,
                move |config, store| async move {
                    boards::color(config, store, board, taskboard_domain::Color::new(&color)).await
                },
            )
            .await
        }
        Command::Boards {
            cmd: BoardsCmd::Archive { board },
        } => run_core(
            &mut ctx.io,
            ctx.config_path.clone(),
            boards::render_text_board_edit,
            move |config, store| async move { boards::archive(config, store, board, true).await },
        )
        .await,
        Command::Boards {
            cmd: BoardsCmd::Unarchive { board },
        } => run_core(
            &mut ctx.io,
            ctx.config_path.clone(),
            boards::render_text_board_edit,
            move |config, store| async move { boards::archive(config, store, board, false).await },
        )
        .await,
        Command::Boards {
            cmd: BoardsCmd::Delete { board, confirm },
        } => run_core(
            &mut ctx.io,
            ctx.config_path.clone(),
            boards::render_text_destruct,
            move |config, store| async move { boards::delete(config, store, board, confirm).await },
        )
        .await,
        Command::Boards {
            cmd: BoardsCmd::Reset { board, confirm },
        } => run_core(
            &mut ctx.io,
            ctx.config_path.clone(),
            boards::render_text_destruct,
            move |config, store| async move { boards::reset(config, store, board, confirm).await },
        )
        .await,
        Command::Boards {
            cmd:
                BoardsCmd::Move {
                    board,
                    to_files: _,
                    undo,
                    timeout_secs,
                    ref to_connection,
                },
        } => {
            // The Deck-to-Deck rebind (the files-sync-status plan
            // US-10, D-11): the move-family's third direction — the
            // same ceremony route (boot, run, shutdown), the rebind's
            // own precondition list, no move budget (the verb is
            // atomic, D-11's payload note).
            if let Some(target) = to_connection {
                let target = target.clone();
                return run_core(
                    &mut ctx.io,
                    ctx.config_path.clone(),
                    crate::commands::boards::render_text_rebind,
                    move |config, store| async move {
                        let sink = crate::bootstrap::resolve_notifier_sink(&config);
                        let app =
                            crate::bootstrap::bootstrap(config.clone(), Arc::clone(&store), sink)
                                .await?;
                        let result =
                            crate::commands::boards::rebind_deck_to_deck(
                                &app, &config, board, &target,
                            )
                            .await;
                        let _ = app.shutdown().await;
                        result
                    },
                )
                .await;
            }
            // Exactly one flag is present (clap's required
            // non-multiple group); the mapping is explicit — `--undo`
            // is named, not the `_` fallback (D-3 of the
            // cli-verb-grammar-hygiene plan).
            let direction = if undo {
                crate::board_move_ceremony::MoveDirection::Undo
            } else {
                crate::board_move_ceremony::MoveDirection::ToFiles
            };
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                crate::board_move_ceremony::render_text,
                move |config, store| async move {
                    let sink = crate::bootstrap::resolve_notifier_sink(&config);
                    let app = crate::bootstrap::bootstrap(config.clone(), Arc::clone(&store), sink).await?;
                    let device_json = crate::board_move_ceremony::default_device_json()?;
                    let result = crate::board_move_ceremony::run_on(
                        &app.engine,
                        &config,
                        &store,
                        board,
                        direction,
                        &device_json,
                        Duration::from_secs(timeout_secs),
                    )
                    .await;
                    let _ = app.shutdown().await;
                    result
                },
            )
            .await
        }
        Command::Boards {
            cmd: BoardsCmd::Unbind { board, confirm },
        } => run_core(
            &mut ctx.io,
            ctx.config_path.clone(),
            boards::render_text_unbind,
            move |config, store| async move { boards::unbind(config, store, board, confirm).await },
        )
        .await,
        Command::Boards {
            cmd: BoardsCmd::Reconcile { board, confirm },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                boards::render_text_reconcile,
                move |config, store| async move {
                    boards::reconcile(config, store, board, confirm).await
                },
            )
            .await
        }
        Command::Stacks {
            cmd:
                StacksCmd::Add {
                    board,
                    title,
                    order,
                },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                stacks::render_text_add,
                move |config, store| async move {
                    stacks::add(config, store, board, &title, order).await
                },
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
            cmd: LabelsCmd::List { board },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                |rows: &Vec<_>| labels::render_text_list(rows),
                |config, store| async move { labels::list(config, store, board).await },
            )
            .await
        }
        Command::Labels {
            cmd:
                LabelsCmd::Add {
                    board,
                    title,
                    color,
                },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                labels::render_text_receipt,
                move |config, store| async move {
                    labels::add(config, store, board, &title, color.as_deref()).await
                },
            )
            .await
        }
        Command::Labels {
            cmd:
                LabelsCmd::Edit {
                    label,
                    flags: LabelEditFlags { title, color },
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
        Command::Labels {
            cmd: LabelsCmd::Restore { label },
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                labels::render_text_restore,
                move |config, store| async move { labels::restore(config, store, &label).await },
            )
            .await
        }
        Command::Tasks {
            cmd: TasksCmd::List { stack },
        } => match core_ctx(ctx.config_path.clone()) {
            Ok((config, store)) => {
                let standalone = crate::connectors::configured_connectors(&config)
                    .expect("connector names validated by AppConfig::load — core_ctx yields validated configs")
                    .is_empty();
                emit_result(
                    &mut ctx.io,
                    tasks::list(config, store, stack.as_deref()).await,
                    |list| tasks::render_text_list(list, standalone),
                )
            }
            Err(err) => report_to(&mut ctx.io, &err),
        },
        Command::Capture { title, board } => {
            run_mutation(&mut ctx.io, ctx.config_path.clone(), move |config, store| async move {
                capture::capture(config, store, title, board).await
            })
            .await
        }
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
            cmd: TasksCmd::Move {
                task,
                stack,
                board,
                order,
            },
        } => {
            run_mutation(
                &mut ctx.io,
                ctx.config_path.clone(),
                move |config, store| async move {
                    tasks::move_(config, store, &task, stack.as_deref(), board.as_deref(), order)
                        .await
                },
            )
            .await
        }
        Command::Tasks {
            cmd: TasksCmd::Edit { task, flags },
        } => {
            run_mutation(&mut ctx.io, ctx.config_path.clone(), move |config, store| async move {
                // The mint rides the execution seam: the config's day
                // calendar is reachable exactly here (US-1/D-5).
                let changes = task_changes(&flags, &config.resolved_day_calendar());
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
            cmd: remind @ TasksCmd::Remind { .. },
        } => match reminders::collapse(remind) {
            Some(invocation) => {
                reminders::remind_command(&mut ctx.io, ctx.config_path.clone(), invocation).await
            }
            None => unreachable!("the outer match selected the Remind arm"),
        },
        Command::Tasks {
            cmd: repeat @ TasksCmd::Repeat { .. },
        } => match repeat::collapse(repeat) {
            Some(invocation) => {
                repeat::repeat_command(&mut ctx.io, ctx.config_path.clone(), invocation).await
            }
            None => unreachable!("the outer match selected the Repeat arm"),
        },
        Command::Tasks {
            cmd: TasksCmd::Undo,
        } => run_core(
            &mut ctx.io,
            ctx.config_path.clone(),
            tasks::render_text_undo,
            move |config, store| async move { tasks::undo(config, store).await },
        )
        .await,
        Command::Tasks {
            cmd: TasksCmd::Redo,
        } => run_core(
            &mut ctx.io,
            ctx.config_path.clone(),
            tasks::render_text_undo,
            move |config, store| async move { tasks::redo(config, store).await },
        )
        .await,
        Command::Find {
            query,
            kind,
            limit,
        } => {
            run_core(
                &mut ctx.io,
                ctx.config_path.clone(),
                |rows: &Vec<_>| find::render_text_list(rows),
                move |config, store| async move {
                    find::find(config, store, &query, kind, limit).await
                },
            )
            .await
        }
        Command::Timers { cmd } => match timers::collapse(cmd) {
            Some(invocation) => {
                timers::timers_command(&mut ctx.io, ctx.config_path.clone(), invocation).await
            }
            None => unreachable!("clap requires exactly one of --in/--at on timers add"),
        },
        Command::Breaks { cmd } => match breaks::collapse(cmd) {
            Some(invocation) => {
                breaks::breaks_command(&mut ctx.io, ctx.config_path.clone(), invocation).await
            }
            None => unreachable!("clap cannot produce an unresolvable breaks add"),
        },
        Command::Track { cmd } => match track::collapse(cmd) {
            Some(invocation) => {
                track::track_command(&mut ctx.io, ctx.config_path.clone(), invocation).await
            }
            None => unreachable!("clap requires exactly one log mode (--for | --from --to)"),
        },
        Command::Habits { cmd } => {
            habits::habits_command(&mut ctx.io, ctx.config_path.clone(), habits::collapse(cmd))
                .await
        }
        Command::Reflection { cmd } => match reflection::collapse(cmd) {
            Some(invocation) => {
                reflection::reflection_command(&mut ctx.io, ctx.config_path.clone(), invocation)
                    .await
            }
            None => unreachable!("the journal grammar has no vacuous branch"),
        },
        Command::Export {
            what,
            format,
            since,
            until,
            task,
            board,
            #[cfg(feature = "pdf-export")]
            layout,
            #[cfg(feature = "pdf-export")]
            out,
            #[cfg(feature = "pdf-export")]
            no_open,
        } => {
            track::export_command(
                &mut ctx.io,
                ctx.config_path.clone(),
                track::ExportQuery {
                    what: what.unwrap_or_default(),
                    format,
                    since,
                    until,
                    task,
                    board,
                    #[cfg(feature = "pdf-export")]
                    layout,
                    #[cfg(feature = "pdf-export")]
                    out,
                    #[cfg(feature = "pdf-export")]
                    no_open,
                },
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
            connector,
        } => {
            sync_command(
                &mut ctx.io,
                ctx.config_path.clone(),
                no_wait,
                timeout_secs,
                connector.as_deref(),
            )
            .await
        }
        Command::Config { cmd } => config::handle(&cmd, &mut ctx).await,
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
        Command::Gui => unreachable!("main handles the gui lifecycle arm itself"),
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

/// One-shot `login` — the deprecated alias of `connect` (design D1):
/// one stderr notice, then the *same* ceremony body with the alias's
/// strictly narrower flag surface (no `--switch-backend`/`--force`, no
/// interactive confirm — the guards refuse typed, naming `connect`).
/// Resolves the config path (writing one if this is the first run —
/// with no config anywhere, the XDG config dir).
async fn login_command(
    io: &mut CommandIo<'_>,
    config_path: Option<PathBuf>,
    server: Option<String>,
    user: Option<String>,
) -> u8 {
    // First on stderr so the `--json` "last stderr line is the error
    // envelope" convention survives; the text is UX, never asserted.
    let _ = writeln!(
        io.err,
        "warning: `login` is a deprecated alias of `connect`"
    );
    connect_command(
        io,
        config_path,
        connect::ConnectArgs {
            server,
            user,
            switch_backend: false,
            force: false,
            confirmable: false,
        },
    )
    .await
}

/// One-shot `connect`: resolves the config path (writing one if this is
/// the first run — with no config anywhere, the XDG config dir) and
/// runs the ceremony with the production prompt reader and store. The
/// D2/D3 guards live inside the ceremony, where the confirm seam
/// exists. On success the §6 daemon heads-up probes the per-database
/// socket — best-effort, never exit-code-relevant.
async fn connect_command(
    io: &mut CommandIo<'_>,
    config_path: Option<PathBuf>,
    args: connect::ConnectArgs,
) -> u8 {
    let path = AppConfig::write_path(config_path);
    match AppConfig::extract_raw(Some(&path)) {
        Ok(raw) => {
            let db_path = raw.db_path().to_path_buf();
            let store = production_store(raw.nextcloud.credential_store, raw.credential_base_dir());
            let mut prompts = connect::StdinPrompts;
            match connect::run(&path, raw, args, &mut prompts, &store).await {
                Ok(mut receipt) => {
                    // The ADR 0014 decision-7 nudge: best-effort,
                    // success-path only, never exit-code-relevant. The
                    // daemon is asked to attach from persisted state —
                    // identity on the wire, never the secret. The alias
                    // (`login`) inherits the nudge by delegation.
                    let nudge = connect::nudge_daemon_attach(
                        &db_path,
                        &receipt.server_url,
                        &receipt.username,
                    )
                    .await;
                    let state = connect::map_attach_nudge(&nudge);
                    if let Some(warning) = connect::attach_nudge_warning(state) {
                        let _ = writeln!(io.err, "{warning}");
                    }
                    receipt.daemon_connector = Some(state);
                    emit_to(io, &receipt, &connect::render_text(&receipt));
                    0
                }
                Err(err) => report_to(io, &err),
            }
        }
        // A not-yet-valid config must not block connect (first-run
        // bootstrap command); undecodable ones still fail.
        Err(err) => report_to(io, &CommandError::from(err)),
    }
}

/// One-shot `logout`: reads the config without validation (the
/// teardown command must survive a not-yet-valid file), deletes the
/// stored secret, clears the auto keys — then, best-effort (WI-H),
/// nudges the daemon serving the same database to drop its connector.
/// The nudge never changes the exit code (D5): the secret *is* deleted;
/// degradations ride stderr.
///
/// `pub` to mirror the peer one-shot dispatches (`repeat_command`,
/// `breaks_command`, …) — the test tier drives the dispatcher tier
/// directly (#1175). The session lane intercepts `Logout` as a
/// ceremony; `dispatch_line`'s own arm stays unreachable through it.
pub async fn logout_command(io: &mut CommandIo<'_>, config_path: Option<PathBuf>) -> u8 {
    let path = AppConfig::config_path(config_path)
        .unwrap_or_else(|| PathBuf::from(crate::config::DEFAULT_CONFIG_PATH));
    match AppConfig::extract_raw(Some(&path)) {
        Ok(raw) => {
            let store = production_store(raw.nextcloud.credential_store, raw.credential_base_dir());
            match logout::run(&path, &raw, store.as_ref()) {
                Ok(mut receipt) => {
                    let nudge =
                        logout::nudge_daemon(raw.db_path(), &receipt.server_url, &receipt.username)
                            .await;
                    let state = logout::map_nudge(&nudge);
                    if let Some(warning) = logout::nudge_warning(state) {
                        let _ = writeln!(io.err, "{warning}");
                    }
                    receipt.daemon_connector = Some(state);
                    emit_to(io, &receipt, &logout::render_text(&receipt));
                    0
                }
                Err(err) => report_to(io, &err),
            }
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
    connector: Option<&str>,
) -> u8 {
    match core_ctx(config_path) {
        Ok((config, store)) => {
            match sync::run(
                config,
                store,
                no_wait,
                Duration::from_secs(timeout_secs),
                connector,
            )
            .await
            {
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
/// distinction), the wrapper flag types (`Option<Option<_>>`) map by
/// identity, `--archive`/`--unarchive` are mutually exclusive (clap).
///
/// `DueSpec::Date` mints here — the execution seam, where
/// `resolved_day_calendar()` is reachable (the `habits`/`reflection`
/// precedent): the bare date's instant is `day.day_last_instant(date)`,
/// the N-4 stamping law's CLI face (task-detail-editor UX plan
/// US-1/D-5; #893 supersedes the mint) — the identical instant the GUI
/// funnel's `parse_stamp` produces, and one that survives the
/// `[now, end)` today membership the raw window end does not.
fn task_changes(
    flags: &crate::cli::TaskEditFlags,
    day: &taskboard_domain::days::DayCalendar,
) -> taskboard_domain::TaskChanges {
    let crate::cli::TaskEditFlags {
        title,
        description,
        due,
        scheduled,
        effort,
        importance,
        urgency,
        kind,
        archive,
        unarchive,
    } = flags;
    taskboard_domain::TaskChanges {
        title: title.clone(),
        description: description.clone(),
        duedate: due.clone().map(|spec| match spec {
            crate::cli::DueSpec::Set(dt) => Some(dt),
            crate::cli::DueSpec::Date(date) => Some(day.day_last_instant(date)),
            crate::cli::DueSpec::Clear => None,
        }),
        scheduled: scheduled.clone().map(|spec| match spec {
            crate::cli::DueSpec::Set(dt) => Some(dt),
            crate::cli::DueSpec::Date(date) => Some(day.day_last_instant(date)),
            crate::cli::DueSpec::Clear => None,
        }),
        // The spec enums share `DueSpec`'s shape: `Clear` becomes
        // `Some(None)` (clear-and-stamp), `Set` becomes `Some(Some(_))`
        // (set-and-stamp), an absent flag stays `None` (untouched).
        effort: effort.clone().map(|spec| match spec {
            crate::cli::EffortSpec::Set(e) => Some(e),
            crate::cli::EffortSpec::Clear => None,
        }),
        importance: importance.clone().map(|spec| match spec {
            crate::cli::ImportanceSpec::Set(level) => Some(level),
            crate::cli::ImportanceSpec::Clear => None,
        }),
        urgency: urgency.clone().map(|spec| match spec {
            crate::cli::UrgencySpec::Set(level) => Some(level),
            crate::cli::UrgencySpec::Clear => None,
        }),
        recur: None,
        kind: kind.clone().map(|spec| match spec {
            crate::cli::TaskKindSpec::Set(value) => Some(value),
            crate::cli::TaskKindSpec::Clear => None,
        }),
        archived: if *archive {
            Some(true)
        } else {
            (*unarchive).then_some(false)
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
        Command::Connect { .. }
        | Command::Login { .. }
        | Command::Logout
        | Command::Config { .. }
        | Command::Daemon { .. }
        | Command::Gui => {
            // The session loop runs connect/login/logout as ceremonies
            // and refuses daemon/config/gui; the IPC server refuses all
            // six before dispatch (ADR 0014). This arm only documents
            // the routing invariant.
            unreachable!(
                "the session loop intercepts connect/login/logout (ceremonies) and daemon/config/gui (refusals); the IPC server refuses all six before dispatch"
            )
        }
        Command::Status => {
            // The session *is* the core holder; the honest client-side
            // answer names it — and under `nextcloud`, the live
            // attach/detach fact read off the App (ADR 0014). It
            // derives the arrays from its own core (D6-7). Never
            // boots, never syncs.
            let view = status::StatusView::in_session(app);
            status::emit(io, &view);
            0
        }
        Command::Boards { cmd } => boards_line(session, app, cmd, io).await,
        Command::Stacks { cmd } => stacks_line(app, cmd, io).await,
        Command::Labels { cmd } => labels_line(app, cmd, io).await,
        Command::Find { query, kind, limit } => {
            // A pure read off the session's published state: exit 0 on
            // every well-formed query, the empty table on no hits.
            let rows = find::find_on(app, &query, kind, limit);
            emit_to(io, &rows, &find::render_text_list(&rows));
            0
        }
        Command::Timers { cmd } => match timers::collapse(cmd) {
            Some(invocation) => timers::timers_line(app, io, invocation).await,
            None => unreachable!("clap requires exactly one of --in/--at on timers add"),
        },
        Command::Breaks { cmd } => match breaks::collapse(cmd) {
            Some(invocation) => breaks::breaks_line(app, io, invocation).await,
            None => unreachable!("clap cannot produce an unresolvable breaks add"),
        },
        Command::Track { cmd } => match track::collapse(cmd) {
            Some(invocation) => {
                track::track_line(
                    app,
                    session.session_config().resolved_day_calendar(),
                    io,
                    invocation,
                )
                .await
            }
            None => unreachable!("clap requires exactly one log mode (--for | --from --to)"),
        },
        Command::Habits { cmd } => habits::habits_line(app, io, habits::collapse(cmd)).await,
        Command::Reflection { cmd } => match reflection::collapse(cmd) {
            Some(invocation) => {
                reflection::reflection_line(
                    app,
                    session.session_config().resolved_day_calendar(),
                    io,
                    invocation,
                )
                .await
            }
            None => unreachable!("the journal grammar has no vacuous branch"),
        },
        Command::Export {
            what,
            format,
            since,
            until,
            task,
            board,
            #[cfg(feature = "pdf-export")]
            layout,
            #[cfg(feature = "pdf-export")]
            out,
            #[cfg(feature = "pdf-export")]
            no_open,
        } => {
            let query = track::ExportQuery {
                what: what.unwrap_or_default(),
                format,
                since,
                until,
                task,
                board,
                #[cfg(feature = "pdf-export")]
                layout,
                #[cfg(feature = "pdf-export")]
                out,
                #[cfg(feature = "pdf-export")]
                no_open,
            };
            #[cfg(feature = "pdf-export")]
            if query.what == crate::cli::ExportWhat::Pdf {
                return track::export_pdf_line(app, io, &query);
            }
            track::export_line(app, io, &query)
        }
        Command::Capture { title, board } => emit_result(
            io,
            capture::capture_on(app, title, board.as_deref()).await,
            tasks::render_text_receipt,
        ),
        Command::Tasks { cmd } => tasks_line(session, app, cmd, io).await,
        Command::Sync {
            no_wait,
            timeout_secs,
            connector,
        } => {
            sync_line(
                session,
                app,
                io,
                no_wait,
                timeout_secs,
                connector.as_deref(),
            )
            .await
        }
    }
}

/// The boards family over a booted session (ADR 0011): `list` reads the
/// session config/store; `select`/`publish` run their connector
/// preludes against the session before touching the core; `create`/
/// `rename` hit the core directly.
#[allow(clippy::too_many_lines)] // one arm per subcommand, kept flat for review
async fn boards_line(
    session: &(impl CoreSession + ?Sized),
    app: &crate::App,
    cmd: BoardsCmd,
    io: &mut CommandIo<'_>,
) -> u8 {
    match cmd {
        BoardsCmd::List { connector } => emit_result(
            io,
            boards::list(
                session.session_config(),
                session.session_store(),
                connector.as_deref(),
            )
            .await,
            |entries: &Vec<_>| boards::render_text_list(entries),
        ),
        BoardsCmd::Select {
            board,
            local_board,
            timeout_secs,
            bind_local_board,
            connector,
        } => {
            match boards::select_prelude(
                session.session_config(),
                session.session_store(),
                board,
                connector.as_deref(),
            )
            .await
            {
                Ok((entry, spec)) => emit_result(
                    io,
                    boards::select_on(
                        app,
                        entry,
                        Duration::from_secs(timeout_secs),
                        bind_local_board,
                        local_board,
                        &spec.name,
                    )
                    .await,
                    boards::render_text_select,
                ),
                Err(err) => report_to(io, &err),
            }
        }
        BoardsCmd::Use { .. } => unreachable!(
            "the REPL loop intercepts boards use (session mutation); the IPC server refuses it before dispatch"
        ),
        BoardsCmd::Create { title, color } => {
            let color = boards::resolved_color(color.as_deref());
            let color_str = color.as_str().to_owned();
            emit_result(
                io,
                boards::create_on(app, title, color, color_str).await,
                boards::render_text_create,
            )
        }
        BoardsCmd::Rename { board, title } => emit_result(
            io,
            boards::rename_on(app, board, title).await,
            boards::render_text_rename,
        ),
        BoardsCmd::Publish {
            board,
            to_files,
            timeout_secs,
            connector,
        } => {
            if to_files {
                // The direct local-to-Files bind (#764): the session's
                // config carries the lane guard, the session core the
                // ceremony — one ceremony, two routings (D-5).
                emit_result(
                    io,
                    boards::publish_to_files_on(
                        &app.engine,
                        session.session_config(),
                        board,
                        Duration::from_secs(timeout_secs),
                    )
                    .await,
                    boards::render_text_files_publish,
                )
            } else {
                match boards::publish_prelude(
                    session.session_config(),
                    session.session_store(),
                    connector.as_deref(),
                ) {
                    Ok((client, spec)) => emit_result(
                        io,
                        boards::publish_on(
                            &app.engine,
                            &client,
                            board,
                            Duration::from_secs(timeout_secs),
                            &spec.name,
                        )
                        .await,
                        boards::render_text_select,
                    ),
                    Err(err) => report_to(io, &err),
                }
            }
        }
        BoardsCmd::Color { board, color } => emit_result(
            io,
            boards::color_on(app, board, taskboard_domain::Color::new(&color)).await,
            boards::render_text_board_edit,
        ),
        BoardsCmd::Archive { board } => emit_result(
            io,
            boards::archive_on(app, board, true).await,
            boards::render_text_board_edit,
        ),
        BoardsCmd::Unarchive { board } => emit_result(
            io,
            boards::archive_on(app, board, false).await,
            boards::render_text_board_edit,
        ),
        BoardsCmd::Delete { board, confirm } => emit_result(
            io,
            boards::delete_on(app, board, confirm).await,
            boards::render_text_destruct,
        ),
        BoardsCmd::Reset { board, confirm } => emit_result(
            io,
            boards::reset_on(app, board, confirm).await,
            boards::render_text_destruct,
        ),
        BoardsCmd::Move {
            board,
            to_files: _,
            undo,
            timeout_secs,
            ref to_connection,
        } => {
            if let Some(target) = to_connection {
                emit_result(
                    io,
                    boards::rebind_deck_to_deck(app, session.session_config(), board, target).await,
                    boards::render_text_rebind,
                )
            } else {
                // Explicit mapping (D-3 of the cli-verb-grammar-hygiene
                // plan): clap's required group admits exactly one flag.
                let direction = if undo {
                    crate::board_move_ceremony::MoveDirection::Undo
                } else {
                    crate::board_move_ceremony::MoveDirection::ToFiles
                };
                // The default-path identity home: the typed
                // `FilesSyncDisabled` mapping rides the helper's error
                // (the injection seam, #751), rendered like any verb
                // refusal.
                let outcome = match crate::board_move_ceremony::default_device_json() {
                    Ok(device_json) => {
                        crate::board_move_ceremony::run_on(
                            &app.engine,
                            session.session_config(),
                            session.session_store(),
                            board,
                            direction,
                            &device_json,
                            Duration::from_secs(timeout_secs),
                        )
                        .await
                    }
                    Err(err) => Err(err),
                };
                emit_result(io, outcome, crate::board_move_ceremony::render_text)
            }
        }
        BoardsCmd::Unbind { board, confirm } => emit_result(
            io,
            boards::unbind_on(app, board, confirm).await,
            boards::render_text_unbind,
        ),
        BoardsCmd::Reconcile { board, confirm } => emit_result(
            io,
            boards::reconcile_on(app, board, confirm).await,
            boards::render_text_reconcile,
        ),
    }
}

/// The stacks family over a booted session: pure core mutations, no
/// prelude.
async fn stacks_line(app: &crate::App, cmd: StacksCmd, io: &mut CommandIo<'_>) -> u8 {
    match cmd {
        StacksCmd::Add {
            board,
            title,
            order,
        } => emit_result(
            io,
            stacks::add_on(app, board, title, order).await,
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
        LabelsCmd::List { board } => {
            emit_result(io, labels::list_on(app, board), |rows: &Vec<_>| {
                labels::render_text_list(rows)
            })
        }
        LabelsCmd::Add {
            board,
            title,
            color,
        } => {
            // The clap value parser normalizes to lowercase; the
            // session arm normalizes like the one-shot wrapper does.
            let color = color.map_or_else(
                || labels::DEFAULT_LABEL_COLOR.to_owned(),
                |c| c.to_lowercase(),
            );
            emit_result(
                io,
                labels::add_on(app, board, title, color).await,
                labels::render_text_receipt,
            )
        }
        LabelsCmd::Edit {
            label,
            flags: LabelEditFlags { title, color },
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
        LabelsCmd::Restore { label } => emit_result(
            io,
            labels::restore_on(app, &label).await,
            labels::render_text_restore,
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
            let standalone = crate::connectors::configured_connectors(session.session_config())
                .expect("connector names validated at session boot — the session config is a validated AppConfig")
                .is_empty();
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
        TasksCmd::Move {
            task,
            stack,
            board,
            order,
        } => emit_result(
            io,
            tasks::move_on(app, &task, stack.as_deref(), board.as_deref(), order).await,
            tasks::render_text_receipt,
        ),
        TasksCmd::Edit { task, flags } => {
            // The mint rides the execution seam: the session's day
            // calendar (US-1/D-5).
            let changes = task_changes(&flags, &app.config.resolved_day_calendar());
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
        remind @ TasksCmd::Remind { .. } => match reminders::collapse(remind) {
            Some(invocation) => reminders::remind_line(app, io, invocation).await,
            None => unreachable!("the outer match selected the Remind arm"),
        },
        repeat @ TasksCmd::Repeat { .. } => match repeat::collapse(repeat) {
            Some(invocation) => repeat::repeat_line(app, io, invocation).await,
            None => unreachable!("the outer match selected the Repeat arm"),
        },
        TasksCmd::Undo => emit_result(io, tasks::undo_on(app).await, tasks::render_text_undo),
        TasksCmd::Redo => emit_result(io, tasks::redo_on(app).await, tasks::render_text_undo),
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
    connector: Option<&str>,
) -> u8 {
    // A live connector outranks the boot-frozen backend word (the
    // status projection's same honesty rule): a standalone-booted
    // daemon that attached from persisted state serves cycles. The
    // refusal classes are otherwise untouched — `standalone` with no
    // connector still refuses `sync-disabled`, and a gated pass with a
    // detached connector still answers `sync-unavailable` from the verb.
    let gated = crate::commands::require_connector(session.session_config());
    if let Err(err) = gated
        && !app.connector_attached()
    {
        return report_to(io, &err);
    }
    let scope = match connector {
        Some(name) => {
            match crate::commands::resolve_connector(session.session_config(), Some(name)) {
                Ok(spec) => Some(spec.name),
                Err(err) => return report_to(io, &err),
            }
        }
        None => None,
    };
    match sync::sync_on(
        app,
        no_wait,
        Duration::from_secs(timeout_secs),
        scope.as_ref(),
        io.out,
    )
    .await
    {
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
    /// Only meaningful against a running daemon. The lane carries the
    /// verb: main's `IpcOnly` arm dispatches on the payload, so a new
    /// `IpcOnly` verb must add a variant here *and* be handled in
    /// main's verb match, or the binary does not compile — the dual
    /// enumeration (a lane table plus a second, `unreachable!`-guarded
    /// match) cannot drift.
    IpcOnly(IpcOnlyVerb),
}

/// The verbs routed `IpcOnly` — the operator commands that are only
/// meaningful against a running daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcOnlyVerb {
    /// `status` — the daemon's health/status report.
    Status,
    /// `daemon stop` — the graceful daemon shutdown request.
    DaemonStop,
    /// `config reload` (#1015, journey US-2/S1, D-RELOAD-ROUTE) — the
    /// manual reload trigger; only meaningful against a running
    /// daemon's engine (a stopped session's file is already its boot
    /// authority — no local fallback fiction).
    ConfigReload,
}

/// The per-`Command` routing table (pure; unit-pinned).
#[must_use]
pub fn route_for_ipc(command: &Command) -> IpcRoute {
    match command {
        Command::Connect { .. }
        | Command::Login { .. }
        | Command::Logout
        // `config print`/`config path` stay `Local` (journey R-2):
        // file-authoritative diagnostics; write-through guarantees the
        // file is written *before* the engine apply, so a local print
        // never lags a successful set.
        | Command::Config {
            cmd: crate::cli::ConfigCmd::Print | crate::cli::ConfigCmd::Path,
        }
        // The gui verb is a lifecycle arm main intercepts before
        // routing ever sees it; pinning it `Local` keeps the
        // never-relayed rule compiler- and test-enforced — any
        // refactor that routes it onto the wire changes this pinned
        // lane.
        | Command::Gui
        // `daemon` with no subcommand is the lifecycle run arm main
        // intercepts before routing (`daemon_run`); pinning it `Local`
        // keeps it off the wire — the same convention as `Gui`. A
        // future `DaemonCmd` variant lands in neither `Daemon` arm and
        // is a compile error: it must pick a lane deliberately.
        | Command::Daemon { cmd: None, .. }
        | Command::Boards {
            cmd: BoardsCmd::List { .. },
        } => IpcRoute::Local,
        Command::Status => IpcRoute::IpcOnly(IpcOnlyVerb::Status),
        Command::Daemon {
            cmd: Some(DaemonCmd::Stop),
            ..
        } => IpcRoute::IpcOnly(IpcOnlyVerb::DaemonStop),
        // `config reload` is daemon-only (journey US-2/S1,
        // D-RELOAD-ROUTE): the reload only means something against a
        // running engine; without one the standing typed `NoDaemon`
        // refusal fires.
        Command::Config {
            cmd: crate::cli::ConfigCmd::Reload,
        } => IpcRoute::IpcOnly(IpcOnlyVerb::ConfigReload),
        Command::Boards { .. }
        | Command::Stacks { .. }
        | Command::Labels { .. }
        | Command::Tasks { .. }
        | Command::Timers { .. }
        // The break-rule family is the timers family's mutation lane
        // verbatim: the daemon's engine owns the rows; a one-shot local
        // boot would orphan them at exit (D-17).
        | Command::Breaks { .. }
        // Every tracking mutation must land in the process that owns
        // the engine: a one-shot local boot would open an entry and
        // orphan the intent at exit — the timers precedent routes
        // identically. Export reads the same engine state; splitting
        // the read off onto a local boot would report a *different*
        // database's history when a daemon serves the db (D-TC1).
        | Command::Track { .. }
        // The habit family joins the mutation lane: a one-shot local
        // boot would write a *different* database's data when a daemon
        // serves the db (the D-TC1 argument verbatim).
        | Command::Habits { .. }
        // The journal family rides the same lane: its mutating verbs
        // must land in the daemon's engine (the reflection rows live
        // in the serving database), and `journal list` answers that
        // database's truth too (the D-TC1 argument verbatim).
        | Command::Reflection { .. }
        | Command::Export { .. }
        | Command::Find { .. }
        // Capture rides the mutation lane (boot-local over the core,
        // like the task verbs).
        | Command::Capture { .. }
        // The config mutation verbs ride the relay lane (journey W-1,
        // #845): the daemon writes the file through the prove gate and
        // applies live via `UpdateSettings`; no daemon serving = the
        // typed next-start local lane (IpcOrLocal *is* the US-2
        // fallback). No new wire variant — `Request::Exec` relays the
        // argv and the daemon re-parses it (D2).
        | Command::Config {
            cmd: crate::cli::ConfigCmd::Set { .. } | crate::cli::ConfigCmd::Unset { .. },
        }
        // (`config reload` above: `IpcOnly` — daemon-only, D-RELOAD-ROUTE.)
        | Command::Sync { .. } => IpcRoute::IpcOrLocal,
    }
}

/// The verb's own daemon-side wait budget in seconds, when it has one
/// (`--timeout-secs`): the input the client read deadline is derived
/// from (`client_read_budget` — the deadline is the verb's budget plus
/// a fixed margin, so it can never preempt an honest long verb). The
/// match is exhaustive: a future `--timeout-secs` flag must land here,
/// or its verb silently derives the flag-less default ceiling.
#[must_use]
pub fn verb_timeout_secs(command: &Command) -> Option<u64> {
    match command {
        Command::Sync { timeout_secs, .. }
        | Command::Boards {
            cmd: BoardsCmd::Select { timeout_secs, .. } | BoardsCmd::Publish { timeout_secs, .. },
        } => Some(*timeout_secs),
        _ => None,
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
    let store = production_store(
        config.nextcloud.credential_store,
        config.credential_base_dir(),
    );
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

/// The shared live-board selector (chunk 10 D10, the ADR 0009
/// cascade's shape): exact uuid or exact title among the *live*
/// boards — one home shared by `capture --board` and
/// `tasks move --board`.
pub(crate) fn resolve_live_board(
    state: &taskboard_domain::AppState,
    selector: &str,
) -> Result<taskboard_domain::BoardId, CommandError> {
    let live: Vec<&taskboard_domain::Board> = state
        .boards
        .values()
        .filter(|b| !b.deleted && !b.archived)
        .collect();
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = taskboard_domain::BoardId::from_uuid(uuid);
        if live.iter().any(|b| b.id == id) {
            return Ok(id);
        }
    }
    let matches: Vec<taskboard_domain::BoardId> = live
        .iter()
        .filter(|b| b.title == selector)
        .map(|b| b.id)
        .collect();
    match matches.len() {
        0 => Err(CommandError::UnknownBoardSelector {
            selector: selector.to_owned(),
        }),
        1 => Ok(matches[0]),
        // Duplicate titles: the typed ambiguity names the live boards'
        // uuids (the AmbiguousBoard convention).
        _ => Err(CommandError::AmbiguousBoard {
            candidates: live.iter().map(|b| b.id.as_uuid().to_string()).collect(),
        }),
    }
}

#[cfg(test)]
mod ipc_routing_tests {
    use crate::cli::{BoardsCmd, DaemonCmd, TasksCmd};
    use crate::commands::{Command, IpcOnlyVerb, IpcRoute, route_for_ipc};

    /// The transport lane table (ADR 0012 §2.1): pre-core verbs stay
    /// client-side; core verbs prefer a daemon; the operator verbs only
    /// talk to one. Exhaustive — the compiler forces future verbs here.
    #[test]
    #[allow(clippy::too_many_lines)] // one row per command family; the list IS the table
    fn the_routing_table_picks_lanes_per_command() {
        let local = [
            Command::Connect {
                server: None,
                user: None,
                switch_backend: false,
                force: false,
            },
            Command::Login {
                server: None,
                user: None,
            },
            Command::Logout,
            Command::Config {
                cmd: crate::cli::ConfigCmd::Print,
            },
            // `config path` stays a file-authoritative diagnostic
            // (journey R-2).
            Command::Config {
                cmd: crate::cli::ConfigCmd::Path,
            },
            Command::Boards {
                cmd: BoardsCmd::List { connector: None },
            },
            // `daemon` with no subcommand is the lifecycle run arm main
            // intercepts before routing; its lane is pinned `Local` —
            // never relayed onto the wire (the gui precedent).
            Command::Daemon {
                cmd: None,
                no_ipc: false,
            },
        ];
        for command in &local {
            assert_eq!(route_for_ipc(command), IpcRoute::Local, "{command:?}");
        }

        // The gui verb is a lifecycle arm main intercepts before
        // routing: its lane is pinned `Local` — it never takes an IPC
        // lane, i.e. it is never relayed onto the wire.
        assert_eq!(route_for_ipc(&Command::Gui), IpcRoute::Local);

        let ipc_only = [
            (Command::Status, IpcOnlyVerb::Status),
            (
                Command::Daemon {
                    cmd: Some(DaemonCmd::Stop),
                    no_ipc: false,
                },
                IpcOnlyVerb::DaemonStop,
            ),
        ];
        for (command, verb) in &ipc_only {
            assert_eq!(
                route_for_ipc(command),
                IpcRoute::IpcOnly(*verb),
                "{command:?}"
            );
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
                    local_board: None,
                    timeout_secs: 1,
                    bind_local_board: false,
                    connector: None,
                },
            },
            Command::Stacks {
                cmd: crate::cli::StacksCmd::Add {
                    board: None,
                    title: "t".to_owned(),
                    order: None,
                },
            },
            // The config mutation verbs ride the relay lane (journey
            // W-1, #845): daemon if serving (live apply), else the
            // local next-start lane.
            Command::Config {
                cmd: crate::cli::ConfigCmd::Set {
                    pairs: vec!["focus.work_mins".to_owned(), "50".to_owned()],
                },
            },
            Command::Config {
                cmd: crate::cli::ConfigCmd::Unset {
                    keys: vec!["focus.work_mins".to_owned()],
                },
            },
            Command::Labels {
                cmd: crate::cli::LabelsCmd::List { board: None },
            },
            Command::Tasks {
                cmd: TasksCmd::Add {
                    title: "t".to_owned(),
                    stack: None,
                },
            },
            // The undo verbs inherit the `Command::Tasks` wildcard's
            // `IpcOrLocal` lane (ADR 0012, structural); the pins make
            // the inheritance guarded — a future split of the wildcard
            // arm cannot silently drop them off the mutation lane.
            Command::Tasks {
                cmd: TasksCmd::Undo,
            },
            Command::Tasks {
                cmd: TasksCmd::Redo,
            },
            Command::Tasks {
                cmd: TasksCmd::Repeat(crate::cli::RepeatArgs {
                    task: "t".to_owned(),
                    mode: crate::cli::RepeatModeFlags {
                        daily: None,
                        weekly: None,
                        monthly: None,
                        off: true,
                        pause: None,
                        resume: false,
                    },
                    every_weeks: None,
                    until: None,
                    count: None,
                }),
            },
            Command::Find {
                query: "needle".to_owned(),
                kind: None,
                limit: None,
            },
            Command::Timers {
                cmd: crate::cli::TimersCmd::List {},
            },
            Command::Breaks {
                cmd: crate::cli::BreaksCmd::List,
            },
            Command::Track {
                cmd: crate::cli::TrackCmd::Start {
                    task: "t".to_owned(),
                    kind: None,
                },
            },
            Command::Habits {
                cmd: crate::cli::HabitsCmd::Log {
                    habit: "h".to_owned(),
                },
            },
            Command::Export {
                what: None,
                format: None,
                since: None,
                until: None,
                task: None,
                board: None,
                #[cfg(feature = "pdf-export")]
                layout: None,
                #[cfg(feature = "pdf-export")]
                out: None,
                #[cfg(feature = "pdf-export")]
                no_open: false,
            },
            Command::Sync {
                no_wait: false,
                timeout_secs: 1,
                connector: None,
            },
        ];
        for command in &ipc_or_local {
            assert_eq!(route_for_ipc(command), IpcRoute::IpcOrLocal, "{command:?}");
        }
    }

    /// `us1_s1`: `config set` takes the relay lane — the daemon (when
    /// one serves) is where the live apply happens (US-1/S1–S2).
    #[test]
    fn us1_s1_config_set_takes_the_relay_lane() {
        assert_eq!(
            route_for_ipc(&Command::Config {
                cmd: crate::cli::ConfigCmd::Set {
                    pairs: vec!["focus.work_mins".to_owned(), "50".to_owned()],
                },
            }),
            IpcRoute::IpcOrLocal
        );
    }

    /// `us3_s1`: `config unset` takes the same relay lane as set —
    /// the lane choice the journey's S1 step dispatches on.
    #[test]
    fn us3_s1_config_unset_takes_the_relay_lane() {
        assert_eq!(
            route_for_ipc(&Command::Config {
                cmd: crate::cli::ConfigCmd::Unset {
                    keys: vec!["focus.work_mins".to_owned()],
                },
            }),
            IpcRoute::IpcOrLocal
        );
    }

    /// `us2_s1`: the relay lane IS the offline fallback — `config
    /// set` rides the same `IpcOrLocal` lane as the mutation family
    /// (daemon if serving, else the typed next-start local lane,
    /// US-2 AC-2's mechanism).
    #[test]
    fn us2_s1_config_set_rides_the_fallback_capable_lane() {
        assert_eq!(
            route_for_ipc(&Command::Config {
                cmd: crate::cli::ConfigCmd::Set {
                    pairs: vec!["focus.work_mins".to_owned(), "50".to_owned()],
                },
            }),
            route_for_ipc(&Command::Sync {
                no_wait: false,
                timeout_secs: 1,
                connector: None,
            }),
            "the same fallback-capable lane as the mutation family"
        );
    }
}

#[cfg(test)]
mod connector_resolution_tests {
    use crate::commands::{CommandError, resolve_connector};
    use crate::config::{AppConfig, ConnectorCfg, SyncBackend};

    fn two_connector_config() -> AppConfig {
        let mut config = AppConfig::default();
        config.sync.connectors = Some(vec![
            ConnectorCfg {
                name: "alpha".into(),
                backend: SyncBackend::Nextcloud,
                server_url: "https://a.example".into(),
                username: "ua".into(),
            },
            ConnectorCfg {
                name: "beta".into(),
                backend: SyncBackend::Nextcloud,
                server_url: "https://b.example".into(),
                username: "ub".into(),
            },
        ]);
        config
    }

    /// `None` resolves the default (first) entry; a named hit resolves
    /// that entry; a miss refuses typed, carrying the name (D6-5).
    #[test]
    fn resolve_connector_picks_default_named_and_refuses_misses() {
        let config = two_connector_config();
        let default = resolve_connector(&config, None).expect("default");
        assert_eq!(default.name.as_str(), "alpha");
        assert_eq!(default.server_url, "https://a.example");
        let named = resolve_connector(&config, Some("beta")).expect("beta");
        assert_eq!(named.name.as_str(), "beta");
        assert_eq!(named.username, "ub");
        assert!(matches!(
            resolve_connector(&config, Some("gama")),
            Err(CommandError::UnknownConnector { ref name }) if name == "gama"
        ));
        assert_eq!(
            resolve_connector(&config, Some("gama"))
                .unwrap_err()
                .error_kind(),
            "unknown-connector"
        );
    }
}

#[cfg(test)]
mod task_field_mapping_tests {
    use crate::cli::{DueSpec, EffortSpec, ImportanceSpec, TaskEditFlags, UrgencySpec};
    use crate::commands::task_changes;
    use taskboard_domain::{Effort, Importance, Urgency};

    #[test]
    fn the_field_flags_map_onto_their_changeset_slots() {
        let due = chrono::Utc::now();
        let day = taskboard_domain::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).expect("zero"),
        };
        let changes = task_changes(
            &TaskEditFlags {
                due: Some(DueSpec::Set(due)),
                scheduled: Some(DueSpec::Clear),
                effort: Some(EffortSpec::Set(Effort::new(5400))),
                importance: Some(ImportanceSpec::Clear),
                urgency: Some(UrgencySpec::Set(Urgency::High)),
                ..TaskEditFlags::default()
            },
            &day,
        );
        assert_eq!(changes.duedate, Some(Some(due)));
        assert_eq!(changes.scheduled, Some(None), "--scheduled none clears");
        assert_eq!(changes.effort, Some(Some(Effort::new(5400))));
        assert_eq!(
            changes.importance,
            Some(None),
            "--importance none clears while the rest set"
        );
        assert_eq!(changes.urgency, Some(Some(Urgency::High)));
        assert_eq!(changes.archived, None);
    }

    /// US-1/S3 (task-detail-editor UX plan D-5, W-2): the CLI's bare
    /// date mints the same instant the GUI funnel's `parse_stamp`
    /// produces — both fronts compose the one domain law
    /// (`days::day_last_instant` over the same day facts, #893's
    /// unified mint), so the parity triangle closes: GUI asserted ==
    /// law (`due_edit` tests), CLI asserted == law (here) ⇒ GUI ==
    /// CLI.
    #[test]
    fn us1_s3_cli_bare_date_matches_the_gui_instant() {
        let day = taskboard_domain::days::DayCalendar {
            day_start_time: chrono::NaiveTime::from_hms_opt(3, 0, 0).expect("3:00"),
            offset: chrono::FixedOffset::east_opt(2 * 3600).expect("valid offset"),
        };
        let date = chrono::NaiveDate::from_ymd_opt(2030, 2, 28).expect("valid date");
        let changes = task_changes(
            &TaskEditFlags {
                due: Some(DueSpec::Date(date)),
                scheduled: Some(DueSpec::Date(date)),
                ..TaskEditFlags::default()
            },
            &day,
        );
        let mint = day.day_last_instant(date);
        assert_eq!(changes.duedate, Some(Some(mint)));
        assert_eq!(changes.scheduled, Some(Some(mint)));
    }

    #[test]
    fn absent_flags_are_untouched_slots() {
        let day = taskboard_domain::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).expect("zero"),
        };
        let changes = task_changes(&TaskEditFlags::default(), &day);
        assert_eq!(changes, taskboard_domain::TaskChanges::default());
    }

    #[test]
    fn importance_maps_the_distinct_type() {
        let day = taskboard_domain::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).expect("zero"),
        };
        let changes = task_changes(
            &TaskEditFlags {
                importance: Some(ImportanceSpec::Set(Importance::VeryLow)),
                archive: true,
                ..TaskEditFlags::default()
            },
            &day,
        );
        assert_eq!(changes.importance, Some(Some(Importance::VeryLow)));
        assert_eq!(changes.urgency, None, "urgency is a distinct slot");
        assert_eq!(changes.archived, Some(true));
    }

    /// `us2_s1_reload_routes_ipc_only` (US-2/S1, W-5, D-RELOAD-ROUTE):
    /// `config reload` joins the `IpcOnly` verbs — only meaningful
    /// against a running daemon; the standing typed `NoDaemon` refusal
    /// rides the existing lane without one.
    #[test]
    fn us2_s1_reload_routes_ipc_only() {
        use crate::cli::Command;
        use crate::commands::{IpcOnlyVerb, IpcRoute, route_for_ipc};
        assert_eq!(
            route_for_ipc(&Command::Config {
                cmd: crate::cli::ConfigCmd::Reload,
            }),
            IpcRoute::IpcOnly(IpcOnlyVerb::ConfigReload)
        );
    }
}

#[cfg(test)]
mod error_detail_tests {
    use super::{CommandError, not_undoable_reason_kind};
    use taskboard_domain::NotUndoableReason;
    use taskboard_state::ExecuteError;

    fn engine_rejected(rejected: taskboard_domain::CommandError) -> CommandError {
        CommandError::Engine(Box::new(ExecuteError::Rejected(rejected)))
    }

    /// The `--json` error detail is a structural surface (ADR 0019): a
    /// variant that carries detail must never degrade to `None` — the
    /// M1 sweep found each arm deletable with the full suite green.
    #[test]
    fn the_detail_carrying_variants_keep_their_detail() {
        assert!(CommandError::UnknownBoard(7).error_detail().is_some());
        assert!(CommandError::Cancelled.error_detail().is_some());
        assert!(
            engine_rejected(taskboard_domain::CommandError::TargetNotUndoable(
                NotUndoableReason::BoardCeremony,
            ))
            .error_detail()
            .is_some()
        );
        assert!(
            engine_rejected(taskboard_domain::CommandError::UndoTargetHasDependents {
                stack: taskboard_domain::StackId::from_uuid(uuid::Uuid::new_v4()),
            },)
            .error_detail()
            .is_some()
        );
    }

    /// The kebab discriminators are the `--json` detail's field names —
    /// structural facts a script branches on, never prose (ADR 0019).
    /// The M1 sweep found the mapping wholesale replaceable.
    #[test]
    fn the_not_undoable_reason_kinds_are_the_adr_0019_discriminators() {
        use NotUndoableReason as R;
        assert_eq!(not_undoable_reason_kind(R::RestoreClass), "restore-class");
        assert_eq!(not_undoable_reason_kind(R::BoardCeremony), "board-ceremony");
        assert_eq!(
            not_undoable_reason_kind(R::ReminderSurface),
            "reminder-surface"
        );
        assert_eq!(
            not_undoable_reason_kind(R::TrackingSurface),
            "tracking-surface"
        );
        assert_eq!(not_undoable_reason_kind(R::HabitSurface), "habit-surface");
        assert_eq!(
            not_undoable_reason_kind(R::ReflectionSurface),
            "reflection-surface"
        );
    }
}

#[cfg(test)]
mod live_board_resolution_tests {
    use super::resolve_live_board;
    use crate::commands::CommandError;
    use taskboard_domain::BoardClocks;
    use taskboard_domain::{AppState, Board, BoardId, Color};

    /// The live filter is a conjunction: a tombstoned (deleted,
    /// unarchived) board must stay unresolvable by title — the M1 sweep
    /// found the `&&` floppable to `||` with the full suite green.
    #[test]
    fn a_deleted_board_is_not_resolvable_by_title() {
        let mut state = AppState::default();
        let ghost = Board {
            id: BoardId::from_uuid(uuid::Uuid::new_v4()),
            remote: None,
            title: "ghost".to_owned(),
            color: Color::new("0000ff"),
            archived: false,
            deleted: true,
            clocks: BoardClocks {
                title: BoardClocks::epoch(),
                color: BoardClocks::epoch(),
                archived: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        };
        state.boards.insert(ghost.id, ghost);
        assert!(matches!(
            resolve_live_board(&state, "ghost"),
            Err(CommandError::UnknownBoardSelector { .. })
        ));
    }
}
