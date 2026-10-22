// SPDX-License-Identifier: AGPL-3.0-only
//! CLI output contract (ADR 0009 §output): typed serde projections —
//! never the domain's serde shapes — rendered as human text or, under
//! `--json`, as JSON on stdout. Runtime failures print
//! `{"error":{"kind":…}}` on stderr; the `kind` strings come from
//! [`crate::commands::CommandError::error_kind`].
//!
//! Stability: the JSON payload shapes are internal-but-scriptable and
//! explicitly *not* versioned-guaranteed (backlog: JSON schema
//! versioning). No projection type ever carries a secret.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use taskboard_domain::{AppState, SyncErrorKind, SyncPhase, SyncStatus, Task, TaskId};

/// One row of `boards list` (remote Deck ids — what `boards select`
/// consumes).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BoardsListEntry {
    /// Remote Deck board id.
    pub id: u64,
    /// Board title.
    pub title: String,
    /// Archived on the server.
    pub archived: bool,
    /// Soft-deleted on the server.
    pub deleted: bool,
}

/// The list-render due banner: `null` / `"overdue"` / `"due-soon"` in
/// the `--json` payload. Banner truth only — derived from the task's
/// own due date, ignoring rules, deliveries, snoozes, and mute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DueMarker {
    /// Live, open, due date in the past.
    Overdue,
    /// Live, open, due within the horizon.
    DueSoon,
}

/// The due-marker context of one list render: the domain-computed
/// overdue/due-soon id sets, computed once per render (O(n)), looked up
/// per row.
#[derive(Debug, Clone, Default)]
pub struct DueContext {
    overdue: HashSet<TaskId>,
    due_soon: HashSet<TaskId>,
}

impl DueContext {
    /// Computes the sets for one render instant (one `now` per render —
    /// presentation, real clock).
    #[must_use]
    pub fn compute(state: &AppState, now: DateTime<Utc>) -> Self {
        Self {
            overdue: task_ids(taskboard_domain::overdue_tasks(&state.tasks, now)),
            due_soon: task_ids(taskboard_domain::due_soon(
                &state.tasks,
                now,
                crate::commands::tasks::DUE_SOON_HORIZON,
            )),
        }
    }

    /// The row's marker, by set membership.
    #[must_use]
    pub fn marker_of(&self, task: &Task) -> Option<DueMarker> {
        if self.overdue.contains(&task.id) {
            Some(DueMarker::Overdue)
        } else if self.due_soon.contains(&task.id) {
            Some(DueMarker::DueSoon)
        } else {
            None
        }
    }
}

fn task_ids<'a>(tasks: impl IntoIterator<Item = &'a Task>) -> HashSet<TaskId> {
    tasks.into_iter().map(|t| t.id).collect()
}

/// One task projection. `id`/`stack_id` are the local uuid strings
/// (`tasks list --json` is the scripting id source); `done` is the
/// boolean view over the completion timestamp.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskRow {
    /// Local task uuid.
    pub id: String,
    /// Task title.
    pub title: String,
    /// Completed?
    pub done: bool,
    /// Sort order within the stack.
    pub order: i64,
    /// Due date (ISO-8601) or `None`.
    pub duedate: Option<DateTime<Utc>>,
    /// The due banner marker (`null` when neither overdue nor due
    /// soon).
    pub due_marker: Option<DueMarker>,
    /// Owning stack's local uuid.
    pub stack_id: String,
    /// Owning stack's title.
    pub stack_title: String,
    /// Attached label titles (deterministic set order).
    pub labels: Vec<String>,
}

/// One stack group of the task list.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StackGroup {
    /// Local stack uuid.
    pub id: String,
    /// Stack title.
    pub title: String,
    /// Sort position within the board.
    pub order: i64,
    /// The stack's tasks in canonical order.
    pub tasks: Vec<TaskRow>,
}

/// The `tasks list` payload: stack groups in board order plus the
/// current sync status footer.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskList {
    /// Stack groups in board order.
    pub stacks: Vec<StackGroup>,
    /// Current sync status.
    pub sync: SyncStatusView,
}

