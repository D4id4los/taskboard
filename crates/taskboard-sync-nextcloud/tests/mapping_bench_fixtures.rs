// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
//! The mapping bench fixture's contract (kiosk wave K7, stage-1 plan
//! §7): determinism, shape, and validity as bench input — the durable
//! guard against rng/strategy/clock ever sneaking into the builder.
//! Mapping semantics themselves are owned by `mapping.rs`'s unit tests.

#[path = "../benches/support/mod.rs"]
mod support;

use support::{
    LABEL_COUNT, N_SIZES, STACK_COUNT, build_fixture, card_shape, new_card_shape, steady_count,
};
use taskboard_domain::outbox::LocalOp;

/// Two runs at each N must produce the identical fixture — domain family
/// by value, wire family by serialized shape (the wire DTOs carry no
/// `PartialEq`; `serde_json` equality is the same structural fact).
#[test]
fn fixture_is_deterministic() {
    for n in N_SIZES {
        let first = build_fixture(n);
        let second = build_fixture(n);
        assert_eq!(first.board, second.board, "board at N={n}");
        assert_eq!(first.stacks, second.stacks, "stacks at N={n}");
        assert_eq!(first.labels, second.labels, "labels at N={n}");
        assert_eq!(first.tasks, second.tasks, "tasks at N={n}");
        assert_eq!(
            first.steady_outbox, second.steady_outbox,
            "steady outbox at N={n}"
        );
        assert_eq!(
            first.burst_outbox, second.burst_outbox,
            "burst outbox at N={n}"
        );
        assert_eq!(
            serde_json::to_value(&first.wire_board).unwrap(),
            serde_json::to_value(&second.wire_board).unwrap(),
            "wire board at N={n}"
        );
        assert_eq!(
            serde_json::to_value(&first.wire_stacks).unwrap(),
            serde_json::to_value(&second.wire_stacks).unwrap(),
            "wire stacks at N={n}"
        );
        assert_eq!(
            serde_json::to_value(&first.wire_labels).unwrap(),
            serde_json::to_value(&second.wire_labels).unwrap(),
            "wire labels at N={n}"
        );
        assert_eq!(
            serde_json::to_value(&first.wire_cards).unwrap(),
            serde_json::to_value(&second.wire_cards).unwrap(),
            "wire cards at N={n}"
        );
    }
}

/// Entity counts per the stage-1 plan's shape table: 1 board, 4 stacks,
/// 6 labels, n tasks/cards; steady outbox = n/10; burst outbox = n ops
/// with the five-kind rotation's exact per-kind partition.
#[test]
fn fixture_shape_matches_table() {
    for n in N_SIZES {
        let f = build_fixture(n);
        assert_eq!(f.board_map.len(), 1);
        assert_eq!(f.stack_map.len(), STACK_COUNT);
        assert_eq!(f.label_map.len(), LABEL_COUNT);
        assert_eq!(f.tasks.len(), n);
        assert_eq!(f.wire_stacks.len(), STACK_COUNT);
        assert_eq!(f.wire_labels.len(), LABEL_COUNT);
        assert_eq!(f.wire_cards.len(), n);
        assert_eq!(f.steady_outbox.len(), steady_count(n));
        assert_eq!(f.burst_outbox.len(), n);

        // Steady: all UpdateTask, distinct targets.
        let mut steady_targets = Vec::new();
        for op in &f.steady_outbox {
            let LocalOp::UpdateTask(task) = op.op else {
                panic!("steady outbox must hold only UpdateTask ops");
            };
            steady_targets.push(task);
        }
        steady_targets.sort();
        let before = steady_targets.len();
        steady_targets.dedup();
        assert_eq!(before, steady_targets.len(), "steady targets distinct");

        // Burst: exact per-kind counts of the [update, assign, create,
        // delete, move][i % 5] rotation, all op ids distinct.
        let expect = |i: usize| (n + 4 - i) / 5;
        let mut counts = [0_usize; 5];
        for op in &f.burst_outbox {
            match op.op {
                LocalOp::UpdateTask(_) => counts[0] += 1,
                LocalOp::AssignLabel(_, _) => counts[1] += 1,
                LocalOp::CreateTask(_) => counts[2] += 1,
                LocalOp::DeleteTask(_) => counts[3] += 1,
                LocalOp::MoveTask(_) => counts[4] += 1,
                _ => panic!("burst outbox must hold only task ops"),
            }
        }
        for (kind, count) in counts.iter().enumerate() {
            assert_eq!(*count, expect(kind), "burst kind {kind} at N={n}");
        }
        let mut op_ids: Vec<_> = f.burst_outbox.iter().map(|op| op.op_id.0).collect();
        op_ids.sort();
        let before = op_ids.len();
        op_ids.dedup();
        assert_eq!(before, op_ids.len(), "burst op ids distinct");

        // Card stamp identity: distinct ids and distinct last_modified.
        let mut stamps: Vec<_> = f.wire_cards.iter().map(|c| c.last_modified).collect();
        stamps.sort_unstable();
        stamps.dedup();
        assert_eq!(stamps.len(), n, "wire card stamps distinct");
        let mut ids: Vec<_> = f.wire_cards.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "wire card ids distinct");
    }
}

