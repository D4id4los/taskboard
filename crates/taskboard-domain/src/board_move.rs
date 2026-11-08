// SPDX-License-Identifier: MIT OR Apache-2.0
//! Cross-device adoption of a board moved from Deck to the files tier
//! (D-BF11). Deck boards mint their ids per device while a files board
//! is keyed by the mover's ids, so a returning device re-keys its copy
//! through the mover's published mapping instead of binding beside it.

use std::collections::{BTreeMap, HashMap};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::ids::{
    BoardId, ConnectorId, LabelId, RemoteBoardBinding, RemoteBoardId, RemoteCardId, RemoteLabelId,
    RemoteStackId, StackId, TaskId,
};
use crate::outbox::{LocalOp, PendingOp};
use crate::persistence::{PersistedState, PersistenceAction};
use crate::push::EntityTables;

/// The sidecar envelope key carrying [`MovedToFilesMarker`].
pub const MOVED_TO_FILES_KEY: &str = "moved_to_files";

/// Left in a moved Deck board's sidecar so a device with no files tier
/// learns the board went away instead of syncing a frozen copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovedToFilesMarker {
    pub files_board: Uuid,
    pub committed_at: chrono::DateTime<chrono::Utc>,
}

impl crate::appdata::AppDataDocument {
    /// The document's move marker; a malformed one reads as absent, as
    /// any unknown envelope key does.
    #[must_use]
    pub fn moved_to_files(&self) -> Option<MovedToFilesMarker> {
        serde_json::from_value(self.unknown.get(MOVED_TO_FILES_KEY)?.clone()).ok()
    }
}

/// The mover's mapping from Deck identities to published UUIDs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardAdoption {
    /// The files identity, which is also the board's id after adoption.
    pub files_board: Uuid,
    pub deck_board: u64,
    pub stacks: BTreeMap<RemoteStackId, Uuid>,
    pub cards: BTreeMap<RemoteCardId, Uuid>,
    pub labels: BTreeMap<RemoteLabelId, Uuid>,
}

/// Why a device cannot adopt; its Deck binding is left intact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AdoptionRefusal {
    /// The board is no longer bound to the moved Deck board.
    #[error("the board is not bound to the moved Deck board")]
    NotBoundToMovedDeck,
    /// A published UUID already names a local row outside this board,
    /// such as a duplicate minted by a build that ignores the marker.
    #[error("a published id already exists locally")]
    Collision,
}

/// The one storage write of an adoption: every id rewrite, the binding
/// swap, and the clearing of the Deck refs land together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardRekey {
    pub old_board: BoardId,
    pub new_board: BoardId,
    pub binding: RemoteBoardBinding,
    pub stacks: BTreeMap<StackId, StackId>,
    pub tasks: BTreeMap<TaskId, TaskId>,
    pub labels: BTreeMap<LabelId, LabelId>,
}

impl BoardRekey {
    /// Every id rewrite as `(old, new)` UUID pairs.
    #[must_use]
    pub fn pairs(&self) -> Vec<(Uuid, Uuid)> {
        std::iter::once((self.old_board.as_uuid(), self.new_board.as_uuid()))
            .chain(self.stacks.iter().map(|(a, b)| (a.as_uuid(), b.as_uuid())))
            .chain(self.tasks.iter().map(|(a, b)| (a.as_uuid(), b.as_uuid())))
            .chain(self.labels.iter().map(|(a, b)| (a.as_uuid(), b.as_uuid())))
            .filter(|(old, new)| old != new)
            .collect()
    }
}

