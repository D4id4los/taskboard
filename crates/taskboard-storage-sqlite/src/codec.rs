// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hand-written `TEXT` codecs between domain tag types and their stored
//! string forms (storage plan §3.10).
//!
//! The forms mirror the domain's serde wire shapes (`"boards"`,
//! `"stacks:<u64>"`, snake_case-style op tags) so the database stays inspectable
//! without pulling serde into this crate. Unknown tags or missing id
//! columns decode to [`Corrupted`](taskboard_domain::persistence::RepositoryError)
//! via [`CodecError`].

use taskboard_domain::ids::{BoardId, LabelId, RemoteBoardId, StackId, TaskId};
use taskboard_domain::outbox::LocalOp;
use taskboard_domain::persistence::ValidatorKey;
use taskboard_domain::reminders::ReminderRef;
use taskboard_domain::state::{SyncErrorKind, SyncPhase};
use uuid::Uuid;

/// The stored TEXT form of an importance/urgency level: the domain
/// enum's serde unit name (the one-representation discipline — storage
/// TEXT *is* the serde encoding).
#[must_use]
pub fn importance_to_text(level: taskboard_domain::entities::Importance) -> &'static str {
    use taskboard_domain::entities::Importance::{High, Low, Normal, VeryHigh, VeryLow};
    match level {
        VeryLow => "very_low",
        Low => "low",
        Normal => "normal",
        High => "high",
        VeryHigh => "very_high",
    }
}

/// Parses the stored TEXT form back; `None` on an unknown level.
#[must_use]
pub fn importance_from_text(raw: &str) -> Option<taskboard_domain::entities::Importance> {
    use taskboard_domain::entities::Importance::{High, Low, Normal, VeryHigh, VeryLow};
    match raw {
        "very_low" => Some(VeryLow),
        "low" => Some(Low),
        "normal" => Some(Normal),
        "high" => Some(High),
        "very_high" => Some(VeryHigh),
        _ => None,
    }
}

/// The stored TEXT form of an urgency level (see
/// [`importance_to_text`]).
#[must_use]
pub fn urgency_to_text(level: taskboard_domain::entities::Urgency) -> &'static str {
    use taskboard_domain::entities::Urgency::{High, Low, Normal, VeryHigh, VeryLow};
    match level {
        VeryLow => "very_low",
        Low => "low",
        Normal => "normal",
        High => "high",
        VeryHigh => "very_high",
    }
}

/// Parses the stored TEXT form back; `None` on an unknown level.
#[must_use]
pub fn urgency_from_text(raw: &str) -> Option<taskboard_domain::entities::Urgency> {
    use taskboard_domain::entities::Urgency::{High, Low, Normal, VeryHigh, VeryLow};
    match raw {
        "very_low" => Some(VeryLow),
        "low" => Some(Low),
        "normal" => Some(Normal),
        "high" => Some(High),
        "very_high" => Some(VeryHigh),
        _ => None,
    }
}

/// The stored TEXT form of a reminder insistence rung: the domain
/// enum's serde unit name (single words, so the serde kebab-case and
/// the TEXT form coincide — the one-representation discipline).
#[must_use]
pub fn insistence_to_text(level: taskboard_domain::reminders::Insistence) -> &'static str {
    use taskboard_domain::reminders::Insistence::{Alarm, Normal};
    match level {
        Normal => "normal",
        Alarm => "alarm",
    }
}

/// Parses the stored TEXT form back; `None` on an unknown rung.
#[must_use]
pub fn insistence_from_text(raw: &str) -> Option<taskboard_domain::reminders::Insistence> {
    use taskboard_domain::reminders::Insistence::{Alarm, Normal};
    match raw {
        "normal" => Some(Normal),
        "alarm" => Some(Alarm),
        _ => None,
    }
}

/// The stored TEXT form of a recurring-series rule: the domain serde
/// JSON of the rule (the `Option` wrapper is not stored — NULL is the
/// unset state at the column level).
#[must_use]
pub fn recur_to_text(rule: &taskboard_domain::recur::RecurRule) -> String {
    serde_json::to_string(rule).unwrap_or_default()
}