/// The modulo rules both fixture families share (the both-arms coverage
/// the mappers and planner rely on): `duedate` present iff i % 3 == 0,
/// `done` iff i % 10 == 0, label-set sizes ∈ {1, 2} — on the domain
/// tasks and the wire cards alike.
#[test]
fn fixture_card_field_arms_match_modulo_rules() {
    for n in N_SIZES {
        let f = build_fixture(n);
        let due = (0..n).filter(|i| i.is_multiple_of(3)).count();
        let done = (0..n).filter(|i| i.is_multiple_of(10)).count();

        assert_eq!(f.tasks.iter().filter(|t| t.duedate.is_some()).count(), due);
        assert_eq!(f.tasks.iter().filter(|t| t.done.is_some()).count(), done);
        assert_eq!(
            f.wire_cards.iter().filter(|c| c.duedate.is_some()).count(),
            due
        );
        assert_eq!(
            f.wire_cards.iter().filter(|c| c.done.is_some()).count(),
            done
        );

        for task in &f.tasks {
            assert!(
                task.live_labels().len() == 1 || task.live_labels().len() == 2,
                "domain label-set size must be 1 or 2"
            );
        }
        for card in &f.wire_cards {
            assert!(
                card.labels.len() == 1 || card.labels.len() == 2,
                "wire label-set size must be 1 or 2"
            );
        }
        assert!(n >= LABEL_COUNT, "every label id must occur in the sets");
        let mut seen = std::collections::BTreeSet::new();
        for card in &f.wire_cards {
            for label in &card.labels {
                assert!(
                    label.id >= 9 && label.id < 9 + LABEL_COUNT as u64,
                    "wire label ids must come from the fixture label family"
                );
                seen.insert(label.id);
            }
        }
        assert_eq!(
            seen.len(),
            LABEL_COUNT,
            "every fixture label must be referenced"
        );
    }
}

/// The planner shape builders mirror their tasks field-for-field (the
/// same list the private `from_task`s use) — the shapes the write-side
/// benches feed the mappers are faithful `plan_pushes` outputs.
#[test]
fn shape_builders_mirror_their_tasks() {
    let f = build_fixture(100);
    assert_eq!(f.task_map.len(), f.tasks.len(), "task table holds all n");
    for task in &f.tasks {
        let new = new_card_shape(task);
        assert_eq!(new.title, task.title);
        assert_eq!(new.order, task.order);
        assert_eq!(new.description, task.description);
        assert_eq!(new.duedate, task.duedate);
        assert_eq!(new.done, task.done);
        assert_eq!(new.archived, task.archived);
        assert_eq!(new.labels, task.live_labels());

        let update = card_shape(task);
        assert_eq!(update.title, task.title);
        assert_eq!(update.description, task.description);
        assert_eq!(update.duedate, task.duedate);
        assert_eq!(update.done, task.done);
        assert_eq!(update.order, task.order);
        assert_eq!(update.archived, task.archived);
        assert_eq!(update.labels, task.live_labels());
    }
}
