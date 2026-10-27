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
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
    )
        .prop_map(|(t, d, du, sc, do_, p, l, ef, im, ur, a, del)| TaskClocks {
            title: t,
            description: d,
            duedate: du,
            scheduled: sc,
            done: do_,
            position: p,
            labels: l,
            effort: ef,
            importance: im,
            urgency: ur,
            recur: ur,

            archived: a,
            deleted: del,
        })
}

/// Bounded effort seconds (`0..=10^9`): values stay inside the storage
/// codec's `i64` fence by a wide margin.
pub fn effort_strategy() -> impl Strategy<Value = crate::entities::Effort> {
    (0_u64..=1_000_000_000).prop_map(crate::entities::Effort::new)
}

/// All five levels, uniform.
pub fn importance_strategy() -> impl Strategy<Value = crate::entities::Importance> {
    prop::sample::select(vec![
        crate::entities::Importance::VeryLow,
        crate::entities::Importance::Low,
        crate::entities::Importance::Normal,
        crate::entities::Importance::High,
        crate::entities::Importance::VeryHigh,
    ])
}

/// All five levels, uniform.
pub fn urgency_strategy() -> impl Strategy<Value = crate::entities::Urgency> {
    prop::sample::select(vec![
        crate::entities::Urgency::VeryLow,
        crate::entities::Urgency::Low,
        crate::entities::Urgency::Normal,
        crate::entities::Urgency::High,
        crate::entities::Urgency::VeryHigh,
    ])
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
        (
            super::task_id_strategy(),
            proptest::option::of(remote_card_ref_strategy()),
            string_strategy(),
            string_strategy(),
            proptest::option::of(timestamp_strategy()),
            proptest::option::of(timestamp_strategy()),
            proptest::option::of(timestamp_strategy()),
            super::stack_id_strategy(),
        ),
        (
            -1_000_i64..=1_000,
            prop::collection::btree_set(super::label_id_strategy(), 0..3),
            proptest::option::of(effort_strategy()),
            proptest::option::of(importance_strategy()),
            proptest::option::of(urgency_strategy()),
            (any::<bool>(), any::<bool>()),
            task_clocks_strategy(),
            proptest::option::of(timestamp_strategy()),
        ),
    )
        .prop_map(
            |(
                (id, remote, title, description, duedate, done, scheduled, stack),
                (
                    order,
                    labels,
                    effort,
                    importance,
                    urgency,
                    (archived, deleted),
                    clocks,
                    remote_seen,
                ),
            )| Task {
                id,
                remote,
                title,
                description,
                duedate,
                scheduled,
                done,
                stack,
                order,
                labels,
                effort,
                importance,
                urgency,
                recur: None,
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
