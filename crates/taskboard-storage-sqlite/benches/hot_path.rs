// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
//! Persistence hot-path benches (dispatch chunk P3): full-hydration
//! `load()` and one whole-state `apply` batch at N ∈ {100, 1000, 10000}
//! on a file-backed database (the production `open` recipe: WAL,
//! synchronous=NORMAL, FK on, single connection).
//!
//! Setup happens once per bench function, outside the measured closure,
//! so measured `apply` iterations always hit the `ON CONFLICT DO UPDATE`
//! branch — the steady-state path a running kiosk exercises. No tracing
//! subscriber is installed on purpose (plan D9/§1.11): every event in
//! the measured path must stay a disabled-dispatcher no-op.

mod support;

use std::time::Duration;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use taskboard_domain::persistence::TaskRepository;
use taskboard_storage_sqlite::open;

use support::{N_SIZES, build_fixture};

/// Per-N measurement budget (plan D6): N=10000 iterations cost
/// hundreds of ms each, so only that tier drops to criterion's minimum
/// sample size and a bounded measurement time.
fn configure_group(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    n: usize,
) {
    if n >= 10_000 {
        group.sample_size(10);
        group.measurement_time(Duration::from_secs(15));
    }
}

#[must_use]
fn element_throughput(n: usize) -> Throughput {
    Throughput::Elements(u64::try_from(n).expect("task count fits u64"))
}

/// Populate one fresh file-backed database with the fixture's batch and
/// verify it loads back exactly the fixture's N tasks — a loud panic if
/// the bench would silently measure an empty database.
fn populate(dir: &tempfile::TempDir, n: usize) -> (tokio::runtime::Runtime, impl TaskRepository) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let fixture = build_fixture(n);
    let expected = fixture.state.tasks.len();
    let repo = rt
        .block_on(async { open(dir.path().join("bench.db").as_path()).await })
        .expect("open file-backed repository");
    rt.block_on(repo.apply(fixture.actions))
        .expect("population apply");
    let loaded = rt.block_on(repo.load()).expect("population load");
    assert_eq!(
        loaded.tasks.len(),
        expected,
        "populated database must hold exactly N tasks"
    );
    (rt, repo)
}

fn bench_load(c: &mut Criterion) {
    for n in N_SIZES {
        let dir = tempfile::tempdir().expect("temp dir");
        let (rt, repo) = populate(&dir, n);
        let mut group = c.benchmark_group("load");
        configure_group(&mut group, n);
        group.throughput(element_throughput(n));
        group.bench_function(format!("{n}"), |b| b.iter(|| rt.block_on(repo.load())));
        group.finish();
    }
}

fn bench_apply(c: &mut Criterion) {
    for n in N_SIZES {
        let dir = tempfile::tempdir().expect("temp dir");
        let (rt, repo) = populate(&dir, n);
        let actions = build_fixture(n).actions;
        let mut group = c.benchmark_group("apply");
        configure_group(&mut group, n);
        group.throughput(element_throughput(n));
        // `apply` consumes the batch; clone it per iteration (outside
        // the measurement) so every iteration runs the identical batch.
        group.bench_function(format!("{n}"), |b| {
            b.iter_batched(
                || actions.clone(),
                |batch| rt.block_on(repo.apply(batch)),
                BatchSize::PerIteration,
            );
        });
        group.finish();
    }
}

criterion_group!(benches, bench_load, bench_apply);
criterion_main!(benches);
