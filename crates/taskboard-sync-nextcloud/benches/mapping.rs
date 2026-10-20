// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
//! DTO↔domain mapping benches (kiosk wave K7; perf-budgets report S4):
//! the pure mapping surface at board scale — the read mappers (pull
//! leg), the write builders (push-leg payload construction), and the
//! pure `plan_pushes` planner called directly with `EntityTables` (never
//! through the actor). Every leg is CPU-only: no wiremock, no reqwest,
//! no tokio runtime, no DB — the full cycle itself is network-dominated
//! and stays out of scope by design.
//!
//! Fixture: `support::build_fixture`, the `taskboard-storage-sqlite`
//! hot-path conventions mirrored (N ∈ {100, 1000, 10000}, 4 stacks,
//! 6 labels, n/10 steady outbox ops, fixed stamp origin, disjoint
//! per-kind uuid bases, setup outside the measured closures, no tracing
//! subscriber). The wire family is built as struct literals, not serde
//! JSON — JSON would measure deserialization, and the wire shapes are
//! contract-tested by `model.rs`'s fixture-file tests.
//!
//! Skips, recorded so the "every arm enumerated" claim is demonstrably
//! true: the echo delegates (`map_echo`/`map_stack_echo`/
//! `map_label_echo` — one-line delegates to benched fns plus an enum
//! wrap, their numbers upper-bounded by those) and the O(1) error-path
//! classifiers `classify_push`/`classify_read` (once per *failed* op,
//! not a per-entity cycle leg).
//!
//! # Budget derivation (budget row 5: sync-cycle wall <= 30 s per typical board)
//!
//! The cycle's wall time is network-dominated (audit plan D7): only the
//! CPU share is in-repo measurable, and the groups above are that
//! share. Per cycle at board scale N = 10 000 (the largest fixture
//! tier):
//!
//! - pull side: `map_board` x1 + `map_stack` x4 + `map_label` x6 +
//!   `map_card` xN
//! - push side: `plan_pushes` on the steady outbox (N/10 ops); the
//!   burst variant (N ops, five-kind rotation) is the offline-session
//!   worst case
//! - write side: the builders, x the cycle's op counts (<= outbox
//!   depth)
//!
//! Dev-machine numbers (stage-2 run, 2026-10-20): pull ~ 1.25 ms, push
//! steady ~ 0.40 ms (burst ~ 5.38 ms), write ~ 3.52 ms; composed CPU
//! share ~ 5.2 ms of the 30 000 ms budget = 0.017% (burst worst case ~
//! 10.2 ms = 0.034%). Under the audit's D2 derate note (a full
//! order-of-magnitude in-order-core derate) the share stays <= 0.34% —
//! still a small fraction of the cycle budget (the S4 pass criterion).
//! Dev-machine references, never device conformance: the on-device
//! verdict is the kiosk spike's. No budget value is invented here
//! (audit plan D4) — the 30 s envelope is consumed from `§budgets`,
//! not re-derived.

mod support;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use taskboard_domain::push::{CardShape, EntityTables, NewCardShape};
use taskboard_sync_nextcloud::mapping::{
    apply_card_shape, board_changes_from, label_changes_from, map_board, map_card, map_label,
    map_stack, new_card_from, stack_changes_from,
};

use support::{N_SIZES, build_fixture, card_shape, new_card_shape, steady_count};

#[must_use]
fn element_throughput(n: usize) -> Throughput {
    Throughput::Elements(u64::try_from(n).expect("element count fits u64"))
}

