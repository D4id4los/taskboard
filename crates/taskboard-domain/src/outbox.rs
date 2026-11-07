// SPDX-License-Identifier: MIT OR Apache-2.0
//! The local outbox: operations created client-side and pending push.
//!
//! Ops are id-scoped and carry no field sets: `TaskClocks` already knows
//! which fields changed, and every Deck write is full-send anyway (the
//! sparse-PUT hazard makes partial payloads useless on the wire). Op
//! *coalescing* (update+delete → delete, etc.) is Phase 2/4 actor/storage
//! logic built on these types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::{BoardId, LabelId, StackId, TaskId};
use crate::push::EntityTables;

/// Identity of a pending outbox operation (`UUIDv7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpId(pub Uuid);

/// A local user operation awaiting push to the remote board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocalOp {
    /// Card create; binds the remote ref on success (R9a).
    CreateTask(TaskId),
    /// Card update (title/description/duedate/done/archived).
    UpdateTask(TaskId),
    /// Card move (stack and/or order).
    MoveTask(TaskId),
    /// Card delete; the tombstone finalizes on success (R9c).
    DeleteTask(TaskId),
    /// Stack create.
    CreateStack(StackId),
    /// Stack rename.
    RenameStack(StackId),
    /// Stack reorder (full-send on push, like [`LocalOp::RenameStack`]).
    MoveStack(StackId),
    /// Stack delete.
    DeleteStack(StackId),
    /// Label create.
    CreateLabel(LabelId),
    /// Label edit (title/color).
    UpdateLabel(LabelId),
    /// Label delete.
    DeleteLabel(LabelId),
    /// Label assignment on a card.
    AssignLabel(TaskId, LabelId),
    /// Label removal from a card.
    UnassignLabel(TaskId, LabelId),
    /// Board edit (title/color/archived — every Deck board write is a
    /// full-send, like [`LocalOp::RenameStack`]).
    UpdateBoard(crate::ids::BoardId),
    /// Board delete; the tombstone finalizes on success (R9c).
    DeleteBoard(crate::ids::BoardId),
    /// DELETE a remote card the local side has **no binding** for — the
    /// crash-window duplicate collapse's doomed twin (#150, rule R-dup).
    /// `board` is the local attribution row; `card` is the wire target.
    /// Completes under `Applied` and under `RemoteMissing` (gone either
    /// way) with no tombstone side effect — there is no local row.
    DeleteRemoteCard {
        /// The local board the collapse ran under.
        board: crate::ids::BoardId,
        /// The doomed card's remote ref.
        card: crate::ids::RemoteCardRef,
    },
    /// Stack twin of [`LocalOp::DeleteRemoteCard`]: the unbound orphan
    /// stack the collapse deletes (the bound survivor keeps the cards).
    DeleteRemoteStack {
        /// The local board the collapse ran under.
        board: crate::ids::BoardId,
        /// The doomed stack's remote ref.
        stack: crate::ids::RemoteStackRef,
    },
    /// App-data document content for a task is stale: app fields,
    /// reminder rules, or a time entry touched. Full-send at flush;
    /// coalesces per board into the single sidecar document upload
    /// (D-S5). Every task-touching planner verb (task verbs, reminder
    /// verbs, the tracking verbs) and the done/deleted close
    /// side-effects enqueue it for each touched task; the flush lane
    /// consumes the board's coalesced ops.
    UpdateAppData(TaskId),
    /// Flush one board's sidecar document (D-H9): habit writes have no
    /// task to key an [`LocalOp::UpdateAppData`] op — this is the
    /// board-level lane's trigger, carried board attribution like the
    /// ref-deletes. The flush uploads the whole merged document, so a
    /// stale or duplicated trigger is harmless bookkeeping, never a
    /// delta; coalesces with any `UpdateAppData` ops of the same board
    /// into the ONE flush group.
    FlushBoardAppData(crate::ids::BoardId),
}

