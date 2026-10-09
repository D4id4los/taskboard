// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hand-written `TEXT` codecs between domain tag types and their stored
//! string forms (storage plan §3.10).
//!
//! The forms mirror the domain's serde wire shapes (`"boards"`,
//! `"stacks:<u64>"`, snake_case-style op tags) so the database stays inspectable
//! without pulling serde into this crate. Unknown tags or missing id
//! columns decode to [`Corrupted`](taskboard_domain::persistence::RepositoryError)
//! via [`CodecError`].

use taskboard_domain::ids::{
    LabelId, RemoteBoardId, RemoteCardId, RemoteLabelId, RemoteStackId, StackId, TaskId,
};
use taskboard_domain::outbox::LocalOp;
use taskboard_domain::persistence::ValidatorKey;
use taskboard_domain::state::{SyncErrorKind, SyncPhase};
use uuid::Uuid;

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

/// `u64` text form of a remote id.
#[must_use]
pub fn remote_id_to_text(id: u64) -> String {
    id.to_string()
}

/// Parses a remote id.
///
/// # Errors
///
/// [`CodecError`] when the text is not a valid `u64`.
pub fn remote_id_from_text(raw: &str) -> Result<u64, CodecError> {
    raw.parse().map_err(|_| CodecError)
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
        LocalOp::DeleteStack(_) => "delete_stack",
        LocalOp::CreateLabel(_) => "create_label",
        LocalOp::UpdateLabel(_) => "update_label",
        LocalOp::DeleteLabel(_) => "delete_label",
        LocalOp::AssignLabel(_, _) => "assign_label",
        LocalOp::UnassignLabel(_, _) => "unassign_label",
    }
}

/// Which id columns an op kind requires in the `outbox` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_field_names)] // the *_id names mirror the SQL columns
pub(crate) struct OpColumns {
    pub(crate) task_id: Option<TaskId>,
    pub(crate) stack_id: Option<StackId>,
    pub(crate) label_id: Option<LabelId>,
}

/// Decomposes an op into the id columns it addresses (storage plan §4.1:
/// `assign_label`/`unassign_label` need both task and label).
#[must_use]
pub(crate) fn op_to_columns(op: &LocalOp) -> OpColumns {
    match *op {
        LocalOp::CreateTask(task)
        | LocalOp::UpdateTask(task)
        | LocalOp::MoveTask(task)
        | LocalOp::DeleteTask(task) => OpColumns {
            task_id: Some(task),
            stack_id: None,
            label_id: None,
        },
        LocalOp::CreateStack(stack) | LocalOp::RenameStack(stack) | LocalOp::DeleteStack(stack) => {
            OpColumns {
                task_id: None,
                stack_id: Some(stack),
                label_id: None,
            }
        }
        LocalOp::CreateLabel(label) | LocalOp::UpdateLabel(label) | LocalOp::DeleteLabel(label) => {
            OpColumns {
                task_id: None,
                stack_id: None,
                label_id: Some(label),
            }
        }
        LocalOp::AssignLabel(task, label) | LocalOp::UnassignLabel(task, label) => OpColumns {
            task_id: Some(task),
            stack_id: None,
            label_id: Some(label),
        },
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
        _ => Err(CodecError),
    }
}

/// The lowercase tag stored in `sync_status.phase`.
#[must_use]
pub fn sync_phase_to_text(phase: &SyncPhase) -> &'static str {
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
    raw.strip_prefix("stacks:")
        .and_then(|num| num.parse::<u64>().ok())
        .map(|num| ValidatorKey::Stacks(RemoteBoardId(num)))
        .ok_or(CodecError)
}

/// Encodes a remote stack binding into its (`remote_board_id`,
/// `remote_stack_id`) columns.
#[must_use]
pub fn remote_stack_to_columns(r: taskboard_domain::ids::RemoteStackRef) -> (i64, i64) {
    (board_col(r.board), stack_col(r.stack))
}

/// Encodes a remote card binding into its (`remote_board_id`,
/// `remote_stack_id`, `remote_card_id`) columns.
#[must_use]
pub fn remote_card_to_columns(r: taskboard_domain::ids::RemoteCardRef) -> (i64, i64, i64) {
    (board_col(r.board), stack_col(r.stack), card_col(r.card))
}