/// Parses the stored TEXT form back; `None` on any JSON that does not
/// decode as a rule (strict decode by design — the mints reject, the
/// repository reports `Corrupted`; a NULL column is handled at the
/// caller as the unset state).
pub fn recur_from_text(raw: &str) -> Option<taskboard_domain::recur::RecurRule> {
    serde_json::from_str(raw).ok()
}

/// The stored TEXT form of a task's remote baseline: the domain serde
/// JSON of the record (the `Option` wrapper is not stored — NULL is the
/// never-observed state at the column level, the fallback trigger).
#[must_use]
pub fn remote_baseline_to_text(
    baseline: &taskboard_domain::entities::RemoteTaskBaseline,
) -> String {
    serde_json::to_string(baseline).unwrap_or_default()
}

/// A decode rejection; the repository maps this onto
/// [`RepositoryError::Corrupted`](taskboard_domain::persistence::RepositoryError).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("stored value failed to decode")]
pub struct CodecError;

/// Hyphenated-UUID text form of a local id.
#[must_use]
pub fn uuid_to_text(id: Uuid) -> String {
    id.to_string()
}

/// Parses a hyphenated-UUID text form back into a local id.
///
/// # Errors
///
/// [`CodecError`] when the text is not a valid UUID.
pub fn uuid_from_text(raw: &str) -> Result<Uuid, CodecError> {
    Uuid::parse_str(raw).map_err(|_| CodecError)
}

/// The snake-case tag stored in `outbox.op_kind`.
#[must_use]
pub fn op_kind_to_text(op: &LocalOp) -> &'static str {
    match op {
        LocalOp::CreateTask(_) => "create_task",
        LocalOp::UpdateTask(_) => "update_task",
        LocalOp::MoveTask(_) => "move_task",
        LocalOp::DeleteTask(_) => "delete_task",
        LocalOp::CreateStack(_) => "create_stack",
        LocalOp::RenameStack(_) => "rename_stack",
        LocalOp::MoveStack(_) => "move_stack",
        LocalOp::DeleteStack(_) => "delete_stack",
        LocalOp::CreateLabel(_) => "create_label",
        LocalOp::UpdateLabel(_) => "update_label",
        LocalOp::DeleteLabel(_) => "delete_label",
        LocalOp::AssignLabel(_, _) => "assign_label",
        LocalOp::UnassignLabel(_, _) => "unassign_label",
        LocalOp::UpdateBoard(_) => "update_board",
        LocalOp::UpdateAppData(_) => "update_app_data",
        LocalOp::DeleteBoard(_) => "delete_board",
        LocalOp::DeleteRemoteCard { .. } => "delete_remote_card",
        LocalOp::DeleteRemoteStack { .. } => "delete_remote_stack",
    }
}

/// Which id columns an op kind requires in the `outbox` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_field_names)] // the *_id names mirror the SQL columns
pub(crate) struct OpColumns {
    pub(crate) task_id: Option<TaskId>,
    pub(crate) stack_id: Option<StackId>,
    pub(crate) label_id: Option<LabelId>,
    pub(crate) board_id: Option<BoardId>,
}