/// The sync status footer shared by payloads that report one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncStatusView {
    /// `"idle" | "syncing" | "offline" | "failed"`.
    pub phase: String,
    /// Last successful full sync (ISO-8601) or `None`.
    pub last_success: Option<DateTime<Utc>>,
    /// Outbox depth (pending operations).
    pub pending_ops: u32,
    /// The failure classification when `phase == "failed"`.
    pub error_kind: Option<String>,
    /// Process-lifetime sync counters (cumulative per engine process
    /// run; never persisted — a restart resets them).
    pub cycles: u64,
    /// Ops successfully pushed.
    pub pushes: u64,
    /// Remote entities pulled (merged).
    pub pulls: u64,
    /// Cycles that ended in a failure.
    pub failures: u64,
    /// Offline transitions lived through.
    pub offline_episodes: u64,
    /// Wall duration of the board's last cycle, in milliseconds (`None`
    /// = no cycle counted yet). Flat number on the wire — never a
    /// `{secs, nanos}` pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_cycle_ms: Option<u64>,
    /// The board's sidecar-document warning, typed and flat (D-S2's
    /// loud fence): `None` = healthy (a read document, or the ruled
    /// silence). Additive with the `connectors` precedent — no protocol
    /// bump; `Healthy` serializes as absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidecar: Option<SidecarHealthView>,
}

/// The typed sidecar warning (ADR 0009's flat envelope): a kebab/snake
/// `reason` tag plus the format version where the reason carries one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SidecarHealthView {
    /// `"format_too_new" | "transport" | "decode" | "foreign_board"`.
    pub reason: String,
    /// The document's too-new format version (`format_too_new` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<u32>,
}

/// One connector's status row (D6-3): the derived health aggregate over
/// its bound boards' sync rows, and those boards in board-id order. A
/// zero-bound connector carries the default `health` and an empty
/// `boards` — configured-and-idle, never hidden.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectorStatusView {
    /// The config-minted connector name (the `--connector` token).
    pub name: String,
    /// The health aggregate over this connector's bound boards
    /// (`aggregate_rows`, the domain fold — attribution is derived from
    /// bindings, never stored).
    pub health: SyncStatusView,
    /// The connector's bound boards, board-id order.
    pub boards: Vec<BoardStatusView>,
}

/// One board's status row in the status answer's arrays (D6-3/D6-4).
/// The `Option` fields serialize as `null`, never absent — inside the
/// arrays the keys are always present (the "local-only" fact is the
/// nulls, structurally distinguishable from a skipped key).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BoardStatusView {
    /// Local board uuid.
    pub board: String,
    /// Board title.
    pub title: String,
    /// The bound remote board id; `null` = local-only.
    pub remote: Option<u64>,
    /// The derived connector attribution (the binding's tag); `null` =
    /// local-only.
    pub connector: Option<String>,
    /// The board's sync map row, or the default when none exists yet
    /// (e.g. a just-bound board before its first cycle). Stranded ops
    /// of an unbound board are *not* counted here — that count is the
    /// unbind receipt's and `read_scope`'s job.
    pub sync: SyncStatusView,
}

impl SyncStatusView {
    /// Projects a domain status; never leaks domain serde casing.
    #[must_use]
    pub fn new(status: &SyncStatus) -> Self {
        let (phase, error_kind) = match status.phase {
            SyncPhase::Idle => ("idle", None),
            SyncPhase::Syncing => ("syncing", None),
            SyncPhase::Offline => ("offline", None),
            SyncPhase::Failed { last_error } => {
                ("failed", Some(sync_error_kind(last_error).to_owned()))
            }
        };
        Self {
            phase: phase.to_owned(),
            last_success: status.last_success,
            pending_ops: status.pending_ops,
            error_kind,
            cycles: status.counters.cycles,
            pushes: status.counters.pushes,
            pulls: status.counters.pulls,
            failures: status.counters.failures,
            offline_episodes: status.counters.offline_episodes,
            last_cycle_ms: status
                .counters
                .last_cycle
                .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
            sidecar: sidecar_view(status.sidecar),
        }
    }
}

