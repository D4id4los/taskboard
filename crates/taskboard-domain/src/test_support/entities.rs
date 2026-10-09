// SPDX-License-Identifier: MIT OR Apache-2.0
//! Strategies for the canonical entities and their per-field clocks.
//!
//! Standalone entity strategies are *not* referentially bound; use
//! [`crate::test_support::state::app_state_strategy`] for state-level
//! tests that need `stack`/`labels` to point at existing rows.

use proptest::prelude::*;

use super::{string_strategy, timestamp_strategy};
use crate::entities::{LabelClocks, StackClocks, Task, TaskClocks};
use crate::ids::{RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteStackId};

pub fn task_clocks_strategy() -> impl Strategy<Value = TaskClocks> {
    (
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
    )
        .prop_map(|(t, d, du, do_, p, l, a, del)| TaskClocks {
            title: t,
            description: d,
            duedate: du,
            done: do_,
            position: p,
            labels: l,
            archived: a,
            deleted: del,
        })
}

pub fn stack_clocks_strategy() -> impl Strategy<Value = StackClocks> {
    (
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
    )
        .prop_map(|(t, o, del)| StackClocks {
            title: t,
            order: o,
            deleted: del,
        })
}

pub fn label_clocks_strategy() -> impl Strategy<Value = LabelClocks> {
    (
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
    )
        .prop_map(|(t, c, del)| LabelClocks {
            title: t,
            color: c,
            deleted: del,
        })
}

/// A task entity.
pub fn task_strategy() -> impl Strategy<Value = Task> {
    (
        super::task_id_strategy(),
        proptest::option::of(remote_card_ref_strategy()),
        string_strategy(),
        string_strategy(),
        proptest::option::of(timestamp_strategy()),
        proptest::option::of(timestamp_strategy()),
        super::stack_id_strategy(),
        -1_000_i64..=1_000,
        prop::collection::btree_set(super::label_id_strategy(), 0..3),
        (any::<bool>(), any::<bool>()),
        task_clocks_strategy(),
        proptest::option::of(timestamp_strategy()),
    )
        .prop_map(
            |(
                id,
                remote,
                title,
                description,
                duedate,
                done,
                stack,
                order,
                labels,
                (archived, deleted),
                clocks,
                remote_seen,
            )| Task {
                id,
                remote,
                title,
                description,
                duedate,
                done,
                stack,
                order,
                labels,
                archived,
                deleted,
                clocks,
                remote_seen,
            },
        )
}

/// A remote card ref over independent positive numbers.
pub(crate) fn remote_card_ref_strategy() -> impl Strategy<Value = RemoteCardRef> {
    (
        super::remote_number(),
        super::remote_number(),
        super::remote_number(),
    )
        .prop_map(|(board, stack, card)| RemoteCardRef {
            board: RemoteBoardId(board),
            stack: RemoteStackId(stack),
            card: RemoteCardId(card),
        })
}