/// Decomposes an op into the id columns it addresses (storage plan §4.1:
/// `assign_label`/`unassign_label` need both task and label).
#[must_use]
pub(crate) fn op_to_columns(op: &LocalOp) -> OpColumns {
    match *op {
        LocalOp::CreateTask(task)
        | LocalOp::UpdateTask(task)
        | LocalOp::MoveTask(task)
        | LocalOp::DeleteTask(task)
        | LocalOp::UpdateAppData(task) => OpColumns {
            task_id: Some(task),
            stack_id: None,
            label_id: None,
            board_id: None,
        },
        LocalOp::CreateStack(stack)
        | LocalOp::RenameStack(stack)
        | LocalOp::MoveStack(stack)
        | LocalOp::DeleteStack(stack) => OpColumns {
            task_id: None,
            stack_id: Some(stack),
            label_id: None,
            board_id: None,
        },
        LocalOp::CreateLabel(label) | LocalOp::UpdateLabel(label) | LocalOp::DeleteLabel(label) => {
            OpColumns {
                task_id: None,
                stack_id: None,
                label_id: Some(label),
                board_id: None,
            }
        }
        LocalOp::AssignLabel(task, label) | LocalOp::UnassignLabel(task, label) => OpColumns {
            task_id: Some(task),
            stack_id: None,
            label_id: Some(label),
            board_id: None,
        },
        LocalOp::UpdateBoard(board) | LocalOp::DeleteBoard(board) => OpColumns {
            task_id: None,
            stack_id: None,
            label_id: None,
            board_id: Some(board),
        },
        // Ref-deletes attribute through their local board; the remote
        // ref itself rides `op_payload` (no local-id column can hold it).
        LocalOp::DeleteRemoteCard { board, .. } | LocalOp::DeleteRemoteStack { board, .. } => {
            OpColumns {
                task_id: None,
                stack_id: None,
                label_id: None,
                board_id: Some(board),
            }
        }
    }
}

/// The JSON stored in `outbox.payload` for the op kinds whose target
/// carries remote ids (the ref-deletes, #150): `None` for every
/// column-addressed op kind.
#[must_use]
pub fn op_payload_to_json(op: &LocalOp) -> Option<String> {
    match op {
        LocalOp::DeleteRemoteCard { card, .. } => Some(serde_json::to_string(card).ok()?),
        LocalOp::DeleteRemoteStack { stack, .. } => Some(serde_json::to_string(stack).ok()?),
        _ => None,
    }
}

/// Rebuilds an op from its tag plus id columns; a required column being
/// NULL is the "missing required op column" corruption class (plan T7).
///
/// # Errors
///
/// [`CodecError`] on an unknown tag or a required id column being absent
/// (or unparseable).
pub fn op_from_columns(
    tag: &str,
    task_id: Option<&str>,
    stack_id: Option<&str>,
    label_id: Option<&str>,
    board_id: Option<&str>,
    payload: Option<&str>,
) -> Result<LocalOp, CodecError> {
    fn parse_id<T: From<Uuid>>(raw: Option<&str>) -> Result<T, CodecError> {
        raw.map(uuid_from_text)
            .transpose()?
            .map(T::from)
            .ok_or(CodecError)
    }
    match tag {
        "create_task" => Ok(LocalOp::CreateTask(parse_id(task_id)?)),
        "update_task" => Ok(LocalOp::UpdateTask(parse_id(task_id)?)),
        "move_task" => Ok(LocalOp::MoveTask(parse_id(task_id)?)),
        "delete_task" => Ok(LocalOp::DeleteTask(parse_id(task_id)?)),
        "create_stack" => Ok(LocalOp::CreateStack(parse_id(stack_id)?)),
        "rename_stack" => Ok(LocalOp::RenameStack(parse_id(stack_id)?)),
        "move_stack" => Ok(LocalOp::MoveStack(parse_id(stack_id)?)),
        "delete_stack" => Ok(LocalOp::DeleteStack(parse_id(stack_id)?)),
        "create_label" => Ok(LocalOp::CreateLabel(parse_id(label_id)?)),
        "update_label" => Ok(LocalOp::UpdateLabel(parse_id(label_id)?)),
        "delete_label" => Ok(LocalOp::DeleteLabel(parse_id(label_id)?)),
        "assign_label" => Ok(LocalOp::AssignLabel(
            parse_id(task_id)?,
            parse_id(label_id)?,
        )),
        "unassign_label" => Ok(LocalOp::UnassignLabel(
            parse_id(task_id)?,
            parse_id(label_id)?,
        )),
        "update_board" => Ok(LocalOp::UpdateBoard(parse_id::<BoardId>(board_id)?)),
        "update_app_data" => Ok(LocalOp::UpdateAppData(parse_id(task_id)?)),
        "delete_board" => Ok(LocalOp::DeleteBoard(parse_id::<BoardId>(board_id)?)),
        // Ref-deletes (#150): the local board stays a column, the remote
        // ref decodes strictly from the payload (missing or undecodable
        // is the corruption class, like a missing required column).
        "delete_remote_card" => Ok(LocalOp::DeleteRemoteCard {
            board: parse_id::<BoardId>(board_id)?,
            card: serde_json::from_str(payload.ok_or(CodecError)?).map_err(|_| CodecError)?,
        }),
        "delete_remote_stack" => Ok(LocalOp::DeleteRemoteStack {
            board: parse_id::<BoardId>(board_id)?,
            stack: serde_json::from_str(payload.ok_or(CodecError)?).map_err(|_| CodecError)?,
        }),
        _ => Err(CodecError),
    }
}