/// Projects the domain health into the flat typed view; healthy is
/// absent on the wire.
#[must_use]
pub fn sidecar_view(health: taskboard_domain::SidecarHealth) -> Option<SidecarHealthView> {
    let taskboard_domain::SidecarHealth::Unreadable(reason) = health else {
        return None;
    };
    let view = match reason {
        taskboard_domain::AppDataUnreadable::FormatTooNew { format } => SidecarHealthView {
            reason: "format_too_new".to_owned(),
            format: Some(format),
        },
        taskboard_domain::AppDataUnreadable::Transport => SidecarHealthView {
            reason: "transport".to_owned(),
            format: None,
        },
        taskboard_domain::AppDataUnreadable::Decode => SidecarHealthView {
            reason: "decode".to_owned(),
            format: None,
        },
        taskboard_domain::AppDataUnreadable::ForeignBoard => SidecarHealthView {
            reason: "foreign_board".to_owned(),
            format: None,
        },
    };
    Some(view)
}

/// The kebab-case sync failure classification (structural, matched in
/// tests by value — never message text).
#[must_use]
pub fn sync_error_kind(kind: SyncErrorKind) -> &'static str {
    match kind {
        SyncErrorKind::Network => "network",
        SyncErrorKind::Auth => "auth",
        SyncErrorKind::Forbidden => "forbidden",
        SyncErrorKind::Server => "server",
        SyncErrorKind::BadRequest => "bad-request",
        SyncErrorKind::LocalData => "local-data",
        SyncErrorKind::NoBoard => "no-board",
    }
}

/// The `tasks add/done/move` receipt: the new/updated task projection,
/// machine-readable under `--json` for the E2E's chaining.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MutationReceipt {
    /// Local task uuid.
    pub id: String,
    /// Task title.
    pub title: String,
    /// Owning stack's local uuid.
    pub stack_id: String,
    /// Owning stack's title.
    pub stack_title: String,
    /// Sort order within the stack.
    pub order: i64,
    /// Completed?
    pub done: bool,
    /// Attached label titles (deterministic set order).
    pub labels: Vec<String>,
}

/// What the post-logout daemon nudge did (WI-H, decision D5): kebab-case
/// in the `--json` receipt; the degradation is best-effort and never
/// changes the exit code.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DaemonConnectorState {
    /// The daemon acknowledged and detached; the credential is gone.
    Dropped,
    /// No daemon serves the db, or the serving daemon holds no connector.
    None,
    /// The daemon serves a different identity; its connector keeps that
    /// identity until restart.
    Mismatch,
    /// The daemon could not be reached (no answer / pre-detach daemon).
    Unreachable,
    /// The daemon answers but the outcome is unknowable (version
    /// mismatch, client budget expiry).
    Unknown,
}

/// The in-session `logout`'s own connector outcome (ADR 0014): the
/// session *is* the lock holder, so the detach it performed is the
/// whole story. Kebab-case in the `--json` receipt — the same
/// vocabulary as the WI-H IPC detach outcomes, honestly named for the
/// session (`daemon_connector` stays the one-shot's field).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SessionConnectorState {
    /// The session's connector was detached (acknowledged, joined in
    /// budget); the credential is gone.
    Detached,
    /// No connector was attached (standalone boot, or already
    /// detached) — idempotent.
    NoConnector,
    /// The actor missed the join budget and was aborted; the credential
    /// died with the task either way.
    TimedOut,
}

/// The one-shot `connect`'s daemon-attach nudge outcome (the mirror of
/// the logout nudge's [`DaemonConnectorState`]): did the running daemon
/// pick up the connection just persisted? Kebab-case in the `--json`
/// receipt.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DaemonAttachState {
    /// The daemon attached and settled — the connection is live there.
    Attached,
    /// No daemon serves this database — nothing to nudge, not a
    /// degradation.
    None,
    /// The daemon's persisted identity differs from the one just
    /// connected — nothing changed there.
    Mismatch,
    /// The daemon could not be reached (a pre-`attach-connector` daemon
    /// drops the link too).
    Unreachable,
    /// The daemon answered with a refusal the client cannot remedy from
    /// here (not-configured, credential-unavailable, engine, draining)
    /// or in a foreign protocol version.
    Unknown,
}