/// The data-less descriptor of a [`LocalOp`] variant: the one home of
/// the `snake_case` storage tag vocabulary (`outbox.op_kind`), shared by
/// the storage codec's encode and decode sides. Distinct from serde:
/// the [`LocalOp`] derive carries no `rename_all`, so the serde wire
/// tags (`PascalCase`) stay untouched — the storage tag is defined once
/// here, serde stays serde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocalOpTag {
    /// Tag of [`LocalOp::CreateTask`].
    CreateTask,
    /// Tag of [`LocalOp::UpdateTask`].
    UpdateTask,
    /// Tag of [`LocalOp::MoveTask`].
    MoveTask,
    /// Tag of [`LocalOp::DeleteTask`].
    DeleteTask,
    /// Tag of [`LocalOp::CreateStack`].
    CreateStack,
    /// Tag of [`LocalOp::RenameStack`].
    RenameStack,
    /// Tag of [`LocalOp::MoveStack`].
    MoveStack,
    /// Tag of [`LocalOp::DeleteStack`].
    DeleteStack,
    /// Tag of [`LocalOp::CreateLabel`].
    CreateLabel,
    /// Tag of [`LocalOp::UpdateLabel`].
    UpdateLabel,
    /// Tag of [`LocalOp::DeleteLabel`].
    DeleteLabel,
    /// Tag of [`LocalOp::AssignLabel`].
    AssignLabel,
    /// Tag of [`LocalOp::UnassignLabel`].
    UnassignLabel,
    /// Tag of [`LocalOp::UpdateBoard`].
    UpdateBoard,
    /// Tag of [`LocalOp::DeleteBoard`].
    DeleteBoard,
    /// Tag of [`LocalOp::DeleteRemoteCard`].
    DeleteRemoteCard,
    /// Tag of [`LocalOp::DeleteRemoteStack`].
    DeleteRemoteStack,
    /// Tag of [`LocalOp::UpdateAppData`].
    UpdateAppData,
    /// Tag of [`LocalOp::FlushBoardAppData`].
    FlushBoardAppData,
}

impl LocalOpTag {
    /// The `snake_case` tag stored in `outbox.op_kind` — the single home
    /// of these literals.
    #[must_use]
    pub const fn text_form(self) -> &'static str {
        match self {
            Self::CreateTask => "create_task",
            Self::UpdateTask => "update_task",
            Self::MoveTask => "move_task",
            Self::DeleteTask => "delete_task",
            Self::CreateStack => "create_stack",
            Self::RenameStack => "rename_stack",
            Self::MoveStack => "move_stack",
            Self::DeleteStack => "delete_stack",
            Self::CreateLabel => "create_label",
            Self::UpdateLabel => "update_label",
            Self::DeleteLabel => "delete_label",
            Self::AssignLabel => "assign_label",
            Self::UnassignLabel => "unassign_label",
            Self::UpdateBoard => "update_board",
            Self::DeleteBoard => "delete_board",
            Self::DeleteRemoteCard => "delete_remote_card",
            Self::DeleteRemoteStack => "delete_remote_stack",
            Self::UpdateAppData => "update_app_data",
            Self::FlushBoardAppData => "flush_board_app_data",
        }
    }

    /// Parses [`text_form`](Self::text_form) back; `None` on an unknown
    /// tag (the legacy `rename_board` included — one representation, no
    /// alias drift).
    #[must_use]
    pub fn parse_text_form(raw: &str) -> Option<Self> {
        match raw {
            "create_task" => Some(Self::CreateTask),
            "update_task" => Some(Self::UpdateTask),
            "move_task" => Some(Self::MoveTask),
            "delete_task" => Some(Self::DeleteTask),
            "create_stack" => Some(Self::CreateStack),
            "rename_stack" => Some(Self::RenameStack),
            "move_stack" => Some(Self::MoveStack),
            "delete_stack" => Some(Self::DeleteStack),
            "create_label" => Some(Self::CreateLabel),
            "update_label" => Some(Self::UpdateLabel),
            "delete_label" => Some(Self::DeleteLabel),
            "assign_label" => Some(Self::AssignLabel),
            "unassign_label" => Some(Self::UnassignLabel),
            "update_board" => Some(Self::UpdateBoard),
            "delete_board" => Some(Self::DeleteBoard),
            "delete_remote_card" => Some(Self::DeleteRemoteCard),
            "delete_remote_stack" => Some(Self::DeleteRemoteStack),
            "update_app_data" => Some(Self::UpdateAppData),
            "flush_board_app_data" => Some(Self::FlushBoardAppData),
            _ => None,
        }
    }
}