/// The lowercase tag stored in `sync_status.phase`.
#[must_use]
pub fn sync_phase_to_text(phase: SyncPhase) -> &'static str {
    match phase {
        SyncPhase::Idle => "idle",
        SyncPhase::Syncing => "syncing",
        SyncPhase::Offline => "offline",
        SyncPhase::Failed { .. } => "failed",
    }
}

/// The lowercase tag stored in `sync_status.last_error`.
#[must_use]
pub fn sync_error_kind_to_text(kind: SyncErrorKind) -> &'static str {
    match kind {
        SyncErrorKind::Network => "network",
        SyncErrorKind::Auth => "auth",
        SyncErrorKind::Forbidden => "forbidden",
        SyncErrorKind::Server => "server",
        SyncErrorKind::BadRequest => "bad_request",
        SyncErrorKind::LocalData => "local_data",
        SyncErrorKind::NoBoard => "no_board",
    }
}

/// Rebuilds a phase from the `phase` + `last_error` columns. `Failed`
/// requires its error kind; non-failed phases require the absence of one
/// (a dangling error tag means the row is not self-consistent).
///
/// # Errors
///
/// [`CodecError`] on unknown tags or a `last_error`/phase mismatch.
pub fn sync_phase_from_columns(
    phase: &str,
    last_error: Option<&str>,
) -> Result<SyncPhase, CodecError> {
    match phase {
        "idle" if last_error.is_none() => Ok(SyncPhase::Idle),
        "syncing" if last_error.is_none() => Ok(SyncPhase::Syncing),
        "offline" if last_error.is_none() => Ok(SyncPhase::Offline),
        "failed" => Ok(SyncPhase::Failed {
            last_error: match last_error {
                Some("network") => SyncErrorKind::Network,
                Some("auth") => SyncErrorKind::Auth,
                Some("forbidden") => SyncErrorKind::Forbidden,
                Some("server") => SyncErrorKind::Server,
                Some("bad_request") => SyncErrorKind::BadRequest,
                Some("local_data") => SyncErrorKind::LocalData,
                Some("no_board") => SyncErrorKind::NoBoard,
                _ => return Err(CodecError),
            },
        }),
        _ => Err(CodecError),
    }
}

/// The stored `sync_metadata.key` form (identical to the serde wire form).
#[must_use]
pub fn validator_key_to_text(key: &ValidatorKey) -> String {
    match *key {
        ValidatorKey::Boards => "boards".to_string(),
        ValidatorKey::Stacks(board) => format!("stacks:{}", board.get()),
        ValidatorKey::ArchivedStacks(board) => format!("archived_stacks:{}", board.get()),
    }
}

/// Parses a `sync_metadata.key`.
///
/// # Errors
///
/// [`CodecError`] on any other shape.
pub fn validator_key_from_text(raw: &str) -> Result<ValidatorKey, CodecError> {
    if raw == "boards" {
        return Ok(ValidatorKey::Boards);
    }
    if let Some(num) = raw
        .strip_prefix("archived_stacks:")
        .and_then(|num| num.parse::<u64>().ok())
    {
        return Ok(ValidatorKey::ArchivedStacks(RemoteBoardId(num)));
    }
    raw.strip_prefix("stacks:")
        .and_then(|num| num.parse::<u64>().ok())
        .map(|num| ValidatorKey::Stacks(RemoteBoardId(num)))
        .ok_or(CodecError)
}