/// The `logout` receipt. No secret ever appears in any projection.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LogoutReceipt {
    /// The server the credentials were deleted for.
    pub server_url: String,
    /// The login name.
    pub username: String,
    /// Whether the config file's server/user keys were stripped (the
    /// auto backend only; an explicit backend is never rewritten).
    pub config_cleared: bool,
    /// The post-logout daemon nudge's outcome (WI-H), when a daemon was
    /// consulted; absent for the pure-local shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon_connector: Option<DaemonConnectorState>,
    /// The in-session detach outcome (ADR 0014); `None` for the
    /// one-shot, which never detaches its own process's connector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_connector: Option<SessionConnectorState>,
}

/// One row of `labels list`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LabelRow {
    /// Local label uuid.
    pub id: String,
    /// Label title.
    pub title: String,
    /// Label color as stored (six hex digits for CLI-created labels).
    pub color: String,
    /// Count of live tasks carrying the label.
    pub assigned_tasks: usize,
}

/// The `labels add/edit` receipt: the label projection.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LabelReceipt {
    /// Local label uuid.
    pub id: String,
    /// Label title.
    pub title: String,
    /// Label color.
    pub color: String,
}

/// The `stacks delete` receipt: the pre-execute snapshot plus the
/// cascade's blast radius (the domain tombstones the stack's live tasks).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StackDeleteReceipt {
    /// Local stack uuid.
    pub id: String,
    /// Stack title at delete time.
    pub title: String,
    /// Live tasks the cascade tombstoned.
    pub cascaded_tasks: usize,
}

/// The `tasks delete` receipt: the pre-execute snapshot (the task is
/// tombstoned afterwards, so there is no post-state to project).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskDeleteReceipt {
    /// Local task uuid.
    pub id: String,
    /// Task title at delete time.
    pub title: String,
}

/// One row of `tasks remind --list`. `before` is the canonical text
/// form (`"10m"`, `"0s"`) — the *config* string stays raw, the
/// projection is canonical.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReminderRow {
    /// Local rule uuid.
    pub id: String,
    /// Owning task's local uuid.
    pub task_id: String,
    /// Owning task's title.
    pub task_title: String,
    /// The rule's offset before due, canonical text.
    pub before: String,
    /// When the rule was created (ISO-8601).
    pub created_at: DateTime<Utc>,
    /// The task's firings are muted (mutes are visible where rules
    /// are).
    pub task_muted: bool,
}

/// The `tasks remind --list` payload: rows in task canonical order,
/// then rule id.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RemindersList {
    /// The rule rows.
    pub rules: Vec<ReminderRow>,
}

/// The `tasks remind --before` receipt.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReminderSetReceipt {
    /// The fresh rule's local uuid.
    pub rule_id: String,
    /// The task's local uuid.
    pub task_id: String,
    /// The task's title.
    pub task_title: String,
    /// The rule's offset before due, canonical text.
    pub before: String,
}

/// The `tasks remind --clear` receipt.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReminderClearReceipt {
    /// The task's local uuid.
    pub task_id: String,
    /// The task's title.
    pub task_title: String,
    /// How many rules were removed (0 for an idempotent repeat).
    pub removed: usize,
}

/// The `tasks remind --off/--on` receipt.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReminderMuteReceipt {
    /// The task's local uuid.
    pub task_id: String,
    /// The task's title.
    pub task_title: String,
    /// The new mute state.
    pub muted: bool,
}

