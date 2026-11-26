// SPDX-License-Identifier: MIT OR Apache-2.0
//! The canonical order-key minting family (#1351, re-homed from the UI's
//! `view_state`): the ONE home for "where does an inserted row land".
//! Every function here is pure over [`AppState`] — no IO, no clock, no
//! presentation types — so the UI crate re-exports them and both the
//! interactive layer and the CLI speak one mint. The semantics are pinned
//! by the `#[cfg(test)]` block below (splice collision-freedom, the end
//! arms, the aim-command shape, the stack append); the engine-snapshot
//! streams are unchanged by the move (the pins prove the arithmetic
//! verbatim).

use crate::Task;
use crate::entities::{Board, Stack};
use crate::ids::{BoardId, StackId, TaskId};
use crate::messages::StateCommand;
use crate::state::AppState;

/// The canonical append position in `stack`: max canonical order + 1.
#[must_use]
pub fn append_order(state: &AppState, stack: StackId) -> i64 {
    state
        .tasks_in_stack(stack)
        .last()
        .map_or(0, |t| t.order + 1)
}

/// The insertion slot's END orders (UX-R8 D-1, re-signed B3 2026-11-24
/// #1247): gap `0` takes `first.order - 1` (before the first); any
/// other gap takes the [`append_order`] (after the last). The
/// between-ends midpoint arm is DELETED: its adjacent-pair midpoint
/// could equal `a.order` — an occupied order whose `(order, id)` tie
/// resolved by `UUIDv7` mint-order luck (#1247's verified defect) — and
/// every same-stack move now mints a gap index for
/// [`StateCommand::TaskReorder`] instead; cross-stack insertion is the
/// append (interior cross-stack positions are the multi-move
/// block-splice's named out-of-scope sibling). No move path computes
/// a midpoint anymore: a gap is either an engine-side reorder index
/// or one of these two end orders.
#[must_use]
pub fn slot_order(state: &AppState, stack: StackId, gap: usize) -> i64 {
    if gap == 0 {
        return state
            .tasks_in_stack(stack)
            .first()
            .map_or(0, |t| t.order - 1);
    }
    append_order(state, stack)
}

/// The one move-aim mint (B3 2026-11-24 #1247, the r2 chunk-8 D-F
/// one-construction-site law carried onto the reorder grammar): the
/// keyboard confirm and the pointer drop both build their command
/// HERE, so command equivalence is by construction. A same-stack aim
/// mints [`StateCommand::TaskReorder`] at the gap (`None` = the append
/// gap, index `len`); a cross-stack aim keeps `MoveTask` — gap `0`
/// via the first-position end arm of [`slot_order`], anything else
/// via the [`append_order`].
#[must_use]
pub fn move_aim_command(
    state: &AppState,
    task: TaskId,
    target: StackId,
    gap: Option<usize>,
) -> StateCommand {
    let same_stack = state.tasks.get(&task).is_some_and(|t| t.stack == target);
    if same_stack {
        let append_gap = state.tasks_in_stack(target).len();
        return StateCommand::TaskReorder {
            id: task,
            target: gap.unwrap_or(append_gap),
        };
    }
    let order = match gap {
        Some(0) => slot_order(state, target, 0),
        _ => append_order(state, target),
    };
    StateCommand::MoveTask {
        id: task,
        stack: target,
        order,
    }
}