/// The stored `reminder_deliveries.reminder` / `reminder_snoozes.reminder`
/// form (identical to [`ReminderRef::text_form`]).
#[must_use]
pub fn reminder_ref_to_text(reminder: ReminderRef) -> String {
    reminder.text_form()
}

/// Parses a stored reminder reference.
///
/// # Errors
///
/// [`CodecError`] on any text outside the two text forms.
pub fn reminder_ref_from_text(raw: &str) -> Result<ReminderRef, CodecError> {
    ReminderRef::parse_text_form(raw).ok_or(CodecError)
}

/// Encodes a remote stack binding into its (`remote_board_id`,
/// `remote_stack_id`) columns.
#[must_use]
pub fn remote_stack_to_columns(r: taskboard_domain::ids::RemoteStackRef) -> (i64, i64) {
    (remote_col(r.board.get()), remote_col(r.stack.get()))
}

/// Encodes a remote card binding into its (`remote_board_id`,
/// `remote_stack_id`, `remote_card_id`) columns.
#[must_use]
pub fn remote_card_to_columns(r: taskboard_domain::ids::RemoteCardRef) -> (i64, i64, i64) {
    (
        remote_col(r.board.get()),
        remote_col(r.stack.get()),
        remote_col(r.card.get()),
    )
}

/// Encodes a remote label binding into its (`remote_board_id`,
/// `remote_label_id`) columns.
#[must_use]
pub fn remote_label_to_columns(r: taskboard_domain::ids::RemoteLabelRef) -> (i64, i64) {
    (remote_col(r.board.get()), remote_col(r.label.get()))
}

/// The all-or-none NULL invariant of remote ref columns, checked at decode
/// time: `Some` columns present ⇔ `Some` ref (plan decision 11).
///
/// # Errors
///
/// [`CodecError`] on a partial column set (some set, some NULL) or a
/// negative stored id.
pub(crate) fn remote_columns_present_or_none(
    ids: &[Option<i64>],
) -> Result<Option<Vec<u64>>, CodecError> {
    let mut out = Vec::with_capacity(ids.len());
    for col in ids {
        match col {
            // Negative ids cannot come from a u64 remote id — corruption.
            Some(num) => out.push(u64::try_from(*num).map_err(|_| CodecError)?),
            None => {
                // A NULL is legal only if every column is NULL.
                return if ids.iter().all(Option::is_none) {
                    Ok(None)
                } else {
                    Err(CodecError)
                };
            }
        }
    }
    Ok(Some(out))
}