/// The `config print` payload: the effective configuration plus the
/// diagnosis (validity travels *in* the payload; exit 0 either way).
///
/// Not `PartialEq` — [`crate::config::AppConfig`] carries provider state
/// that does not implement it; assertions go through the field values.
#[derive(Debug, Clone, Serialize)]
pub struct ConfigView {
    /// The resolved config file path, or `None` (defaults only).
    pub config_path: Option<String>,
    /// The resolved sync backend: `"standalone"` or `"nextcloud"`.
    pub backend: String,
    /// `false` when the decoded config failed validation.
    pub valid: bool,
    /// The [`crate::config::ConfigError::kind`] discriminant when
    /// invalid.
    pub invalid_kind: Option<String>,
    /// The effective configuration (secret-free by construction).
    pub config: crate::config::AppConfig,
}

/// The `config path` payload: pure path facts — which file reads
/// resolve, and where `connect`/`login` would write. No config
/// content, no decode, no side effects.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConfigPathView {
    /// The file effective-config reads resolve (the `config print`
    /// resolution rule); `None` — defaults only.
    pub config_path: Option<String>,
    /// The `connect`/`login` write target (the first-run arm's
    /// candidate when resolution finds no file).
    pub write_path: String,
}

/// The `connect` (and deprecated `login` alias) receipt. No secret ever
/// appears in any projection.
// The bools are independent, documented receipt facts (what the
// ceremony did, what transport it used) — a state machine or enums
// would obscure the JSON shape the CLI contract pins.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConnectReceipt {
    /// The verified server URL.
    pub server_url: String,
    /// The login name.
    pub username: String,
    /// The server accepted the credentials.
    pub verified: bool,
    /// Where the secret went: `"keyring"` or `"env"` (read-only store —
    /// export `TASKBOARD_APP_PASSWORD` yourself).
    pub stored: String,
    /// An explicit `standalone` backend was loudly switched to
    /// `nextcloud` (flag or confirmed).
    pub backend_switched: bool,
    /// A configured different identity was replaced (confirmed or
    /// forced); its secret was deleted after the new one verified.
    pub repointed: bool,
    /// The session ceremony attached the connector to the running core
    /// (ADR 0014); `None` for the one-shot, which never owns a core —
    /// the one-shot JSON shape stays byte-identical.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attached: Option<bool>,
    /// The daemon-attach nudge's outcome (ADR 0014 decision 7): whether
    /// a daemon serving this database picked up the connection — absent
    /// when none was consulted (the pure-local shape).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon_connector: Option<DaemonAttachState>,
    /// The server URL's scheme is plain `http` — the app password
    /// travels unencrypted (F12-03). A plain bool: every receipt knows
    /// its URL, unlike the front-end-dependent fields beside it.
    pub cleartext_transport: bool,
}

/// The `boards select` receipt: the adopted board summary plus the sync
/// status observed at adoption.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SelectReceipt {
    /// The adopted board (remote id, title, flags).
    pub board: BoardsListEntry,
    /// Sync status at adoption.
    pub sync: SyncStatusView,
    /// Descendant remote refs the bind wiped, surfaced only when the
    /// bind actually cleared some (the no-wipe receipt stays
    /// byte-identical; `Some` rides the forecast note — the exact
    /// stranded-op count lands push-side).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiped_refs: Option<usize>,
}

/// The `boards publish` receipt: the created remote board plus the
/// settling cycle's outcome. The same shape as `SelectReceipt` (the
/// binding adoption *is* the select ceremony onto a fresh board).
pub type PublishReceipt = SelectReceipt;

/// The `boards create` receipt: the new local board projection.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BoardReceipt {
    /// Local board uuid.
    pub id: String,
    /// Board title.
    pub title: String,
    /// Board color as stored (lenient string).
    pub color: String,
}

/// The `boards use` receipt (REPL-only, D6-6): the session's selected
/// board. `board: null` serializes (not skips) — the honest "no board
/// selected" query answer; `title` is present only with a selection.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BoardUseView {
    /// The selected board's uuid, or `null` when none is selected.
    pub board: Option<String>,
    /// The selected board's title; absent without a selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// The `boards delete`/`boards reset` receipt: the landed counts of the