/// Plans a device's adoption of a moved board.
///
/// # Errors
///
/// [`AdoptionRefusal`], with nothing planned.
pub fn plan_board_adoption(
    tables: &EntityTables<'_>,
    outbox: &[PendingOp],
    local: BoardId,
    adoption: &BoardAdoption,
    connector: &ConnectorId,
) -> Result<Vec<PersistenceAction>, AdoptionRefusal> {
    let deck = RemoteBoardId::deck(adoption.deck_board);
    if tables
        .boards
        .get(&local)
        .and_then(|board| board.remote.as_ref())
        .is_none_or(|binding| binding.board != deck)
    {
        return Err(AdoptionRefusal::NotBoundToMovedDeck);
    }
    let new_board = BoardId::from_uuid(adoption.files_board);
    if new_board != local && tables.boards.contains_key(&new_board) {
        return Err(AdoptionRefusal::Collision);
    }

    let stacks: BTreeMap<StackId, StackId> = tables
        .stacks
        .values()
        .filter(|stack| stack.board == local)
        .filter_map(|stack| {
            let target = adoption.stacks.get(&stack.remote?.stack)?;
            Some((stack.id, StackId::from_uuid(*target)))
        })
        .collect();
    let tasks: BTreeMap<TaskId, TaskId> = tables
        .tasks
        .values()
        .filter(|task| {
            tables
                .stacks
                .get(&task.stack)
                .is_some_and(|stack| stack.board == local)
        })
        .filter_map(|task| {
            let target = adoption.cards.get(&task.remote?.card)?;
            Some((task.id, TaskId::from_uuid(*target)))
        })
        .collect();
    let labels: BTreeMap<LabelId, LabelId> = tables
        .labels
        .values()
        .filter(|label| label.board == local)
        .filter_map(|label| {
            let target = adoption.labels.get(&label.remote?.label)?;
            Some((label.id, LabelId::from_uuid(*target)))
        })
        .collect();

    let collides = stacks
        .iter()
        .any(|(old, new)| old != new && tables.stacks.contains_key(new))
        || tasks
            .iter()
            .any(|(old, new)| old != new && tables.tasks.contains_key(new))
        || labels
            .iter()
            .any(|(old, new)| old != new && tables.labels.contains_key(new));
    if collides {
        return Err(AdoptionRefusal::Collision);
    }

    let mut actions = vec![PersistenceAction::AdoptMovedBoard(Box::new(BoardRekey {
        old_board: local,
        new_board,
        binding: RemoteBoardBinding {
            connector: connector.clone(),
            board: RemoteBoardId::files(adoption.files_board),
        },
        stacks,
        tasks,
        labels,
    }))];
    // A Deck ref-delete has no files equivalent and would stay queued
    // forever; the worst outcome of dropping it is a visible duplicate.
    actions.extend(
        outbox
            .iter()
            .filter(|pending| match pending.op {
                LocalOp::DeleteRemoteCard { board, .. }
                | LocalOp::DeleteRemoteStack { board, .. } => board == local,
                _ => false,
            })
            .map(|pending| PersistenceAction::CompleteOp(pending.op_id)),
    );
    Ok(actions)
}

fn rewrite_value(value: &mut Value, ids: &HashMap<Uuid, Uuid>) {
    match value {
        Value::String(text) => {
            if let Some(new) = Uuid::parse_str(text).ok().and_then(|id| ids.get(&id)) {
                *text = new.to_string();
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| rewrite_value(item, ids)),
        // Map keys may be composites such as a snooze's
        // `default:<uuid>|<instant>`, and are never free text.
        Value::Object(map) => {
            *map = std::mem::take(map)
                .into_iter()
                .map(|(key, mut item)| {
                    rewrite_value(&mut item, ids);
                    (rewrite_uuid_text(&key, ids), item)
                })
                .collect();
        }
        _ => {}
    }
}

/// The JSON walk covers every id-bearing field by construction, so a
/// field added to any of these families is re-keyed without a code
/// change here. Exact string equality keeps free text untouched.
fn rewrite<T: Serialize + DeserializeOwned>(target: &mut T, ids: &HashMap<Uuid, Uuid>) {
    let mut value = serde_json::to_value(&*target).expect("persisted state serializes");
    rewrite_value(&mut value, ids);
    *target = serde_json::from_value(value).expect("an id rewrite preserves the shape");
}

/// Rewrites the UUIDs inside stored JSON text (storage's payload
/// columns); other text passes through.
#[must_use]
pub fn rewrite_uuid_text<S: std::hash::BuildHasher>(
    text: &str,
    pairs: &HashMap<Uuid, Uuid, S>,
) -> String {
    const UUID_LEN: usize = 36;
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let hit = (bytes.len() - at >= UUID_LEN)
            .then(|| text.get(at..at + UUID_LEN))
            .flatten()
            .filter(|window| window.as_bytes()[8] == b'-')
            .and_then(|window| Uuid::parse_str(window).ok())
            .and_then(|id| pairs.get(&id));
        if let Some(new) = hit {
            out.push_str(&new.to_string());
            at += UUID_LEN;
        } else {
            let width = text[at..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&text[at..at + width]);
            at += width;
        }
    }
    out
}