/// A remote id as an INTEGER column. Deck ids are sequential `u64`s, far
/// below `i64::MAX`; a value above that would silently saturate (the
/// decode-side mirror treats negatives as corruption).
pub(crate) fn remote_col(raw: u64) -> i64 {
    i64::try_from(raw).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codec_payload(op: &LocalOp) -> Option<String> {
        op_payload_to_json(op)
    }
    use taskboard_domain::ids::{RemoteBoardId, RemoteCardId, RemoteLabelId, RemoteStackId};

    fn id(raw: u128) -> Uuid {
        Uuid::from_u128(raw)
    }

    #[test]
    fn uuid_text_roundtrips_and_rejects_garbage() {
        let raw = id(42);
        assert_eq!(uuid_from_text(&uuid_to_text(raw)), Ok(raw));
        assert_eq!(uuid_from_text("not-a-uuid"), Err(CodecError));
    }

    #[test]
    fn insistence_text_roundtrips_and_rejects_unknown() {
        use taskboard_domain::reminders::Insistence;
        assert_eq!(insistence_to_text(Insistence::Normal), "normal");
        assert_eq!(insistence_to_text(Insistence::Alarm), "alarm");
        assert_eq!(insistence_from_text("normal"), Some(Insistence::Normal));
        assert_eq!(insistence_from_text("alarm"), Some(Insistence::Alarm));
        assert_eq!(insistence_from_text("wake_up"), None);
        assert_eq!(insistence_from_text(""), None);
    }

    #[test]
    fn op_columns_roundtrip_every_variant() {
        let task = TaskId::from(id(1));
        let stack = StackId::from(id(2));
        let label = LabelId::from(id(3));
        let ops = [
            LocalOp::CreateTask(task),
            LocalOp::UpdateTask(task),
            LocalOp::MoveTask(task),
            LocalOp::DeleteTask(task),
            LocalOp::CreateStack(stack),
            LocalOp::RenameStack(stack),
            LocalOp::MoveStack(stack),
            LocalOp::DeleteStack(stack),
            LocalOp::CreateLabel(label),
            LocalOp::UpdateLabel(label),
            LocalOp::DeleteLabel(label),
            LocalOp::AssignLabel(task, label),
            LocalOp::UnassignLabel(task, label),
            LocalOp::UpdateBoard(taskboard_domain::ids::BoardId::from(id(9))),
            LocalOp::DeleteBoard(taskboard_domain::ids::BoardId::from(id(9))),
            LocalOp::UpdateAppData(taskboard_domain::ids::TaskId::from(id(3))),
        ];
        for op in ops {
            let cols = op_to_columns(&op);
            let task_text = cols.task_id.map(|t| uuid_to_text(t.as_uuid()));
            let stack_text = cols.stack_id.map(|s| uuid_to_text(s.as_uuid()));
            let label_text = cols.label_id.map(|l| uuid_to_text(l.as_uuid()));
            let board_text = cols.board_id.map(|b| uuid_to_text(b.as_uuid()));
            let payload_text = codec_payload(&op);
            let back = op_from_columns(
                op_kind_to_text(&op),
                task_text.as_deref(),
                stack_text.as_deref(),
                label_text.as_deref(),
                board_text.as_deref(),
                payload_text.as_deref(),
            );
            assert_eq!(back, Ok(op), "{op:?} must round-trip through columns");
        }
    }

    #[test]
    fn unknown_op_tag_is_rejected() {
        assert_eq!(
            op_from_columns("nope", None, None, None, None, None),
            Err(CodecError)
        );
    }

    #[test]
    fn ref_delete_ops_roundtrip_through_board_column_and_payload() {
        let board = taskboard_domain::ids::BoardId::from(id(9));
        let card = taskboard_domain::ids::RemoteCardRef {
            board: taskboard_domain::ids::RemoteBoardId(42),
            stack: taskboard_domain::ids::RemoteStackId(7),
            card: taskboard_domain::ids::RemoteCardId(517),
        };
        let stack_ref = taskboard_domain::ids::RemoteStackRef {
            board: taskboard_domain::ids::RemoteBoardId(42),
            stack: taskboard_domain::ids::RemoteStackId(8),
        };
        for op in [
            LocalOp::DeleteRemoteCard { board, card },
            LocalOp::DeleteRemoteStack {
                board,
                stack: stack_ref,
            },
        ] {
            let payload = codec_payload(&op).expect("ref-delete carries a payload");
            let back = op_from_columns(
                op_kind_to_text(&op),
                None,
                None,
                None,
                Some(&uuid_to_text(board.as_uuid())),
                Some(&payload),
            );
            assert_eq!(back, Ok(op), "{op:?} must round-trip");
        }
    }

    #[test]
    fn ref_delete_ops_require_board_and_payload() {
        // Missing payload is the corruption class, like a missing column.
        assert_eq!(
            op_from_columns(
                "delete_remote_card",
                None,
                None,
                None,
                Some(&uuid_to_text(id(9))),
                None
            ),
            Err(CodecError)
        );
        // Missing board likewise.
        assert_eq!(
            op_from_columns(
                "delete_remote_stack",
                None,
                None,
                None,
                None,
                Some(r#"{"board":42,"stack":8}"#)
            ),
            Err(CodecError)
        );
        // Undecodable payload likewise.
        assert_eq!(
            op_from_columns(
                "delete_remote_card",
                None,
                None,
                None,
                Some(&uuid_to_text(id(9))),
                Some("{bad")
            ),
            Err(CodecError)
        );
        // Column-addressed op kinds carry no payload.
        assert_eq!(
            op_payload_to_json(&LocalOp::CreateTask(taskboard_domain::ids::TaskId::from(
                id(1)
            ))),
            None
        );
    }

    #[test]
    fn missing_required_op_column_is_rejected() {
        // Task ops need task_id.
        assert_eq!(
            op_from_columns("create_task", None, None, None, None, None),
            Err(CodecError)
        );
        // The sidecar op is task-scoped like its siblings.
        assert_eq!(
            op_from_columns("update_app_data", None, None, None, None, None),
            Err(CodecError)
        );
        // assign_label needs both columns.
        assert_eq!(
            op_from_columns(
                "assign_label",
                Some(&uuid_to_text(id(1))),
                None,
                None,
                None,
                None
            ),
            Err(CodecError)
        );
        // The legacy `rename_board` tag is unknown after the 0005 tag
        // rename (one representation, no alias drift) — a rejection, not
        // a silent alias.
        assert_eq!(
            op_from_columns(
                "rename_board",
                None,
                Some(&uuid_to_text(id(2))),
                None,
                None,
                None
            ),
            Err(CodecError)
        );
        assert_eq!(
            op_from_columns(
                "rename_board",
                None,
                None,
                None,
                Some(&uuid_to_text(id(2))),
                None
            ),
            Err(CodecError)
        );
        // Unparseable id text counts as missing.
        assert_eq!(
            op_from_columns("create_task", Some("junk"), None, None, None, None),
            Err(CodecError)
        );
    }

    #[test]
    fn sync_phase_roundtrips_all_forms() {
        let phases = [
            SyncPhase::Idle,
            SyncPhase::Syncing,
            SyncPhase::Offline,
            SyncPhase::Failed {
                last_error: SyncErrorKind::Network,
            },
            SyncPhase::Failed {
                last_error: SyncErrorKind::LocalData,
            },
        ];
        for phase in phases {
            let last_error = match phase {
                SyncPhase::Failed { last_error } => Some(sync_error_kind_to_text(last_error)),
                _ => None,
            };
            assert_eq!(
                sync_phase_from_columns(sync_phase_to_text(phase), last_error),
                Ok(phase)
            );
        }
    }

    #[test]
    fn sync_phase_rejections() {
        // Unknown phase tag.
        assert_eq!(sync_phase_from_columns("warp", None), Err(CodecError));
        // Failed without an error kind.
        assert_eq!(sync_phase_from_columns("failed", None), Err(CodecError));
        // Unknown error kind.
        assert_eq!(
            sync_phase_from_columns("failed", Some("meteor")),
            Err(CodecError)
        );
        // Dangling error kind on a non-failed phase (every guard arm).
        assert_eq!(
            sync_phase_from_columns("idle", Some("network")),
            Err(CodecError)
        );
        assert_eq!(
            sync_phase_from_columns("syncing", Some("auth")),
            Err(CodecError)
        );
        assert_eq!(
            sync_phase_from_columns("offline", Some("forbidden")),
            Err(CodecError)
        );
    }

    #[test]
    fn validator_key_roundtrips_and_rejects_garbage() {
        assert_eq!(
            validator_key_from_text(&validator_key_to_text(&ValidatorKey::Boards)),
            Ok(ValidatorKey::Boards)
        );
        let stacks = ValidatorKey::Stacks(RemoteBoardId(9));
        assert_eq!(
            validator_key_from_text(&validator_key_to_text(&stacks)),
            Ok(stacks)
        );
        let archived = ValidatorKey::ArchivedStacks(RemoteBoardId(9));
        assert_eq!(
            validator_key_from_text(&validator_key_to_text(&archived)),
            Ok(archived)
        );
        assert_eq!(
            validator_key_to_text(&archived),
            "archived_stacks:9",
            "the text form is free-form TEXT: no migration needed"
        );
        assert_eq!(validator_key_from_text("labels"), Err(CodecError));
        assert_eq!(validator_key_from_text("stacks:x"), Err(CodecError));
        assert_eq!(
            validator_key_from_text("archived_stacks:x"),
            Err(CodecError)
        );
    }

    #[test]
    fn no_board_error_kind_roundtrips_through_the_codec() {
        let phase = SyncPhase::Failed {
            last_error: SyncErrorKind::NoBoard,
        };
        assert_eq!(
            sync_phase_from_columns(
                sync_phase_to_text(phase),
                Some(sync_error_kind_to_text(SyncErrorKind::NoBoard))
            ),
            Ok(phase)
        );
    }

    #[test]
    fn reminder_ref_text_roundtrips_and_rejects_garbage() {
        let rule = ReminderRef::Rule(taskboard_domain::ids::ReminderRuleId::from(id(7)));
        let default = ReminderRef::Default(TaskId::from(id(8)));
        let alarm = ReminderRef::Adhoc(taskboard_domain::ids::AdhocAlarmId::from(id(9)));
        for reminder in [rule, default, alarm] {
            assert_eq!(
                reminder_ref_from_text(&reminder_ref_to_text(reminder)),
                Ok(reminder)
            );
        }
        assert_eq!(reminder_ref_from_text("garbage"), Err(CodecError));
        assert_eq!(reminder_ref_from_text("rule:not-a-uuid"), Err(CodecError));
        assert_eq!(reminder_ref_from_text("alarm:not-a-uuid"), Err(CodecError));
        assert_eq!(reminder_ref_from_text("alarm:"), Err(CodecError));
        assert_eq!(reminder_ref_from_text(""), Err(CodecError));
    }

    #[test]
    fn remote_columns_all_or_none() {
        assert_eq!(remote_columns_present_or_none(&[None, None]), Ok(None));
        assert_eq!(
            remote_columns_present_or_none(&[Some(1), Some(2)]),
            Ok(Some(vec![1, 2]))
        );
        assert_eq!(
            remote_columns_present_or_none(&[Some(1), None]),
            Err(CodecError)
        );
    }

    #[test]
    fn remote_refs_decompose_into_columns() {
        let (b, s) = remote_stack_to_columns(taskboard_domain::ids::RemoteStackRef {
            board: RemoteBoardId(3),
            stack: RemoteStackId(4),
        });
        assert_eq!((b, s), (3, 4));

        let (b, s, c) = remote_card_to_columns(taskboard_domain::ids::RemoteCardRef {
            board: RemoteBoardId(3),
            stack: RemoteStackId(4),
            card: RemoteCardId(5),
        });
        assert_eq!((b, s, c), (3, 4, 5));

        let (b, l) = remote_label_to_columns(taskboard_domain::ids::RemoteLabelRef {
            board: RemoteBoardId(3),
            label: RemoteLabelId(6),
        });
        assert_eq!((b, l), (3, 6));
    }
}

#[cfg(test)]
mod recur_codec_tests {
    use super::*;

    fn sample_rule() -> taskboard_domain::recur::RecurRule {
        taskboard_domain::recur::RecurRule {
            series: taskboard_domain::SeriesId::from_uuid(uuid::Uuid::from_u128(0x5e)),
            anchor: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            pattern: taskboard_domain::recur::RecurPattern::Daily {
                every: taskboard_domain::recur::EveryDays::new(2).unwrap(),
            },
            bound: taskboard_domain::recur::RecurBound::None,
            continued_from: None,
        }
    }

    #[test]
    fn recur_text_round_trips_a_rule() {
        let text = recur_to_text(&sample_rule());
        assert_eq!(recur_from_text(&text), Some(sample_rule()));
    }

    #[test]
    fn recur_text_rejects_garbage_and_the_zero_interval() {
        assert_eq!(recur_from_text("not json"), None);
        // The strict mints: a stored every:0 cannot decode (typed
        // Corrupted at the repository, never a silently-wrong rule).
        assert_eq!(recur_from_text("{\"every\": 0}"), None);
        assert_eq!(recur_from_text(""), None);
    }
}