/// The multi-move sequence (UX-R8 D-6, US-4/S2): one `MoveTask` per
/// selected member in `members` order (the anchor order, D-4),
/// relative order preserved within the moved set — the set's members
/// reinsert in their existing relative sequence at the aim point.
/// Cross-stack (or bare-surface) aims append: `append_order + i` per
/// member; a same-stack gap splices the block between the remaining
/// neighbours (room allowing; otherwise the deterministic append
/// fallback — the documented leniency, the engine's `(order, id)`
/// sort keeps any collision stable). Per-task absorb: a member not
/// live against the passed state is dropped and the sequence
/// continues — no batch verb, no early abort.
///
/// `ring` is the caller-supplied move ring — the flat cross-board
/// stack candidate list ([`move_ring`]); it only orders the members'
/// existing relative sequence, never the minted orders.
///
/// # Panics
/// Never: the per-member index conversions saturate at `i64::MAX`
/// (the documented leniency — sequence lengths are far below the
/// conversion's reach anyway).
#[must_use]
pub fn multi_move_commands(
    state: &AppState,
    members: &[TaskId],
    target: StackId,
    gap: Option<usize>,
    ring: &[StackId],
) -> Vec<StateCommand> {
    let is_live = |id: TaskId| state.tasks.get(&id).is_some_and(Task::is_live);
    let live: Vec<TaskId> = members.iter().copied().filter(|id| is_live(*id)).collect();
    if live.is_empty() {
        return Vec::new();
    }
    // The existing relative sequence: the members' current canonical
    // position — stack ring order, then in-stack order.
    let mut sorted = live;
    sorted.sort_by_key(|id| {
        let task = &state.tasks[id];
        (
            ring.iter().position(|stack| *stack == task.stack),
            task.order,
        )
    });
    let same_stack = sorted
        .iter()
        .any(|id| state.tasks.get(id).is_some_and(|t| t.stack == target));
    let per_member = |base: i64| -> Vec<i64> {
        (0..sorted.len())
            .map(|i| base + i64::try_from(i).unwrap_or(i64::MAX))
            .collect()
    };
    let orders: Vec<i64> = if same_stack {
        let set: std::collections::BTreeSet<TaskId> = sorted.iter().copied().collect();
        let remaining: Vec<i64> = state
            .tasks_in_stack(target)
            .iter()
            .filter(|t| !set.contains(&t.id))
            .map(|t| t.order)
            .collect();
        let gap = gap.unwrap_or(remaining.len());
        let append_base = remaining.last().map_or(0, |last| last + 1);
        if gap >= remaining.len() {
            per_member(append_base)
        } else {
            let a = remaining[gap - 1];
            let b = remaining[gap];
            let k = i64::try_from(sorted.len()).unwrap_or(i64::MAX);
            if b - a > k {
                let step = (b - a) / (k + 1);
                (0..sorted.len())
                    .map(|i| a + step * (i64::try_from(i).unwrap_or(i64::MAX) + 1))
                    .collect()
            } else {
                per_member(append_base)
            }
        }
    } else {
        per_member(append_order(state, target))
    };
    sorted
        .into_iter()
        .zip(orders)
        .map(|(id, order)| StateCommand::MoveTask {
            id,
            stack: target,
            order,
        })
        .collect()
}

/// The canonical append position on `board`: the board's live stacks'
/// max order + 1 (#304's "sensible default appended after the existing
/// stacks"); 0 on an empty board.
#[must_use]
pub fn append_stack_order(state: &AppState, board: BoardId) -> i64 {
    state
        .stacks
        .values()
        .filter(|s| s.board == board && !s.deleted)
        .map(|s| s.order)
        .max()
        .map_or(0, |max| max + 1)
}

/// The board's live stacks in display order (D10): `(order, id)`
/// ascending — the domain's own reading of the persisted sort position
/// (the domain owns the concept and the write side; this is the
/// canonical reader over it).
#[must_use]
pub fn stacks_in_board(state: &AppState, board: BoardId) -> Vec<&Stack> {
    let mut stacks: Vec<&Stack> = state
        .stacks
        .values()
        .filter(|s| !s.deleted && s.board == board)
        .collect();
    stacks.sort_by_key(|s| (s.order, s.id));
    stacks
}

/// The live, non-archived boards in canonical order: `state.boards`
/// `BTreeMap` iteration order — board-id ascending, the exact order
/// the CLI's `boards list` renders. The board walk's domain and the
/// seed rule's candidate list.
#[must_use]
pub fn live_boards(state: &AppState) -> Vec<&Board> {
    state
        .boards
        .values()
        .filter(|b| !b.deleted && !b.archived)
        .collect()
}

/// The move ring (chunk 10 D6, the deliberate behavior change): the
/// *flat cross-board* candidate list — every live board's stacks in
/// `(order, id)` order, boards in canonical `BTreeMap` order —
/// wrapping across the board edge. No Inbox special-casing: the Inbox
/// board's stacks appear like any board's.
#[must_use]
pub fn move_ring(state: &AppState) -> Vec<StackId> {
    live_boards(state)
        .into_iter()
        .flat_map(|board| stacks_in_board(state, board.id))
        .map(|stack| stack.id)
        .collect()
}

