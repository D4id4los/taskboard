// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
//! The bench fixture's contract (plan D7): determinism, shape, and
//! validity as bench input. These tests pin what the benches rely on;
//! repository semantics themselves are owned by `roundtrip.rs`.

#[path = "../benches/support/mod.rs"]
mod support;

use std::collections::BTreeSet;

use support::{LABEL_COUNT, N_SIZES, STACK_COUNT, build_fixture, outbox_count, rule_count};
use taskboard_domain::persistence::TaskRepository;
use taskboard_storage_sqlite::open_memory;

/// Two runs at each N must produce the identical state — the dispatch
/// acceptance "two runs, identical input shape" as a durable regression
/// test against rng/strategy/clock ever sneaking into the builder.
#[test]
fn fixture_is_deterministic() {
    for n in N_SIZES {
        let first = build_fixture(n).state;
        let second = build_fixture(n).state;
        assert_eq!(first, second, "fixture at N={n} must be deterministic");
    }
}

/// Entity counts, label-set sizes, and stamp monotonicity per the plan's
/// shape table — the N-scaling contract Bencher trends rely on.
#[test]
fn fixture_shape_matches_table() {
    for n in N_SIZES {
        let state = build_fixture(n).state;
        assert_eq!(state.boards.len(), 1);
        assert_eq!(state.stacks.len(), STACK_COUNT);
        assert_eq!(state.labels.len(), LABEL_COUNT);
        assert_eq!(state.tasks.len(), n);
        assert_eq!(state.outbox.len(), outbox_count(n));
        assert_eq!(state.reminders.rules.len(), rule_count(n));
        assert_eq!(state.validators.len(), 2);
        assert!(state.reminders.deliveries.is_empty());
        assert!(state.reminders.snoozes.is_empty());
        assert!(state.reminders.muted.is_empty());

        let mut clock_stamps = state
            .tasks
            .values()
            .map(|t| t.clocks.title)
            .collect::<Vec<_>>();
        clock_stamps.sort();
        clock_stamps.dedup();
        assert_eq!(
            clock_stamps.len(),
            n,
            "task write stamps must be distinct and increasing"
        );

        for task in state.tasks.values() {
            assert!(
                task.labels.len() == 1 || task.labels.len() == 2,
                "label-set size must be 1 or 2"
            );
        }
    }
}

/// The fixture is valid persistence input: the whole-state batch applies
/// to a fresh database and loads back per-field, with `pending_ops`
/// re-derived to the outbox depth (N/10). The fixture's contract as bench
/// input, not a repository re-test.
#[test]
fn fixture_is_valid_persistence_input() {
    let n = 100;
    let fixture = build_fixture(n);
    let mut expected = fixture.state.clone();
    // The per-board projection is never stored: load() re-derives it
    // from the outbox attribution, so the fixture's zero is not what
    // comes back. Reproduce the same derivation over the fixture.
    taskboard_domain::derive_sync_states(
        &expected.boards,
        &expected.stacks,
        &expected.tasks,
        &expected.labels,
        &expected.outbox,
        &mut expected.sync,
    );

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async move {
            let repo = open_memory(taskboard_domain::ConnectorId::from("nextcloud"))
                .await
                .expect("open");
            repo.apply(fixture.actions).await.expect("apply");
            let loaded = repo.load().await.expect("load");

            assert_eq!(loaded, expected);
            assert_eq!(
                loaded.sync.aggregate().pending_ops as usize,
                outbox_count(n),
                "pending_ops must be re-derived from the outbox depth"
            );
            let label_sizes: BTreeSet<usize> =
                loaded.tasks.values().map(|t| t.labels.len()).collect();
            assert!(label_sizes.is_subset(&BTreeSet::from([1, 2])));
        });
}