/// An outbox entry: which operation, when it was queued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingOp {
    /// Unique operation id (used by push outcomes to address the op).
    pub op_id: OpId,
    /// The operation itself.
    pub op: LocalOp,
    /// When the operation entered the outbox.
    pub queued_at: DateTime<Utc>,
}

impl LocalOp {
    /// The variant's [`LocalOpTag`] descriptor. An exhaustive match: a
    /// future op variant fails to compile here until it is tagged, so
    /// the vocabulary cannot silently drift from the enum.
    #[must_use]
    pub const fn tag(&self) -> LocalOpTag {
        match self {
            Self::CreateTask(_) => LocalOpTag::CreateTask,
            Self::UpdateTask(_) => LocalOpTag::UpdateTask,
            Self::MoveTask(_) => LocalOpTag::MoveTask,
            Self::DeleteTask(_) => LocalOpTag::DeleteTask,
            Self::CreateStack(_) => LocalOpTag::CreateStack,
            Self::RenameStack(_) => LocalOpTag::RenameStack,
            Self::MoveStack(_) => LocalOpTag::MoveStack,
            Self::DeleteStack(_) => LocalOpTag::DeleteStack,
            Self::CreateLabel(_) => LocalOpTag::CreateLabel,
            Self::UpdateLabel(_) => LocalOpTag::UpdateLabel,
            Self::DeleteLabel(_) => LocalOpTag::DeleteLabel,
            Self::AssignLabel(_, _) => LocalOpTag::AssignLabel,
            Self::UnassignLabel(_, _) => LocalOpTag::UnassignLabel,
            Self::UpdateBoard(_) => LocalOpTag::UpdateBoard,
            Self::DeleteBoard(_) => LocalOpTag::DeleteBoard,
            Self::DeleteRemoteCard { .. } => LocalOpTag::DeleteRemoteCard,
            Self::DeleteRemoteStack { .. } => LocalOpTag::DeleteRemoteStack,
            Self::UpdateAppData(_) => LocalOpTag::UpdateAppData,
            Self::FlushBoardAppData(_) => LocalOpTag::FlushBoardAppData,
        }
    }

    /// The board that owns this op's entity (board ops: themselves;
    /// stack ops: the stack's board; label ops: the label's board;
    /// task ops: the task's stack's board). `None` when the owning
    /// entity row no longer exists (clear-board content wipes).
    #[must_use]
    pub fn owning_board(&self, entities: &EntityTables<'_>) -> Option<BoardId> {
        match *self {
            // Board ops, ref-deletes (#150), and the task-less flush
            // trigger attribute exactly: they carry their board.
            Self::UpdateBoard(board)
            | Self::DeleteBoard(board)
            | Self::DeleteRemoteCard { board, .. }
            | Self::DeleteRemoteStack { board, .. }
            | Self::FlushBoardAppData(board) => Some(board),
            Self::CreateStack(stack)
            | Self::RenameStack(stack)
            | Self::MoveStack(stack)
            | Self::DeleteStack(stack) => entities.stacks.get(&stack).map(|s| s.board),
            Self::CreateLabel(label) | Self::UpdateLabel(label) | Self::DeleteLabel(label) => {
                entities.labels.get(&label).map(|l| l.board)
            }
            Self::CreateTask(task)
            | Self::UpdateTask(task)
            | Self::MoveTask(task)
            | Self::DeleteTask(task)
            | Self::AssignLabel(task, _)
            | Self::UnassignLabel(task, _)
            | Self::UpdateAppData(task) => entities
                .tasks
                .get(&task)
                .and_then(|t| entities.stacks.get(&t.stack))
                .map(|s| s.board),
        }
    }
}