/// The flattened slot enumeration (UX-R8 D-1, US-1/AC-3): every live
/// stack's insertion gaps — stacks in [`move_ring`] order, positions
/// top→bottom within a stack, N+1 slots per stack (a single-task stack
/// offers exactly two). Computed on demand from the live state, never
/// stored — refresh-survival by re-derivation, the ring's existing law.
#[must_use]
pub fn move_slots(state: &AppState) -> Vec<(StackId, usize)> {
    move_ring(state)
        .into_iter()
        .flat_map(|stack| {
            let len = state.tasks_in_stack(stack).len();
            (0..=len).map(move |gap| (stack, gap))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{StackAuthors, StackClocks, TaskAuthors, TaskClocks, TaskKind};
    use crate::ids::BoardId;
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    fn epoch() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::<chrono::Utc>::UNIX_EPOCH
    }

    fn task(raw: u128, stack: StackId, order: i64) -> Task {
        Task {
            id: TaskId::from(uuid::Uuid::from_u128(raw)),
            remote: None,
            title: format!("task {raw}"),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack,
            order,
            label_memberships: BTreeMap::default(),
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
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn stack(raw: u128, board: BoardId, order: i64) -> crate::entities::Stack {
        crate::entities::Stack {
            id: StackId::from(uuid::Uuid::from_u128(raw)),
            remote: None,
            board,
            title: format!("stack {raw}"),
            order,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: epoch(),
                order: epoch(),
                deleted: epoch(),
            },
            authors: StackAuthors::default(),
            remote_seen: None,
        }
    }

    /// A state with one board and `count` live stacks (canonical orders
    /// `0..count`); returns the state and the stack ids in order.
    fn seeded(count: u128) -> (AppState, BoardId, Vec<StackId>) {
        let board = BoardId::from(uuid::Uuid::from_u128(1));
        let mut state = AppState::default();
        let stacks: Vec<StackId> = (0..count)
            .map(|i| {
                let s = stack(10 + i, board, i64::try_from(i).unwrap());
                let id = s.id;
                state.stacks.insert(id, s);
                id
            })
            .collect();
        (state, board, stacks)
    }

    fn push_task(state: &mut AppState, raw: u128, stack: StackId, order: i64) -> TaskId {
        let t = task(raw, stack, order);
        let id = t.id;
        state.tasks.insert(id, t);
        id
    }

    // -- the end arms (the exact law the deleted midpoint arm violated) --

    #[test]
    fn append_order_is_last_order_plus_one() {
        let (mut state, _, stacks) = seeded(2);
        let s0 = stacks[0];
        let s1 = stacks[1];
        push_task(&mut state, 100, s0, 0);
        push_task(&mut state, 101, s0, 1);
        push_task(&mut state, 102, s1, 0);
        assert_eq!(append_order(&state, s0), 2);
        assert_eq!(append_order(&state, s1), 1);
        let empty = StackId::from(uuid::Uuid::now_v7());
        assert_eq!(append_order(&state, empty), 0, "an empty stack appends 0");
    }

    #[test]
    fn slot_order_gap0_is_first_order_minus_one() {
        let (mut state, _, stacks) = seeded(2);
        let s0 = stacks[0];
        let s1 = stacks[1];
        push_task(&mut state, 100, s0, 0);
        push_task(&mut state, 101, s0, 1);
        push_task(&mut state, 102, s1, 7);
        assert_eq!(slot_order(&state, s0, 0), -1, "before the first");
        assert_eq!(slot_order(&state, s1, 0), 6, "before the only task");
        let empty = StackId::from(uuid::Uuid::now_v7());
        assert_eq!(
            slot_order(&state, empty, 0),
            0,
            "an empty stack's slot is 0"
        );
    }

    #[test]
    fn slot_order_interior_gap_is_the_append() {
        // B3 (2026-11-24, #1247): the between-ends midpoint arm is
        // deleted — a between gap mints the append, no midpoint.
        let (mut state, _, stacks) = seeded(1);
        let s0 = stacks[0];
        push_task(&mut state, 100, s0, 0);
        push_task(&mut state, 101, s0, 1);
        assert_eq!(
            slot_order(&state, s0, 1),
            2,
            "a between gap mints the append — no midpoint anymore"
        );
        assert_eq!(slot_order(&state, s0, 2), 2, "after the last (append)");
        assert_eq!(slot_order(&state, s0, 9), 2, "any non-0 gap is the append");
    }

    #[test]
    fn move_aim_same_stack_is_a_gap_index_and_cross_stack_is_the_end_arm() {
        // The one-mint law: a same-stack aim carries `TaskReorder` (a
        // gap index), NEVER an absolute `MoveTask` order; a cross-stack
        // aim keeps `MoveTask` via the two end arms.
        let (mut state, _, stacks) = seeded(2);
        let s0 = stacks[0];
        let s1 = stacks[1];
        let a = push_task(&mut state, 100, s0, 0);
        let b = push_task(&mut state, 101, s0, 1);
        let c = push_task(&mut state, 102, s1, 0);
        // Same-stack: always the gap-index reorder (None = the append gap).
        assert_eq!(
            move_aim_command(&state, a, s0, Some(1)),
            StateCommand::TaskReorder { id: a, target: 1 }
        );
        assert_eq!(
            move_aim_command(&state, a, s0, None),
            StateCommand::TaskReorder { id: a, target: 2 },
            "None means the append gap (index len)"
        );
        // Cross-stack: gap 0 = first-order-minus-one, anything else = append.
        assert_eq!(
            move_aim_command(&state, c, s0, Some(0)),
            StateCommand::MoveTask {
                id: c,
                stack: s0,
                order: -1
            }
        );
        assert_eq!(
            move_aim_command(&state, c, s0, None),
            StateCommand::MoveTask {
                id: c,
                stack: s0,
                order: 2
            }
        );
        assert_eq!(
            move_aim_command(&state, c, s0, Some(1)),
            StateCommand::MoveTask {
                id: c,
                stack: s0,
                order: 2
            },
            "an interior cross-stack gap is still the append"
        );
        let _ = b;
    }

    // -- the multi sequences (UX-R8 D-6, US-4/S2) -------------------------

    #[test]
    fn multi_move_appends_in_existing_relative_order() {
        let (mut state, _, stacks) = seeded(2);
        let s0 = stacks[0];
        let s1 = stacks[1];
        let a = push_task(&mut state, 100, s0, 0);
        let b = push_task(&mut state, 101, s0, 1);
        let c = push_task(&mut state, 102, s1, 0);
        let _ = c;
        let ring = [s0, s1];
        // The pair a, b moves from s0 to s1 (cross-stack append):
        // relative order kept, one command per member.
        let commands = multi_move_commands(&state, &[a, b], s1, None, &ring);
        assert_eq!(
            commands,
            vec![
                StateCommand::MoveTask {
                    id: a,
                    stack: s1,
                    order: 1
                },
                StateCommand::MoveTask {
                    id: b,
                    stack: s1,
                    order: 2
                },
            ],
            "append_order + i per member in existing sequence"
        );
        // The anchor order (a first) is the caller's; the sequence's
        // members land in their existing relative order regardless.
        let commands = multi_move_commands(&state, &[b, a], s1, None, &ring);
        assert_eq!(
            commands,
            vec![
                StateCommand::MoveTask {
                    id: a,
                    stack: s1,
                    order: 1
                },
                StateCommand::MoveTask {
                    id: b,
                    stack: s1,
                    order: 2
                },
            ],
            "the set sorts into its existing relative sequence"
        );
    }

    #[test]
    fn multi_move_splices_the_block_at_the_same_stack_gap() {
        let (mut state, _, stacks) = seeded(1);
        let s0 = stacks[0];
        let a = push_task(&mut state, 100, s0, 0);
        let b = push_task(&mut state, 101, s0, 1);
        // A third task d(2) in s0: orders a(0), b(1), d(2). Moving
        // the pair a, b between d and nothing (gap after the set's
        // removal = len) appends after d.
        let _d = push_task(&mut state, 102, s0, 2);
        let ring = [s0];
        let commands = multi_move_commands(&state, &[a, b], s0, Some(2), &ring);
        assert_eq!(
            commands,
            vec![
                StateCommand::MoveTask {
                    id: a,
                    stack: s0,
                    order: 3
                },
                StateCommand::MoveTask {
                    id: b,
                    stack: s0,
                    order: 4
                },
            ],
            "the block appends after the remaining neighbours"
        );
        // A between-gap with room: move a alone between b(1) and d(2)
        // — no room for a block; the block path needs room > k;
        // room = 1, k = 1 → the append fallback (the documented leniency).
        let commands = multi_move_commands(&state, &[a], s0, Some(2), &ring);
        assert_eq!(
            commands,
            vec![StateCommand::MoveTask {
                id: a,
                stack: s0,
                order: 3
            }],
            "a no-room gap falls back to the deterministic append"
        );
    }

    #[test]
    fn multi_move_absorbs_a_vanished_member_and_continues() {
        let (mut state, _, stacks) = seeded(2);
        let s0 = stacks[0];
        let s1 = stacks[1];
        let a = push_task(&mut state, 100, s0, 0);
        let b = push_task(&mut state, 101, s0, 1);
        // The cross-stack target s1 holds one live row (append base 1).
        let _c = push_task(&mut state, 102, s1, 0);
        let ring = [s0, s1];
        // b dies mid-sequence (the engine applied its deletion): the
        // sequence drops b's command and continues with a.
        state.tasks.get_mut(&b).expect("task").deleted = true;
        let commands = multi_move_commands(&state, &[a, b], s1, None, &ring);
        assert_eq!(
            commands,
            vec![StateCommand::MoveTask {
                id: a,
                stack: s1,
                order: 1
            }],
            "the vanished member's command is dropped, no early abort"
        );
        // An all-dead set dispatches nothing.
        state.tasks.get_mut(&a).expect("task").deleted = true;
        assert_eq!(
            multi_move_commands(&state, &[a, b], s1, None, &ring),
            [] as [StateCommand; 0]
        );
    }

    // -- the divergence-failing pins ([L](a), #1351) -----------------------

    proptest! {
        /// The splice law: with room (`b - a > k`), every minted order
        /// lands strictly between the bracketing remaining rows,
        /// distinct, colliding with no remaining row. Fails if anyone
        /// changes the arithmetic into the collision-lenient class.
        #[test]
        fn multi_move_splice_never_collides_with_a_remaining_row_when_room_allows(
            gaps in 1usize..8,
            members_count in 1usize..5,
            extra in 1i64..9,
        ) {
            // remaining rows at `step_width`-spaced orders (0, w, 2w,
            // ...), w = k + extra so `b - a > k` always holds; the moved
            // members trail them in the same stack.
            let step_width = i64::try_from(members_count).unwrap() + extra;
            let remaining_count = gaps + 1;
            let (mut state, _, stacks) = seeded(1);
        let s0 = stacks[0];
            let mut raw = 100u128;
            let mut order = 0i64;
            for _ in 0..remaining_count {
                push_task(&mut state, raw, s0, order);
                raw += 1;
                order += step_width;
            }
            let mut members = Vec::new();
            for _ in 0..members_count {
                members.push(push_task(&mut state, raw, s0, order));
                raw += 1;
                order += step_width;
            }
            let remaining_orders: Vec<i64> = (0..remaining_count)
                .map(|i| i64::try_from(i).unwrap() * step_width)
                .collect();
            let gap = gaps; // strictly interior: 1..remaining_count
            let a = remaining_orders[gap - 1];
            let b = remaining_orders[gap];
            let commands = multi_move_commands(&state, &members, s0, Some(gap), &[s0]);
            prop_assert_eq!(commands.len(), members_count);
            let minted: Vec<i64> = commands
                .iter()
                .map(|c| match c {
                    StateCommand::MoveTask { order, .. } => *order,
                    other => panic!("unexpected command: {other:?}"),
                })
                .collect();
            for m in &minted {
                prop_assert!(m > &a && m < &b, "order {m} escapes the bracket ({a}, {b})");
                prop_assert!(!remaining_orders.contains(m), "order {m} collides with a remaining row");
            }
            let mut distinct = minted.clone();
            distinct.sort_unstable();
            distinct.dedup();
            prop_assert_eq!(distinct.len(), minted.len(), "minted orders collide with each other");
        }

        /// The dense fallback pin (the permanent-contract half of
        /// #1355's direction): when the gap has no room (`b - a <= k`)
        /// the block appends after the last remaining row, monotonically
        /// increasing in member order.
        #[test]
        fn multi_move_dense_fallback_appends_after_the_last_remaining(
            gaps in 1usize..7,
            members_count in 1usize..5,
        ) {
            // Dense remaining orders 0..n (b - a == 1, never room for
            // k >= 1 members) plus the trailing moved members.
            let remaining_count = gaps + 1;
            let (mut state, _, stacks) = seeded(1);
        let s0 = stacks[0];
            for (raw, i) in (100u128..).zip(0..remaining_count + members_count) {
                let ord = i64::try_from(i).unwrap();
                push_task(&mut state, raw, s0, ord);
            }
            let members: Vec<TaskId> = ((remaining_count)..(remaining_count + members_count))
                .map(|i| state.tasks.iter().nth(i).expect("task").0)
                .copied()
                .collect();
            let last_remaining = i64::try_from(remaining_count - 1).unwrap();
            let commands = multi_move_commands(&state, &members, s0, Some(gaps), &[s0]);
            prop_assert_eq!(commands.len(), members_count);
            let minted: Vec<i64> = commands
                .iter()
                .map(|c| match c {
                    StateCommand::MoveTask { order, .. } => *order,
                    other => panic!("unexpected command: {other:?}"),
                })
                .collect();
            for m in &minted {
                prop_assert!(m >= &(last_remaining + 1), "fallback order {m} lands before the append base");
            }
            let mut sorted = minted.clone();
            sorted.sort_unstable();
            prop_assert_eq!(sorted, minted, "fallback orders increase in member order");
        }
    }

    // -- the stack append ---------------------------------------------------

    #[test]
    fn append_stack_order_is_max_live_stack_order_plus_one() {
        let (state, board, stacks) = seeded(2);
        let s0 = stacks[0];
        let s1 = stacks[1];
        assert_eq!(append_stack_order(&state, board), 2);
        // A tombstoned stack's order does not count (#304's live fold).
        let mut with_dead = state.clone();
        let dead = stack(99, board, 50);
        with_dead.stacks.insert(
            dead.id,
            crate::entities::Stack {
                deleted: true,
                ..dead
            },
        );
        assert_eq!(
            append_stack_order(&with_dead, board),
            2,
            "a deleted stack does not raise the append position"
        );
        // An empty board appends 0.
        let empty_board = BoardId::from(uuid::Uuid::now_v7());
        assert_eq!(append_stack_order(&state, empty_board), 0);
        let _ = (s0, s1);
    }

    // -- the move-ring reader family (the #1399 re-home) ------------------

    fn board(id: BoardId) -> Board {
        Board {
            id,
            remote: None,
            title: "board".to_owned(),
            color: crate::Color::new(""),
            archived: false,
            deleted: false,
            clocks: crate::entities::BoardClocks {
                title: epoch(),
                color: epoch(),
                archived: epoch(),
                deleted: epoch(),
            },
            authors: crate::entities::BoardAuthors::default(),
            remote_seen: None,
        }
    }

    #[test]
    fn stacks_in_board_filters_and_orders() {
        let b1 = BoardId::from_uuid(uuid::Uuid::from_u128(0x01));
        let b2 = BoardId::from_uuid(uuid::Uuid::from_u128(0x02));
        let mut state = AppState::default();
        state.boards.insert(b1, board(b1));
        // (order, id) order with a tie on order broken by id.
        let s1_id = StackId::from_uuid(uuid::Uuid::from_u128(0x11));
        let s2_id = StackId::from_uuid(uuid::Uuid::from_u128(0x12));
        state.stacks.insert(s2_id, stack(0x12, b1, 1));
        state.stacks.insert(s1_id, stack(0x11, b1, 1));
        let s3 = stack(0x13, b2, 0);
        state.stacks.insert(s3.id, s3);
        let mut deleted = stack(0x13, b1, 2);
        deleted.deleted = true;
        state.stacks.insert(deleted.id, deleted);

        let ordered: Vec<StackId> = stacks_in_board(&state, b1)
            .into_iter()
            .map(|s| s.id)
            .collect();
        let mut expected = vec![s1_id, s2_id];
        expected.sort();
        assert_eq!(ordered, expected, "other boards and tombstones filtered");
    }

    #[test]
    fn live_boards_filters_to_live_boards_in_canonical_order() {
        // Deterministic ids: live low + live high, one deleted and one
        // archived between them; the walk order is board-id ascending
        // (the `boards list` order).
        let low = BoardId::from_uuid(uuid::Uuid::from_u128(0x01));
        let dead = BoardId::from_uuid(uuid::Uuid::from_u128(0x02));
        let archived = BoardId::from_uuid(uuid::Uuid::from_u128(0x03));
        let high = BoardId::from_uuid(uuid::Uuid::from_u128(0x04));
        let mut state = AppState::default();
        state.boards.insert(high, board(high));
        state.boards.insert(archived, {
            let mut b = board(archived);
            b.archived = true;
            b
        });
        state.boards.insert(dead, {
            let mut b = board(dead);
            b.deleted = true;
            b
        });
        state.boards.insert(low, board(low));

        let ids: Vec<BoardId> = live_boards(&state).into_iter().map(|b| b.id).collect();
        assert_eq!(ids, vec![low, high]);
    }

    /// A state with two boards, the first carrying `first` and the
    /// second `second` live stacks in canonical order; returns the
    /// stack ids of each board in `(order, id)` order.
    fn two_board_fixture(first: usize, second: usize) -> (AppState, Vec<StackId>, Vec<StackId>) {
        fn seed(state: &mut AppState, count: usize, base: u128, board: BoardId) -> Vec<StackId> {
            (0..count)
                .map(|i| {
                    let s = stack(
                        base + u128::try_from(i).unwrap(),
                        board,
                        i64::try_from(i).unwrap(),
                    );
                    let id = s.id;
                    state.stacks.insert(id, s);
                    id
                })
                .collect()
        }
        let b0 = BoardId::from(uuid::Uuid::from_u128(0x01));
        let b1 = BoardId::from(uuid::Uuid::from_u128(0x02));
        let mut state = AppState::default();
        state.boards.insert(b0, board(b0));
        state.boards.insert(b1, board(b1));
        let first_ids = seed(&mut state, first, 0x10, b0);
        let second_ids = seed(&mut state, second, 0x20, b1);
        (state, first_ids, second_ids)
    }

    #[test]
    fn the_move_ring_is_the_flat_cross_board_candidate_list() {
        let (state, first, second) = two_board_fixture(2, 2);
        let [s0, s1] = first[..] else { unreachable!() };
        let [t0, t1] = second[..] else { unreachable!() };
        assert_eq!(
            move_ring(&state),
            vec![s0, s1, t0, t1],
            "boards in canonical order, each board's stacks in (order, id)"
        );
    }

    #[test]
    fn a_single_task_stack_enumerates_exactly_two_slots() {
        let (mut state, first, _) = two_board_fixture(2, 0);
        let [s0, s1] = first[..] else { unreachable!() };
        push_task(&mut state, 0x100, s1, 0);
        let _ = s0;
        let s1_slots: Vec<(StackId, usize)> = move_slots(&state)
            .into_iter()
            .filter(|(stack, _)| *stack == s1)
            .collect();
        assert_eq!(
            s1_slots,
            vec![(s1, 0), (s1, 1)],
            "one task, two insertion points: before and after"
        );
    }

    #[test]
    fn the_slot_walk_order_is_stack_canonical_top_to_bottom() {
        let (mut state, first, _) = two_board_fixture(2, 0);
        let [s0, s1] = first[..] else { unreachable!() };
        push_task(&mut state, 0x100, s0, 0);
        push_task(&mut state, 0x101, s0, 1);
        push_task(&mut state, 0x102, s1, 0);
        assert_eq!(
            move_slots(&state),
            vec![(s0, 0), (s0, 1), (s0, 2), (s1, 0), (s1, 1),],
            "stacks in ring order, gaps 0..=len within each"
        );
    }

    proptest! {
        /// `stacks_in_board` equals the reference `(order, id)` sort of
        /// the board's live stacks (test-side model recomputation).
        #[test]
        fn stacks_in_board_matches_reference_sort(orders in prop::collection::vec(0i64..5, 1..6)) {
            let b1 = BoardId::from_uuid(uuid::Uuid::from_u128(0x01));
            let b2 = BoardId::from_uuid(uuid::Uuid::from_u128(0x02));
            let mut state = AppState::default();
            state.boards.insert(b1, board(b1));
            let mut ids = Vec::new();
            for (i, order) in orders.iter().enumerate() {
                let s = stack(
                    0x30 + u128::try_from(i).unwrap(),
                    b1,
                    *order,
                );
                ids.push((s.id, *order));
                state.stacks.insert(s.id, s);
                if i == 0 {
                    let other = stack(0x20, b2, 0);
                    state.stacks.insert(other.id, other);
                }
            }
        let mut expected: Vec<(i64, StackId)> =
            ids.into_iter().map(|(id, order)| (order, id)).collect();
        expected.sort();
        let got: Vec<(i64, StackId)> = stacks_in_board(&state, b1)
            .into_iter()
            .map(|s| (s.order, s.id))
            .collect();
        prop_assert_eq!(got, expected);
        }
    }
}
