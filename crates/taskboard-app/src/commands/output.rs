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

use taskboard_domain::{AppState, SyncErrorKind, SyncPhase, SyncStatus, Task};

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
        }
    }
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

/// The `login` receipt. No secret ever appears in any projection.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LoginReceipt {
    /// The verified server URL.
    pub server_url: String,
    /// The login name.
    pub username: String,
    /// The server accepted the credentials.
    pub verified: bool,
    /// Where the secret went: `"keyring"` or `"env"` (read-only store —
    /// export `TASKBOARD_APP_PASSWORD` yourself).
    pub stored: String,
}

/// The `boards select` receipt: the adopted board summary plus the sync
/// status observed at adoption.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SelectReceipt {
    /// The adopted board (remote id, title, flags).
    pub board: BoardsListEntry,
    /// Sync status at adoption.
    pub sync: SyncStatusView,
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

/// Builds a task row from a task and its surrounding state.
#[must_use]
pub fn task_row(state: &AppState, task: &Task) -> TaskRow {
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

/// Builds a mutation receipt for a task in the published state.
#[must_use]
pub fn mutation_receipt(state: &AppState, task: &Task) -> MutationReceipt {
    let row = task_row(state, task);
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
