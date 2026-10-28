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
    )
        .prop_map(|(t, d, du, sc, do_, p, ef, im, ur, a, del)| TaskClocks {
            title: t,
            description: d,
            duedate: du,
            scheduled: sc,
            done: do_,
            position: p,
            effort: ef,
            importance: im,
            urgency: ur,
            recur: ur,

            archived: a,
            deleted: del,
        })
}

/// A per-membership OR-set state: either variant, over bounded stamps.
/// `Removed`-heavy by symmetry — the no-flap and entity-death properties
/// need tombstone-rich tasks.
pub fn membership_state_strategy() -> impl Strategy<Value = crate::entities::MembershipState> {
    (prop::sample::select(vec![0usize, 1]), timestamp_strategy()).prop_map(|(variant, at)| {
        match variant {
            0 => crate::entities::MembershipState::Added { at },
            _ => crate::entities::MembershipState::Removed { at },
        }
    })
}

/// A small membership map over independent label ids.
pub fn membership_map_strategy() -> impl Strategy<
    Value = std::collections::BTreeMap<crate::ids::LabelId, crate::entities::MembershipState>,
> {
    prop::collection::btree_map(
        super::label_id_strategy(),
        membership_state_strategy(),
        0..3,
    )
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
            membership_map_strategy(),
            proptest::option::of(effort_strategy()),
            proptest::option::of(importance_strategy()),
            proptest::option::of(urgency_strategy()),
            (any::<bool>(), any::<bool>()),
            task_clocks_strategy(),
            proptest::option::of(timestamp_strategy()),
            proptest::option::of(remote_task_baseline_strategy()),
        ),
    )
        .prop_map(
            |(
                (id, remote, title, description, duedate, done, scheduled, stack),
                (
                    order,
                    label_memberships,
                    effort,
                    importance,
                    urgency,
                    (archived, deleted),
                    clocks,
                    remote_seen,
                    remote_baseline,
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
                label_memberships,
                effort,
                importance,
                urgency,
                recur: None,
                archived,
                deleted,
                clocks,
                remote_seen,
                remote_baseline,
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

/// A [`crate::entities::RemoteTaskBaseline`] over all fields: bounded
/// timestamps, small label sets. Independent of the task's bindings —
/// nothing in the domain cross-validates the record, and every consumer
/// (serde, merge gate, codec) is total on all values.
pub fn remote_task_baseline_strategy() -> impl Strategy<Value = crate::entities::RemoteTaskBaseline>
{
    (
        string_strategy(),
        string_strategy(),
        proptest::option::of(timestamp_strategy()),
        proptest::option::of(timestamp_strategy()),
        proptest::option::of(timestamp_strategy()),
        super::remote_number(),
        -1_000_i64..=1_000,
        prop::collection::btree_set(
            super::remote_number().prop_map(crate::ids::RemoteLabelId),
            0..3,
        ),
        any::<bool>(),
    )
        .prop_map(
            |(title, description, duedate, scheduled, done, stack, order, labels, archived)| {
                crate::entities::RemoteTaskBaseline {
                    title,
                    description,
                    duedate,
                    scheduled,
                    done,
                    stack: RemoteStackId(stack),
                    order,
                    labels,
                    archived,
                }
            },
        )
}