/// Encodes a remote label binding into its (`remote_board_id`,
/// `remote_label_id`) columns.
#[must_use]
pub fn remote_label_to_columns(r: taskboard_domain::ids::RemoteLabelRef) -> (i64, i64) {
    (board_col(r.board), label_col(r.label))
}

/// The all-or-none NULL invariant of remote ref columns, checked at decode
/// time: `Some` columns present ⇔ `Some` ref (plan decision 11). Stored
/// values are non-negative `u64`s; a negative column is corruption.
///
/// # Errors
///
/// [`CodecError`] on a partial column set or a negative stored id.
pub(crate) fn remote_columns_present_or_none(
    ids: &[Option<i64>],
) -> Result<Option<Vec<u64>>, CodecError> {
    let all: Option<Vec<Option<u64>>> = ids
        .iter()
        .copied()
        .map(|col| match col {
            // Negative ids cannot come from a u64 remote id — corruption.
            Some(num) => u64::try_from(num).ok().map(Some),
            None => Some(None),
        })
        .collect();
    let all = all.ok_or(CodecError)?;
    if all.iter().all(Option::is_none) {
        return Ok(None);
    }
    // A partial set (some columns set, others NULL) is corruption.
    all.into_iter()
        .collect::<Option<Vec<_>>>()
        .map(Some)
        .ok_or(CodecError)
}

fn board_col(id: RemoteBoardId) -> i64 {
    i64::try_from(id.get()).unwrap_or(i64::MAX)
}

fn stack_col(id: RemoteStackId) -> i64 {
    i64::try_from(id.get()).unwrap_or(i64::MAX)
}

fn card_col(id: RemoteCardId) -> i64 {
    i64::try_from(id.get()).unwrap_or(i64::MAX)
}

fn label_col(id: RemoteLabelId) -> i64 {
    i64::try_from(id.get()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskboard_domain::ids::RemoteLabelId;

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
    fn remote_id_text_roundtrips_and_rejects_garbage() {
        assert_eq!(remote_id_from_text(&remote_id_to_text(7)), Ok(7));
        assert_eq!(remote_id_from_text("x"), Err(CodecError));
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
            LocalOp::DeleteStack(stack),
            LocalOp::CreateLabel(label),
            LocalOp::UpdateLabel(label),
            LocalOp::DeleteLabel(label),
            LocalOp::AssignLabel(task, label),
            LocalOp::UnassignLabel(task, label),
        ];
        for op in ops {
            let cols = op_to_columns(&op);
            let task_text = cols.task_id.map(|t| uuid_to_text(t.as_uuid()));
            let stack_text = cols.stack_id.map(|s| uuid_to_text(s.as_uuid()));
            let label_text = cols.label_id.map(|l| uuid_to_text(l.as_uuid()));
            let back = op_from_columns(
                op_kind_to_text(&op),
                task_text.as_deref(),
                stack_text.as_deref(),
                label_text.as_deref(),
            );
            assert_eq!(back, Ok(op), "{op:?} must round-trip through columns");
        }
    }

    #[test]
    fn unknown_op_tag_is_rejected() {
        assert_eq!(op_from_columns("nope", None, None, None), Err(CodecError));
    }

    #[test]
    fn missing_required_op_column_is_rejected() {
        // Task ops need task_id.
        assert_eq!(
            op_from_columns("create_task", None, None, None),
            Err(CodecError)
        );
        // assign_label needs both columns.
        assert_eq!(
            op_from_columns("assign_label", Some(&uuid_to_text(id(1))), None, None),
            Err(CodecError)
        );
        // Unparseable id text counts as missing.
        assert_eq!(
            op_from_columns("create_task", Some("junk"), None, None),
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
                sync_phase_from_columns(sync_phase_to_text(&phase), last_error),
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
        // Dangling error kind on a non-failed phase.
        assert_eq!(
            sync_phase_from_columns("idle", Some("network")),
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
        assert_eq!(validator_key_from_text("labels"), Err(CodecError));
        assert_eq!(validator_key_from_text("stacks:x"), Err(CodecError));
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