/// ceremony (pre-counted from the state the command planned against).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BoardDestructReceipt {
    /// The affected local board's uuid.
    pub board: String,
    /// The verb as performed (`delete` | `reset`).
    pub verb: String,
    /// Live stacks affected (reset) / tombstoned (delete).
    pub stacks: usize,
    /// Live tasks affected / tombstoned.
    pub tasks: usize,
    /// Live labels affected / tombstoned.
    pub labels: usize,
    /// Pending ops cancelled with the doomed entities (delete only;
    /// reset clears them with the content).
    pub pending_ops: usize,
    /// The remote board id when the board is bound (delete only: the
    /// remote content is destroyed server-side by the push).
    pub bound_remote: Option<u64>,
}

/// The `boards unbind` receipt: the landed ceremony — the ex-bound
/// remote, the ops that now strand honestly, and the refs a future
/// same-remote re-bind re-adopts.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UnbindReceipt {
    /// The unbound local board's uuid.
    pub board: String,
    /// The remote board id the binding was dropped from; `None` when
    /// the board was already unbound (the idempotent no-op receipt).
    pub bound_remote: Option<u64>,
    /// Outbox ops attributed to the board that now wait for a re-bind.
    pub stranded_ops: usize,
    /// Live descendant refs kept (a same-remote re-bind re-adopts them).
    pub kept_refs: usize,
}

/// The `boards reconcile` receipt: the merged same-title duplicates and
/// the cancelled duplicate create-ops.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReconcileReceipt {
    /// The reconciled local board's uuid.
    pub board: String,
    /// Merged stack pairs.
    pub stacks: usize,
    /// Merged label pairs.
    pub labels: usize,
    /// Merged task pairs.
    pub tasks: usize,
    /// Create-ops cancelled on survivors that just adopted a ref.
    pub cancelled_ops: usize,
}

/// The `stacks add` receipt: the new stack projection.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StackReceipt {
    /// Local stack uuid.
    pub id: String,
    /// Stack title.
    pub title: String,
    /// Sort position within the board.
    pub order: i64,
}

/// The informational `tasks list` stand-alone footer, or `None` when the
/// resolved backend is a connector (`standalone` is the caller's
/// decision; the note is presentation-only and carries the outbox depth
/// so operators see *why* nothing drains).
#[must_use]
pub fn standalone_note(standalone: bool, pending_ops: u32) -> Option<String> {
    standalone.then(|| format!("stand-alone: {pending_ops} changes waiting for a sync connector"))
}

/// The stderr error shape under `--json`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ErrorEnvelope {
    /// The error body.
    pub error: ErrorBody,
}

/// The typed error body: a stable kebab-case discriminator plus an
/// optional structural detail (ids, nested sync kind).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ErrorBody {
    /// The kebab-case [`crate::commands::CommandError`] discriminator.
    pub kind: String,
    /// Structural detail (e.g. the offending board id, the nested sync
    /// failure kind).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Builds a task row from a task, its surrounding state, and the
/// render's due-marker context.
#[must_use]
pub fn task_row(state: &AppState, task: &Task, due: &DueContext) -> TaskRow {
    let stack_title = state
        .stacks
        .get(&task.stack)
        .map_or_else(String::new, |s| s.title.clone());
    TaskRow {
        id: task.id.as_uuid().to_string(),
        title: task.title.clone(),
        done: task.done.is_some(),
        order: task.order,
        duedate: task.duedate,
        due_marker: due.marker_of(task),
        stack_id: task.stack.as_uuid().to_string(),
        stack_title,
        labels: task
            .labels
            .iter()
            .filter_map(|id| state.labels.get(id))
            .map(|l| l.title.clone())
            .collect(),
    }
}

/// Builds a mutation receipt for a task in the published state (the
/// receipt's marker context is computed for this render instant).
#[must_use]
pub fn mutation_receipt(state: &AppState, task: &Task) -> MutationReceipt {
    let due = DueContext::compute(state, Utc::now());
    let row = task_row(state, task, &due);
    MutationReceipt {
        id: row.id,
        title: row.title,
        stack_id: row.stack_id,
        stack_title: row.stack_title,
        order: row.order,
        done: row.done,
        labels: row.labels,
    }
}