/// The in-memory definition of [`PersistenceAction::AdoptMovedBoard`].
pub(crate) fn apply_adoption(state: &mut PersistedState, rekey: &BoardRekey) {
    let ids: HashMap<Uuid, Uuid> = rekey.pairs().into_iter().collect();
    rewrite(&mut state.boards, &ids);
    rewrite(&mut state.stacks, &ids);
    rewrite(&mut state.tasks, &ids);
    rewrite(&mut state.labels, &ids);
    rewrite(&mut state.outbox, &ids);
    rewrite(&mut state.reminders, &ids);
    rewrite(&mut state.tracking, &ids);
    rewrite(&mut state.habits, &ids);
    rewrite(&mut state.journal, &ids);

    state.sync.remove(rekey.old_board);
    if let Some(board) = state.boards.get_mut(&rekey.new_board) {
        board.remote = Some(rekey.binding.clone());
        board.remote_seen = None;
    }
    let stacks: Vec<StackId> = state
        .stacks
        .values_mut()
        .filter(|stack| stack.board == rekey.new_board)
        .map(|stack| {
            stack.remote = None;
            stack.remote_seen = None;
            stack.id
        })
        .collect();
    for task in state
        .tasks
        .values_mut()
        .filter(|t| stacks.contains(&t.stack))
    {
        task.remote = None;
        task.remote_seen = None;
        task.remote_baseline = None;
    }
    for label in state
        .labels
        .values_mut()
        .filter(|label| label.board == rekey.new_board)
    {
        label.remote = None;
        label.remote_seen = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{
        Board, BoardAuthors, BoardClocks, Color, Label, LabelAuthors, LabelClocks, Stack,
        StackAuthors, Task, TaskAuthors, TaskClocks, TaskKind,
    };
    use crate::ids::{RemoteCardRef, RemoteLabelRef, RemoteStackRef};
    use crate::ownership::BoardOwnershipMap;
    use crate::persistence::apply_actions;
    use crate::test_support::proptest_config;
    use crate::test_support::state::persisted_state_strategy;
    use proptest::prelude::*;

    const DECK: u64 = 42;

    fn uuid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }
    fn epoch() -> chrono::DateTime<chrono::Utc> {
        BoardClocks::epoch()
    }
    fn deck_board() -> RemoteBoardId {
        RemoteBoardId::deck(DECK)
    }
    fn nextcloud() -> ConnectorId {
        ConnectorId::from("nextcloud")
    }
    fn files() -> ConnectorId {
        ConnectorId::from("files-sync")
    }

    fn state() -> PersistedState {
        let board = BoardId::from_uuid(uuid(1));
        let mut state = PersistedState::default();
        state.boards.insert(
            board,
            Board {
                id: board,
                remote: Some(RemoteBoardBinding {
                    connector: nextcloud(),
                    board: deck_board(),
                }),
                title: "b".into(),
                color: Color::new("3366cc"),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: epoch(),
                    color: epoch(),
                    archived: epoch(),
                    deleted: epoch(),
                },
                authors: BoardAuthors::default(),
                remote_seen: Some(epoch()),
            },
        );
        let stack = StackId::from_uuid(uuid(2));
        state.stacks.insert(
            stack,
            Stack {
                id: stack,
                remote: Some(RemoteStackRef {
                    board: deck_board(),
                    stack: RemoteStackId::deck(7),
                }),
                board,
                title: "s".into(),
                order: 1,
                archived: false,
                deleted: false,
                clocks: crate::StackClocks {
                    title: epoch(),
                    order: epoch(),
                    deleted: epoch(),
                },
                authors: StackAuthors::default(),
                remote_seen: Some(epoch()),
            },
        );
        for (id, card) in [(3, Some(70)), (4, None)] {
            let task = TaskId::from_uuid(uuid(id));
            state.tasks.insert(task, task_row(task, stack, card));
        }
        let label = LabelId::from_uuid(uuid(5));
        state.labels.insert(
            label,
            Label {
                id: label,
                remote: Some(RemoteLabelRef {
                    board: deck_board(),
                    label: RemoteLabelId::deck(9),
                }),
                board,
                title: "l".into(),
                color: Color::new("3366cc"),
                deleted: false,
                clocks: LabelClocks {
                    title: epoch(),
                    color: epoch(),
                    deleted: epoch(),
                },
                authors: LabelAuthors::default(),
                remote_seen: Some(epoch()),
            },
        );
        state
    }

    fn task_row(id: TaskId, stack: StackId, card: Option<u64>) -> Task {
        Task {
            id,
            stack,
            remote: card.map(|card| RemoteCardRef {
                board: deck_board(),
                stack: RemoteStackId::deck(7),
                card: RemoteCardId::deck(card),
            }),
            title: "t".into(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            order: 1,
            label_memberships: BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: epoch(),
                description: epoch(),
                duedate: epoch(),
                scheduled: epoch(),
                done: epoch(),
                position: epoch(),
                effort: epoch(),
                importance: epoch(),
                urgency: epoch(),
                recur: epoch(),
                kind: epoch(),
                archived: epoch(),
                deleted: epoch(),
            },
            authors: TaskAuthors::default(),
            remote_seen: Some(epoch()),
            remote_baseline: None,
        }
    }

    fn adoption() -> BoardAdoption {
        BoardAdoption {
            files_board: uuid(101),
            deck_board: DECK,
            stacks: BTreeMap::from([(RemoteStackId::deck(7), uuid(102))]),
            cards: BTreeMap::from([(RemoteCardId::deck(70), uuid(103))]),
            labels: BTreeMap::from([(RemoteLabelId::deck(9), uuid(105))]),
        }
    }

    fn tables(state: &PersistedState) -> EntityTables<'_> {
        EntityTables {
            tasks: &state.tasks,
            stacks: &state.stacks,
            labels: &state.labels,
            boards: &state.boards,
        }
    }

    fn plan(state: &PersistedState) -> Result<Vec<PersistenceAction>, AdoptionRefusal> {
        plan_board_adoption(
            &tables(state),
            &state.outbox,
            BoardId::from_uuid(uuid(1)),
            &adoption(),
            &files(),
        )
    }

    fn adopted() -> PersistedState {
        let mut state = state();
        let actions = plan(&state).expect("plans");
        apply_actions(&mut state, &actions);
        state
    }

    #[test]
    fn mapped_rows_take_the_movers_ids_and_lose_their_deck_refs() {
        let state = adopted();
        let board = BoardId::from_uuid(uuid(101));
        let stack = StackId::from_uuid(uuid(102));
        assert_eq!(state.stacks[&stack].board, board);
        let task = &state.tasks[&TaskId::from_uuid(uuid(103))];
        assert_eq!(task.stack, stack);
        assert_eq!(task.remote, None);
        assert_eq!(task.remote_seen, None);
        assert_eq!(state.stacks[&stack].remote, None);
        assert_eq!(state.labels[&LabelId::from_uuid(uuid(105))].board, board);
        assert!(!state.boards.contains_key(&BoardId::from_uuid(uuid(1))));
    }

    #[test]
    fn an_unmapped_card_keeps_its_id_under_the_rekeyed_stack() {
        let state = adopted();
        let task = &state.tasks[&TaskId::from_uuid(uuid(4))];
        assert_eq!(task.stack, StackId::from_uuid(uuid(102)));
    }

    #[test]
    fn the_board_has_exactly_one_owner_on_each_side_of_the_write() {
        let before = state();
        let map = BoardOwnershipMap::from_boards(&before.boards);
        assert_eq!(map.bindings().count(), 1);
        assert_eq!(
            map.owner_of(&BoardId::from_uuid(uuid(1))),
            Some(&nextcloud())
        );
        let after = adopted();
        let map = BoardOwnershipMap::from_boards(&after.boards);
        assert_eq!(map.bindings().count(), 1);
        assert_eq!(map.owner_of(&BoardId::from_uuid(uuid(101))), Some(&files()));
    }

    #[test]
    fn queued_ops_follow_the_rekey_and_ref_deletes_complete() {
        let mut state = state();
        let task = TaskId::from_uuid(uuid(3));
        let update = PendingOp {
            op_id: crate::OpId(uuid(201)),
            op: LocalOp::UpdateTask(task),
            queued_at: epoch(),
        };
        let delete = PendingOp {
            op_id: crate::OpId(uuid(202)),
            op: LocalOp::DeleteRemoteCard {
                board: BoardId::from_uuid(uuid(1)),
                card: RemoteCardRef {
                    board: deck_board(),
                    stack: RemoteStackId::deck(7),
                    card: RemoteCardId::deck(71),
                },
            },
            queued_at: epoch(),
        };
        state.outbox = vec![update, delete];
        let actions = plan(&state).expect("plans");
        apply_actions(&mut state, &actions);
        assert_eq!(state.outbox.len(), 1);
        assert_eq!(
            state.outbox[0].op,
            LocalOp::UpdateTask(TaskId::from_uuid(uuid(103)))
        );
    }

    #[test]
    fn a_collision_refuses_and_plans_nothing() {
        let mut state = state();
        let stack = StackId::from_uuid(uuid(2));
        let squatter = TaskId::from_uuid(uuid(103));
        state
            .tasks
            .insert(squatter, task_row(squatter, stack, None));
        assert_eq!(plan(&state), Err(AdoptionRefusal::Collision));
    }

    #[test]
    fn a_board_off_the_moved_deck_board_refuses() {
        let mut state = state();
        for board in state.boards.values_mut() {
            board.remote = None;
        }
        assert_eq!(plan(&state), Err(AdoptionRefusal::NotBoundToMovedDeck));
    }

    #[test]
    fn the_sidecar_marker_reads_back_and_a_malformed_one_is_absent() {
        let mut doc = crate::appdata::AppDataDocument {
            format: crate::appdata::APPDATA_FORMAT,
            board: deck_board(),
            sections: crate::appdata::AppDataSections::default(),
            unknown: BTreeMap::new(),
        };
        assert_eq!(doc.moved_to_files(), None);
        let marker = MovedToFilesMarker {
            files_board: uuid(101),
            committed_at: epoch(),
        };
        doc.unknown.insert(
            MOVED_TO_FILES_KEY.to_owned(),
            serde_json::to_value(marker).expect("serializes"),
        );
        assert_eq!(doc.moved_to_files(), Some(marker));
        doc.unknown
            .insert(MOVED_TO_FILES_KEY.to_owned(), Value::Bool(true));
        assert_eq!(doc.moved_to_files(), None);
    }

    #[test]
    fn a_marked_sidecar_derives_the_moved_health() {
        use crate::messages::AppDataEvidence;
        use crate::state::{SidecarHealth, derive_sidecar_health};
        let marker = MovedToFilesMarker {
            files_board: uuid(101),
            committed_at: epoch(),
        };
        let doc = crate::appdata::AppDataDocument {
            format: crate::appdata::APPDATA_FORMAT,
            board: deck_board(),
            sections: crate::appdata::AppDataSections::default(),
            unknown: BTreeMap::from([(
                MOVED_TO_FILES_KEY.to_owned(),
                serde_json::to_value(marker).expect("serializes"),
            )]),
        };
        assert_eq!(
            derive_sidecar_health(&AppDataEvidence::Document(Box::new(doc))),
            SidecarHealth::MovedToFiles {
                files_board: uuid(101)
            }
        );
    }

    #[test]
    fn stored_text_rewrites_uuids_and_keeps_other_text() {
        let pairs = HashMap::from([(uuid(1), uuid(2))]);
        let text = format!(r#"{{"a":"{}","é":"{}"}}"#, uuid(1), uuid(3));
        assert_eq!(
            rewrite_uuid_text(&text, &pairs),
            format!(r#"{{"a":"{}","é":"{}"}}"#, uuid(2), uuid(3))
        );
    }

    proptest! {
        #![proptest_config(proptest_config(32))]

        #[test]
        fn rewriting_nothing_is_lossless(mut state in persisted_state_strategy()) {
            let before = state.clone();
            let none = HashMap::new();
            rewrite(&mut state.reminders, &none);
            rewrite(&mut state.tracking, &none);
            rewrite(&mut state.habits, &none);
            rewrite(&mut state.journal, &none);
            rewrite(&mut state.outbox, &none);
            rewrite(&mut state.tasks, &none);
            prop_assert_eq!(state, before);
        }

        #[test]
        fn a_full_rekey_leaves_no_old_id_and_no_dangling_reference(
            mut state in persisted_state_strategy()
        ) {
            let fresh = |id: Uuid| Uuid::from_u128(!id.as_u128());
            let Some(board) = state.boards.keys().next().copied() else {
                return Ok(());
            };
            let stacks: BTreeMap<_, _> = state.stacks.values()
                .filter(|s| s.board == board)
                .map(|s| (s.id, StackId::from_uuid(fresh(s.id.as_uuid())))).collect();
            let tasks: BTreeMap<_, _> = state.tasks.values()
                .filter(|t| stacks.contains_key(&t.stack))
                .map(|t| (t.id, TaskId::from_uuid(fresh(t.id.as_uuid())))).collect();
            let labels: BTreeMap<_, _> = state.labels.values()
                .filter(|l| l.board == board)
                .map(|l| (l.id, LabelId::from_uuid(fresh(l.id.as_uuid())))).collect();
            let rekey = BoardRekey {
                old_board: board,
                new_board: BoardId::from_uuid(fresh(board.as_uuid())),
                binding: RemoteBoardBinding {
                    connector: files(),
                    board: RemoteBoardId::files(fresh(board.as_uuid())),
                },
                stacks, tasks, labels,
            };
            let old: Vec<String> = rekey.pairs().iter().map(|(o, _)| o.to_string()).collect();
            apply_adoption(&mut state, &rekey);

            let dump = serde_json::to_string(&(
                &state.boards, &state.stacks, &state.tasks, &state.labels,
                &state.outbox, &state.reminders, &state.tracking, &state.habits, &state.journal,
            )).expect("serializes");
            for id in &old {
                prop_assert!(!dump.contains(id.as_str()));
            }
            for task in state.tasks.values() {
                prop_assert!(state.stacks.contains_key(&task.stack));
            }
            for stack in state.stacks.values() {
                prop_assert!(state.boards.contains_key(&stack.board));
            }
        }
    }
}