/// `true` when the pending op addresses the given task: the id-scoping
/// rule for op cancellation, shared by the command layer and the sync
/// pipeline so the two can never drift.
#[must_use]
pub fn op_targets_task(op: &LocalOp, task: TaskId) -> bool {
    match op {
        LocalOp::CreateTask(id)
        | LocalOp::UpdateTask(id)
        | LocalOp::MoveTask(id)
        | LocalOp::DeleteTask(id)
        | LocalOp::AssignLabel(id, _)
        | LocalOp::UnassignLabel(id, _)
        | LocalOp::UpdateAppData(id) => *id == task,
        LocalOp::CreateStack(_)
        | LocalOp::RenameStack(_)
        | LocalOp::MoveStack(_)
        | LocalOp::DeleteStack(_)
        | LocalOp::CreateLabel(_)
        | LocalOp::UpdateLabel(_)
        | LocalOp::DeleteLabel(_)
        | LocalOp::UpdateBoard(_)
        | LocalOp::DeleteBoard(_)
        // Ref-deletes target no local task by construction.
        | LocalOp::DeleteRemoteCard { .. }
        | LocalOp::DeleteRemoteStack { .. }
        // The flush trigger is board-scoped: no task is addressed.
        | LocalOp::FlushBoardAppData(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{Board, Label, Stack, Task};
    use crate::push::EntityTables;
    use chrono::TimeZone;
    use std::collections::BTreeMap;

    #[test]
    fn every_op_variant_tags_to_the_pinned_storage_form() {
        let task = TaskId::from(uuid::Uuid::from_u128(1));
        let stack = StackId::from(uuid::Uuid::from_u128(2));
        let label = LabelId::from(uuid::Uuid::from_u128(3));
        let board = BoardId::from(uuid::Uuid::from_u128(9));
        let ops = [
            (LocalOp::CreateTask(task), "create_task"),
            (LocalOp::UpdateTask(task), "update_task"),
            (LocalOp::MoveTask(task), "move_task"),
            (LocalOp::DeleteTask(task), "delete_task"),
            (LocalOp::CreateStack(stack), "create_stack"),
            (LocalOp::RenameStack(stack), "rename_stack"),
            (LocalOp::MoveStack(stack), "move_stack"),
            (LocalOp::DeleteStack(stack), "delete_stack"),
            (LocalOp::CreateLabel(label), "create_label"),
            (LocalOp::UpdateLabel(label), "update_label"),
            (LocalOp::DeleteLabel(label), "delete_label"),
            (LocalOp::AssignLabel(task, label), "assign_label"),
            (LocalOp::UnassignLabel(task, label), "unassign_label"),
            (LocalOp::UpdateBoard(board), "update_board"),
            (LocalOp::DeleteBoard(board), "delete_board"),
            (LocalOp::UpdateAppData(task), "update_app_data"),
            (LocalOp::FlushBoardAppData(board), "flush_board_app_data"),
            (
                LocalOp::DeleteRemoteCard {
                    board,
                    card: crate::ids::RemoteCardRef {
                        board: crate::ids::RemoteBoardId(42),
                        stack: crate::ids::RemoteStackId(7),
                        card: crate::ids::RemoteCardId(517),
                    },
                },
                "delete_remote_card",
            ),
            (
                LocalOp::DeleteRemoteStack {
                    board,
                    stack: crate::ids::RemoteStackRef {
                        board: crate::ids::RemoteBoardId(42),
                        stack: crate::ids::RemoteStackId(8),
                    },
                },
                "delete_remote_stack",
            ),
        ];
        for (op, tag) in ops {
            assert_eq!(op.tag().text_form(), tag, "{op:?}");
            assert_eq!(LocalOpTag::parse_text_form(tag), Some(op.tag()));
        }
        // The legacy `rename_board` tag is unknown after the 0005 tag
        // rename — a rejection, not a silent alias.
        assert_eq!(LocalOpTag::parse_text_form("rename_board"), None);
        assert_eq!(LocalOpTag::parse_text_form(""), None);
    }

    fn stack_clocks() -> crate::entities::StackClocks {
        let t = ts(100);
        crate::entities::StackClocks {
            title: t,
            order: t,
            deleted: t,
        }
    }

    fn label_clocks() -> crate::entities::LabelClocks {
        let t = ts(100);
        crate::entities::LabelClocks {
            title: t,
            color: t,
            deleted: t,
        }
    }

    fn task_clocks() -> crate::entities::TaskClocks {
        let t = ts(100);
        crate::entities::TaskClocks {
            title: t,
            description: t,
            duedate: t,
            scheduled: t,
            done: t,
            position: t,
            effort: t,
            importance: t,
            urgency: t,
            recur: t,
            kind: t,

            archived: t,
            deleted: t,
        }
    }

    fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn stack_row(raw: u128, board: BoardId, deleted: bool) -> Stack {
        Stack {
            id: StackId::from(Uuid::from_u128(raw)),
            remote: None,
            board,
            title: "stack".into(),
            order: 0,
            archived: false,
            deleted,
            clocks: stack_clocks(),
            authors: crate::entities::StackAuthors::default(),
            remote_seen: None,
        }
    }

    fn label_row(raw: u128, board: BoardId, deleted: bool) -> Label {
        Label {
            id: LabelId::from(Uuid::from_u128(raw)),
            remote: None,
            board,
            title: "label".into(),
            color: crate::entities::Color::new("00ff00"),
            deleted,
            clocks: label_clocks(),
            authors: crate::entities::LabelAuthors::default(),
            remote_seen: None,
        }
    }

    fn task_row(raw: u128, stack: StackId, deleted: bool) -> Task {
        Task {
            id: TaskId::from(Uuid::from_u128(raw)),
            remote: None,
            title: "task".into(),
            description: String::new(),
            duedate: None,
            done: None,
            scheduled: None,
            stack,
            order: 0,
            label_memberships: std::collections::BTreeMap::default(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: crate::entities::TaskKind::Standard,

            archived: false,
            deleted,
            clocks: task_clocks(),
            authors: crate::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn tables<'a>(
        tasks: &'a BTreeMap<TaskId, Task>,
        stacks: &'a BTreeMap<StackId, Stack>,
        labels: &'a BTreeMap<LabelId, Label>,
        boards: &'a BTreeMap<BoardId, Board>,
    ) -> EntityTables<'a> {
        EntityTables {
            tasks,
            stacks,
            labels,
            boards,
        }
    }

    #[test]
    fn board_ops_own_their_board_even_without_a_row() {
        let empty: BTreeMap<TaskId, Task> = BTreeMap::new();
        let stacks: BTreeMap<StackId, Stack> = BTreeMap::new();
        let labels: BTreeMap<LabelId, Label> = BTreeMap::new();
        let boards: BTreeMap<BoardId, Board> = BTreeMap::new();
        let t = tables(&empty, &stacks, &labels, &boards);
        let board = BoardId::from(Uuid::from_u128(9));
        assert_eq!(
            LocalOp::UpdateBoard(board).owning_board(&t),
            Some(board),
            "board ops resolve themselves"
        );
        assert_eq!(LocalOp::DeleteBoard(board).owning_board(&t), Some(board));
    }

    #[test]
    fn stack_ops_resolve_through_the_stack_row() {
        let board = BoardId::from(Uuid::from_u128(1));
        let stack = StackId::from(Uuid::from_u128(2));
        let tombstoned = StackId::from(Uuid::from_u128(3));
        let missing = StackId::from(Uuid::from_u128(4));
        let mut stacks = BTreeMap::new();
        stacks.insert(stack, stack_row(2, board, false));
        stacks.insert(tombstoned, stack_row(3, board, true));
        let empty: BTreeMap<TaskId, Task> = BTreeMap::new();
        let labels: BTreeMap<LabelId, Label> = BTreeMap::new();
        let boards: BTreeMap<BoardId, Board> = BTreeMap::new();
        let t = tables(&empty, &stacks, &labels, &boards);
        assert_eq!(LocalOp::RenameStack(stack).owning_board(&t), Some(board));
        assert_eq!(
            LocalOp::DeleteStack(tombstoned).owning_board(&t),
            Some(board),
            "a tombstoned row still resolves (rows survive deletion)"
        );
        assert_eq!(
            LocalOp::CreateStack(missing).owning_board(&t),
            None,
            "a missing row has no owner"
        );
    }

    #[test]
    fn label_ops_resolve_through_the_label_row() {
        let board = BoardId::from(Uuid::from_u128(1));
        let label = LabelId::from(Uuid::from_u128(2));
        let tombstoned = LabelId::from(Uuid::from_u128(3));
        let missing = LabelId::from(Uuid::from_u128(4));
        let mut labels = BTreeMap::new();
        labels.insert(label, label_row(2, board, false));
        labels.insert(tombstoned, label_row(3, board, true));
        let empty: BTreeMap<TaskId, Task> = BTreeMap::new();
        let stacks: BTreeMap<StackId, Stack> = BTreeMap::new();
        let boards: BTreeMap<BoardId, Board> = BTreeMap::new();
        let t = tables(&empty, &stacks, &labels, &boards);
        assert_eq!(LocalOp::UpdateLabel(label).owning_board(&t), Some(board));
        assert_eq!(
            LocalOp::DeleteLabel(tombstoned).owning_board(&t),
            Some(board)
        );
        assert_eq!(LocalOp::CreateLabel(missing).owning_board(&t), None);
    }

    #[test]
    fn task_ops_resolve_through_the_task_stack_chain() {
        let board = BoardId::from(Uuid::from_u128(1));
        let stack = StackId::from(Uuid::from_u128(2));
        let task = TaskId::from(Uuid::from_u128(3));
        let tombstoned = TaskId::from(Uuid::from_u128(4));
        let stackless = TaskId::from(Uuid::from_u128(5));
        let missing = TaskId::from(Uuid::from_u128(6));
        let mut stacks = BTreeMap::new();
        stacks.insert(stack, stack_row(2, board, false));
        let mut tasks = BTreeMap::new();
        tasks.insert(task, task_row(3, stack, false));
        tasks.insert(tombstoned, task_row(4, stack, true));
        tasks.insert(
            stackless,
            task_row(5, StackId::from(Uuid::from_u128(99)), false),
        );
        let empty: BTreeMap<LabelId, Label> = BTreeMap::new();
        let boards: BTreeMap<BoardId, Board> = BTreeMap::new();
        let t = tables(&tasks, &stacks, &empty, &boards);
        assert_eq!(LocalOp::UpdateTask(task).owning_board(&t), Some(board));
        assert_eq!(
            LocalOp::DeleteTask(tombstoned).owning_board(&t),
            Some(board),
            "a tombstoned task still resolves (its delete push drains)"
        );
        assert_eq!(
            LocalOp::AssignLabel(stackless, LabelId::from(Uuid::from_u128(7))).owning_board(&t),
            None,
            "a task whose stack row is gone has no owner"
        );
        assert_eq!(LocalOp::MoveTask(missing).owning_board(&t), None);
    }

    #[test]
    fn the_flush_trigger_owns_its_board_and_targets_no_task() {
        let board = BoardId::from(Uuid::from_u128(1));
        let empty: BTreeMap<TaskId, Task> = BTreeMap::new();
        let stacks: BTreeMap<StackId, Stack> = BTreeMap::new();
        let labels: BTreeMap<LabelId, Label> = BTreeMap::new();
        let boards: BTreeMap<BoardId, Board> = BTreeMap::new();
        let t = tables(&empty, &stacks, &labels, &boards);
        let op = LocalOp::FlushBoardAppData(board);
        // No entity graph: the board rides the op itself.
        assert_eq!(op.owning_board(&t), Some(board));
        assert!(!op_targets_task(&op, TaskId::from(Uuid::from_u128(9))));
    }

    #[test]
    fn pending_op_roundtrips_through_serde() {
        let op = PendingOp {
            // Fixed from_u128 ids and stamp: the subject is serde symmetry
            // (version-agnostic representation); real v7 generation stays
            // covered by idgen's tests.
            op_id: OpId(Uuid::from_u128(1)),
            op: LocalOp::AssignLabel(
                TaskId::from(Uuid::from_u128(2)),
                LabelId::from(Uuid::from_u128(3)),
            ),
            queued_at: Utc.timestamp_opt(1_700_100_011, 0).unwrap(),
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: PendingOp = serde_json::from_str(&json).unwrap();
        assert_eq!(back, op);
    }

    #[test]
    fn ref_delete_ops_own_their_board_and_target_no_task() {
        let board = BoardId::from(Uuid::from_u128(1));
        let card_ref = crate::ids::RemoteCardRef {
            board: crate::ids::RemoteBoardId(42),
            stack: crate::ids::RemoteStackId(7),
            card: crate::ids::RemoteCardId(517),
        };
        let stack_ref = crate::ids::RemoteStackRef {
            board: crate::ids::RemoteBoardId(42),
            stack: crate::ids::RemoteStackId(8),
        };
        let empty: BTreeMap<TaskId, Task> = BTreeMap::new();
        let stacks: BTreeMap<StackId, Stack> = BTreeMap::new();
        let labels: BTreeMap<LabelId, Label> = BTreeMap::new();
        let boards: BTreeMap<BoardId, Board> = BTreeMap::new();
        let t = tables(&empty, &stacks, &labels, &boards);
        let card_op = LocalOp::DeleteRemoteCard {
            board,
            card: card_ref,
        };
        let stack_op = LocalOp::DeleteRemoteStack {
            board,
            stack: stack_ref,
        };
        assert_eq!(card_op.owning_board(&t), Some(board));
        assert_eq!(stack_op.owning_board(&t), Some(board));
        assert!(!op_targets_task(&card_op, TaskId::from(Uuid::from_u128(9))));
        assert!(!op_targets_task(
            &stack_op,
            TaskId::from(Uuid::from_u128(9))
        ));
    }

    #[test]
    fn ref_delete_pending_ops_roundtrip_through_serde() {
        let card_ref = crate::ids::RemoteCardRef {
            board: crate::ids::RemoteBoardId(42),
            stack: crate::ids::RemoteStackId(7),
            card: crate::ids::RemoteCardId(517),
        };
        for op in [
            LocalOp::DeleteRemoteCard {
                board: BoardId::from(Uuid::from_u128(1)),
                card: card_ref,
            },
            LocalOp::DeleteRemoteStack {
                board: BoardId::from(Uuid::from_u128(1)),
                stack: crate::ids::RemoteStackRef {
                    board: crate::ids::RemoteBoardId(42),
                    stack: crate::ids::RemoteStackId(8),
                },
            },
        ] {
            let pending = PendingOp {
                op_id: OpId(Uuid::from_u128(2)),
                op,
                queued_at: Utc.timestamp_opt(1_700_100_011, 0).unwrap(),
            };
            let json = serde_json::to_string(&pending).unwrap();
            let back: PendingOp = serde_json::from_str(&json).unwrap();
            assert_eq!(back, pending);
        }
    }
}