fn bench_read_mappers(c: &mut Criterion) {
    let f = build_fixture(N_SIZES[2]);
    let board_id = taskboard_domain::ids::RemoteBoardId(77);

    let mut group = c.benchmark_group("map_board");
    group.throughput(element_throughput(1));
    group.bench_function("1", |b| {
        b.iter(|| map_board(std::hint::black_box(&f.wire_board)));
    });
    group.finish();

    let mut group = c.benchmark_group("map_stack");
    group.throughput(element_throughput(f.wire_stacks.len()));
    group.bench_function("4", |b| {
        b.iter(|| {
            f.wire_stacks
                .iter()
                .map(|s| map_stack(s, board_id))
                .collect::<Vec<_>>()
        });
    });
    group.finish();

    let mut group = c.benchmark_group("map_label");
    group.throughput(element_throughput(f.wire_labels.len()));
    group.bench_function("6", |b| {
        b.iter(|| {
            f.wire_labels
                .iter()
                .map(|l| map_label(l, board_id))
                .collect::<Vec<_>>()
        });
    });
    group.finish();

    let mut group = c.benchmark_group("map_card");
    for n in N_SIZES {
        let f = build_fixture(n);
        group.throughput(element_throughput(n));
        group.bench_function(format!("{n}"), |b| {
            b.iter(|| {
                f.wire_cards
                    .iter()
                    .map(|card| map_card(card, board_id))
                    .collect::<Vec<_>>()
            });
        });
    }
    group.finish();
}

fn bench_write_builders(c: &mut Criterion) {
    let mut group = c.benchmark_group("new_card_from");
    for n in N_SIZES {
        let f = build_fixture(n);
        let shapes: Vec<NewCardShape> = f.tasks.iter().map(new_card_shape).collect();
        group.throughput(element_throughput(n));
        group.bench_function(format!("{n}"), |b| {
            b.iter(|| shapes.iter().map(new_card_from).collect::<Vec<_>>());
        });
    }
    group.finish();

    let mut group = c.benchmark_group("apply_card_shape");
    for n in N_SIZES {
        let f = build_fixture(n);
        let shapes: Vec<CardShape> = f.tasks.iter().map(card_shape).collect();
        // Resolved remote label ids: the same fixture label family the
        // wire cards and domain tasks carry (9–14, 1–2 per card).
        let resolved: Vec<Vec<u64>> = (0..n)
            .map(|i| {
                let mut labels = vec![9 + u64::try_from(i % support::LABEL_COUNT).expect("fits")];
                if i % 2 == 1 {
                    labels.push(9 + u64::try_from((i + 3) % support::LABEL_COUNT).expect("fits"));
                }
                labels
            })
            .collect();
        group.throughput(element_throughput(n));
        group.bench_function(format!("{n}"), |b| {
            b.iter(|| {
                f.wire_cards
                    .iter()
                    .zip(&shapes)
                    .zip(&resolved)
                    .map(|((fetched, shape), labels)| {
                        apply_card_shape(fetched, shape, Some(labels))
                    })
                    .collect::<Vec<_>>()
            });
        });
    }
    group.finish();

    let f = build_fixture(N_SIZES[2]);

    let mut group = c.benchmark_group("board_changes_from");
    group.throughput(element_throughput(1));
    group.bench_function("1", |b| {
        b.iter(|| board_changes_from(std::hint::black_box(&f.board)));
    });
    group.finish();

    let mut group = c.benchmark_group("stack_changes_from");
    group.throughput(element_throughput(f.stacks.len()));
    group.bench_function("4", |b| {
        b.iter(|| f.stacks.iter().map(stack_changes_from).collect::<Vec<_>>());
    });
    group.finish();

    let mut group = c.benchmark_group("label_changes_from");
    group.throughput(element_throughput(f.labels.len()));
    group.bench_function("6", |b| {
        b.iter(|| f.labels.iter().map(label_changes_from).collect::<Vec<_>>());
    });
    group.finish();
}

fn bench_plan_pushes(c: &mut Criterion) {
    let mut group = c.benchmark_group("plan_pushes");
    for n in N_SIZES {
        let f = build_fixture(n);
        let tables = EntityTables {
            tasks: &f.task_map,
            stacks: &f.stack_map,
            labels: &f.label_map,
            boards: &f.board_map,
        };
        group.throughput(element_throughput(steady_count(n)));
        group.bench_function(format!("steady/{n}"), |b| {
            b.iter(|| {
                taskboard_domain::plan_pushes(std::hint::black_box(&f.steady_outbox), tables)
            });
        });
        group.throughput(element_throughput(n));
        group.bench_function(format!("burst/{n}"), |b| {
            b.iter(|| taskboard_domain::plan_pushes(std::hint::black_box(&f.burst_outbox), tables));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_read_mappers,
    bench_write_builders,
    bench_plan_pushes
);
criterion_main!(benches);
