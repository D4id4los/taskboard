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
            kind: ur,

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
                kind: crate::entities::TaskKind::Standard,
                archived,
                deleted,
                clocks,
                authors: crate::entities::TaskAuthors::default(),
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
            board: RemoteBoardId::deck(board),
            stack: RemoteStackId::deck(stack),
            card: RemoteCardId::deck(card),
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
            super::remote_number().prop_map(crate::ids::RemoteLabelId::deck),
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
                    stack: RemoteStackId::deck(stack),
                    order,
                    labels,
                    archived,
                }
            },
        )
}

// ---- deterministic entity mints (#1269) --------------------------------
//
// Deterministic counterparts to the strategies: `Uuid::from_u128(raw)`
// ids (the `CountingIds` spelling), epoch clocks ("never locally
// written"), and defaults for every other field. These are test-only
// fixtures — outside the trust boundary — so plain constructors with
// full defaults are the right shape; no fallible minting.

fn epoch() -> chrono::DateTime<chrono::Utc> {
    crate::entities::BoardClocks::epoch()
}

fn default_color() -> crate::entities::Color {
    crate::entities::Color::new(crate::entities::DEFAULT_BOARD_COLOR)
}

/// A live board with the deterministic id `raw` and `title`.
#[must_use]
pub fn mint_board(raw: u128, title: &str) -> crate::entities::Board {
    crate::entities::Board {
        id: crate::BoardId::from(uuid::Uuid::from_u128(raw)),
        remote: None,
        title: title.to_owned(),
        color: default_color(),
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

/// A live stack of `board` with the deterministic id `raw` and `title`
/// (order 0).
#[must_use]
pub fn mint_stack(raw: u128, board: crate::BoardId, title: &str) -> crate::entities::Stack {
    crate::entities::Stack {
        id: crate::StackId::from(uuid::Uuid::from_u128(raw)),
        remote: None,
        board,
        title: title.to_owned(),
        order: 0,
        archived: false,
        deleted: false,
        clocks: crate::entities::StackClocks {
            title: epoch(),
            order: epoch(),
            deleted: epoch(),
        },
        authors: crate::entities::StackAuthors::default(),
        remote_seen: None,
    }
}

/// A live, open, standard task of `stack` with the deterministic id
/// `raw` and `title` (order 0).
#[must_use]
pub fn mint_task(raw: u128, stack: crate::StackId, title: &str) -> crate::entities::Task {
    let e = epoch();
    crate::entities::Task {
        id: crate::TaskId::from(uuid::Uuid::from_u128(raw)),
        remote: None,
        title: title.to_owned(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack,
        order: 0,
        label_memberships: std::collections::BTreeMap::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: crate::entities::TaskKind::Standard,
        archived: false,
        deleted: false,
        clocks: crate::entities::TaskClocks {
            title: e,
            description: e,
            duedate: e,
            scheduled: e,
            done: e,
            position: e,
            effort: e,
            importance: e,
            urgency: e,
            recur: e,
            kind: e,
            archived: e,
            deleted: e,
        },
        authors: crate::entities::TaskAuthors::default(),
        remote_seen: None,
        remote_baseline: None,
    }
}

/// A live label of `board` with the deterministic id `raw` and `title`.
#[must_use]
pub fn mint_label(raw: u128, board: crate::BoardId, title: &str) -> crate::entities::Label {
    crate::entities::Label {
        id: crate::LabelId::from(uuid::Uuid::from_u128(raw)),
        remote: None,
        board,
        title: title.to_owned(),
        color: default_color(),
        deleted: false,
        clocks: crate::entities::LabelClocks {
            title: epoch(),
            color: epoch(),
            deleted: epoch(),
        },
        authors: crate::entities::LabelAuthors::default(),
        remote_seen: None,
    }
}

#[cfg(test)]
mod mint_tests {
    use super::*;

    /// One determinism pin, lowest layer (AGENTS §5): the same raw
    /// mints the same entity twice.
    #[test]
    fn the_same_raw_mints_the_same_entity_twice() {
        let a = mint_board(7, "board");
        let b = mint_board(7, "board");
        assert_eq!(a, b);
        assert_eq!(
            a.id,
            crate::BoardId::from(uuid::Uuid::from_u128(7)),
            "the id is exactly Uuid::from_u128(raw)"
        );
        let t1 = mint_task(3, crate::StackId::from(uuid::Uuid::from_u128(9)), "t");
        let t2 = mint_task(3, crate::StackId::from(uuid::Uuid::from_u128(9)), "t");
        assert_eq!(t1, t2);
        assert_eq!(
            t1.id,
            crate::TaskId::from(uuid::Uuid::from_u128(3)),
            "the id is exactly Uuid::from_u128(raw)"
        );
    }
}
