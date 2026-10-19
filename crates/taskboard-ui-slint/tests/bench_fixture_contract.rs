// SPDX-License-Identifier: AGPL-3.0-only
//! The refresh+draw bench fixture's contract (stage-1 plan T5/D7):
//! determinism, shape, and validity as bench input for the UI-mount
//! state. These tests pin what the bench relies on; board semantics
//! themselves are owned by the state crate's suite.

#[path = "../benches/support/mod.rs"]
mod support;

use support::{STACK_COUNT, build_state};

/// Two runs at each N must produce the identical state — the durable
/// regression test against rng/strategy/clock ever sneaking into the
/// builder (the storage fixture-contract convention).
#[test]
fn fixture_is_deterministic() {
    for n in support::N_SIZES {
        let first = build_state(n);
        let second = build_state(n);
        assert_eq!(first, second, "fixture at N={n} must be deterministic");
    }
}

/// Entity counts and shape: exactly one live board, `STACK_COUNT`
/// live stacks, exactly `n` non-deleted tasks round-robinned across
/// them, ASCII-only titles, and the fixed duedate/done patterns.
#[test]
fn fixture_shape_matches_contract() {
    for n in support::N_SIZES {
        let state = build_state(n);
        assert_eq!(state.boards.len(), 1);
        let board = state.boards.values().next().expect("one board");
        assert!(!board.deleted && !board.archived);

        assert_eq!(state.stacks.len(), STACK_COUNT);
        assert!(
            state
                .stacks
                .values()
                .all(|s| !s.deleted && s.board == board.id)
        );

        assert_eq!(state.tasks.len(), n);
        let mut per_stack = std::collections::BTreeMap::new();
        for task in state.tasks.values() {
            assert!(!task.deleted && !task.archived);
            assert!(
                task.title.chars().all(|c| c.is_ascii_graphic() || c == ' '),
                "titles must be ASCII-only so software text layout never \
                 falls back to a system font for a missing glyph"
            );
            assert_eq!(
                task.duedate.is_some(),
                support::has_duedate(task.order),
                "duedate pattern must be the fixed i%3 rule"
            );
            assert_eq!(
                task.done.is_some(),
                support::has_done(task.order),
                "done pattern must be the fixed i%10 rule"
            );
            *per_stack.entry(task.stack).or_insert(0_usize) += 1;
        }
        assert_eq!(per_stack.len(), STACK_COUNT, "tasks round-robin all stacks");
    }
}
