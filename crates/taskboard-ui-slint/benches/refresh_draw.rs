// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Refresh+draw benches (kiosk chunk 6, K6/S2 — perf-budgets report
//! row 3): the refresh leg and the software draw leg of one full UI
//! pass, **timed separately, never one combined number**, at board
//! scale N ∈ {100, 1 000, 10 000} on the deterministic fixture.
//!
//! Budget context (row 3, stated not asserted): full-frame repaint
//! ≤ 50 ms / partial ≤ 20 ms @ 1366×768 software renderer on one
//! 1.5 GHz in-order core. These are **dev-machine references, never
//! gates** — the on-device verdict belongs to the phase-B spike. The
//! composed input→visible latency stays floored by the 100 ms repaint
//! cadence on any hardware (C2/F13-09, recorded; not measured here —
//! the legs are timer-free by design).
//!
//! Lanes (plan D8):
//!
//! - `refresh/{n}` — the public `UiHandle::refresh()` pass:
//!   `ArcSwap` load → `view_state.reconcile` → `models::push`
//!   `VecModel` rebuild (`adapter.rs` refresh). The real clock stays
//!   in (the production path; µs variance against a ms budget).
//! - `draw-full/{n}` — one software render into a pre-allocated
//!   1366×768 caller buffer, default `NewBuffer` repaint type (full
//!   window redrawn each call).
//! - `draw-partial/{n}` — `ReusedBuffer` dirty tracking over the
//!   kiosk steady-state framebuffer model: unmeasured per-iteration
//!   setup (one task's `done` toggled → `ArcSwap` store → refresh),
//!   measured routine = one render. The reported region area is the
//!   honest finding-carrier: if the whole-`VecModel` replacement
//!   dirties the entire board every refresh, this lane shows
//!   full-window regions — a recorded finding (feeds C2/F13-09 and
//!   the spike's event-driven-redraw question), never hidden.
//!
//! No GL/llvmpipe path anywhere: the software renderer is pure CPU
//! over a caller buffer (`slint::platform::software_renderer`, the
//! linuxkms production path). No tracing subscriber is installed
//! (the mount-tail `taskboard_perf` event stays a
//! disabled-dispatcher no-op — the storage bench's D9 stance). No
//! visual-correctness assertions (the golden lane owns those).

#[allow(dead_code)] // `on_slint_thread` serves the lib's unit tests only
#[path = "../src/test_platform.rs"]
mod seam;

#[path = "support/mod.rs"]
mod support;

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use chrono::TimeZone as _;
use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use slint::ComponentHandle as _;
use taskboard_domain::TaskId;
use taskboard_ui_slint::requests;
use taskboard_ui_slint::{UiHandle, UiSettings, mount};

use support::{BENCH_HEIGHT, BENCH_WIDTH, N_SIZES, build_state};

/// The seam adapter: the mounted window's `NullWindowAdapter` (render
/// pass-throughs), reached through the seam's registry.
struct Bench {
    handle: UiHandle,
    adapter: std::rc::Rc<seam::NullWindowAdapter>,
    task_ids: Vec<TaskId>,
    state: Arc<ArcSwap<taskboard_domain::AppState>>,
}

/// The fixed `done` stamp the partial lane flips un-done tasks to
/// (arbitrary fixed instant; the fixture never produces it — its own
/// stamps come from `ts(order + 2_000_000)`).
const PARTIAL_STAMP_SECS: i64 = 1_704_067_200 + 9_000_000;

/// Platform guard (plan D9): slint's platform is process-global and
/// thread-affine; criterion (no rayon) runs everything on the main
/// thread, which is where the null platform registers — once.
fn ensure_platform() {
    use std::sync::OnceLock;
    static PLATFORM: OnceLock<()> = OnceLock::new();
    PLATFORM.get_or_init(|| {
        slint::platform::set_platform(Box::new(seam::NullPlatform))
            .expect("the null platform registers exactly once per bench process");
    });
}

/// Mounts the fixture state windowed at the row-3 budget's own
/// 1366×768 (plan D8/D10: the budget row is stated in
/// software-renderer terms at a size, not fullscreen-kiosk terms),
/// pushes the first refresh, and hands back the seam halves.
fn mounted(n: usize) -> Bench {
    ensure_platform();
    let state = Arc::new(ArcSwap::from_pointee(build_state(n)));
    let (commands_tx, _rx) = tokio::sync::mpsc::channel(64);
    let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
    let (_stx, signals) = tokio::sync::broadcast::channel(16);
    let (_syt, system) = tokio::sync::broadcast::channel(16);
    let handle = mount(
        Arc::clone(&state),
        signals,
        system,
        commands_tx,
        requests_tx,
        UiSettings::default(),
    )
    .expect("mount succeeds on the null platform");
    handle
        .window()
        .window()
        .set_size(slint::WindowSize::Physical(slint::PhysicalSize::new(
            u32::try_from(BENCH_WIDTH).expect("width fits u32"),
            u32::try_from(BENCH_HEIGHT).expect("height fits u32"),
        )));
    let adapter = seam::window_adapter().expect("the seam registry holds the mounted adapter");
    let task_ids = state.load().tasks.keys().copied().collect::<Vec<_>>();
    handle.refresh();
    Bench {
        handle,
        adapter,
        task_ids,
        state,
    }
}

/// Per-N measurement budget (the storage benches' convention): the
/// 10 000 tier drops to criterion's minimum sample size and a bounded
/// measurement time (plan D8; risk 5's deviation threshold: a
/// pathological > 5 s/iter tier would drop to 10 s, recorded).
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

/// The refresh leg (plan D5): the entire audit-defined refresh pass
/// through the public `UiHandle::refresh()` — no production code
/// changes, no `pub(crate)` widening.
fn refresh_lanes(c: &mut Criterion) {
    let mut group = c.benchmark_group("refresh");
    for n in N_SIZES {
        group.throughput(element_throughput(n));
        configure_group(&mut group, n);
        let bench = mounted(n);
        group.bench_with_input(criterion::BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| bench.handle.refresh());
        });
    }
    group.finish();
}

/// The full-frame draw leg (plan D6): pre-allocated buffer outside
/// the measured closure, default `NewBuffer` repaint type. The setup
/// render's region is the structural guard — a lane that measures
/// anything less than the full window panics loudly rather than
/// silently mis-reporting.
fn draw_full_lanes(c: &mut Criterion) {
    let mut group = c.benchmark_group("draw-full");
    for n in N_SIZES {
        group.throughput(element_throughput(n));
        configure_group(&mut group, n);
        let bench = mounted(n);
        let mut buffer = vec![slint::Rgb8Pixel::new(0, 0, 0); BENCH_WIDTH * BENCH_HEIGHT];
        let stride = BENCH_WIDTH;
        let warmup = bench.adapter.render_into(&mut buffer, stride);
        assert_eq!(
            warmup.bounding_box_size(),
            slint::PhysicalSize::new(
                u32::try_from(BENCH_WIDTH).expect("width fits u32"),
                u32::try_from(BENCH_HEIGHT).expect("height fits u32"),
            ),
            "draw-full/{n} must measure a full-frame render (NewBuffer)"
        );
        eprintln!(
            "[refresh_draw] draw-full/{n}: warmup region {}×{} (full frame)",
            warmup.bounding_box_size().width,
            warmup.bounding_box_size().height
        );
        group.bench_with_input(criterion::BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| bench.adapter.render_into(&mut buffer, stride));
        });
    }
    group.finish();
}

/// The partial draw leg (plan D6): `ReusedBuffer` dirty tracking over
/// the kiosk steady-state framebuffer. Unmeasured per-iteration
/// setup toggles one task's `done` flag (rotating index), stores a
/// fresh `Arc<AppState>`, and refreshes; the measured routine is one
/// render. The reported region area is the finding-carrier (D6).
fn draw_partial_lanes(c: &mut Criterion) {
    let mut group = c.benchmark_group("draw-partial");
    for n in N_SIZES {
        group.throughput(element_throughput(n));
        configure_group(&mut group, n);
        let bench = mounted(n);
        bench.adapter.set_repaint_buffer_type(
            slint::platform::software_renderer::RepaintBufferType::ReusedBuffer,
        );
        let mut buffer = vec![slint::Rgb8Pixel::new(0, 0, 0); BENCH_WIDTH * BENCH_HEIGHT];
        let stride = BENCH_WIDTH;
        let full_area = u64::from(
            bench
                .adapter
                .render_into(&mut buffer, stride)
                .bounding_box_size()
                .width,
        ) * u64::try_from(BENCH_HEIGHT).expect("height fits u64");
        let rotate = std::cell::Cell::new(0_usize);
        let task_ids = bench.task_ids.clone();
        let report = std::cell::Cell::new(None::<(u32, u32)>);
        let last = std::cell::Cell::new(None::<(u32, u32)>);
        group.bench_with_input(criterion::BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter_batched(
                || {
                    let i = rotate.get();
                    rotate.set((i + 1) % task_ids.len().max(1));
                    let mut next = (**bench.state.load()).clone();
                    if let Some(task) = next.tasks.get_mut(&task_ids[i % task_ids.len()]) {
                        // Flip the task's *current* done value: the
                        // mutation must change the row every iteration,
                        // or the row-diff correctly dirties nothing and
                        // the lane silently measures an unchanged
                        // refresh (the 2026-10-26 re-measure finding).
                        task.done = if task.done.is_none() {
                            chrono::Utc.timestamp_opt(PARTIAL_STAMP_SECS, 0).single()
                        } else {
                            None
                        };
                    }
                    bench.state.store(Arc::new(next));
                    bench.handle.refresh();
                },
                |()| {
                    let region = bench.adapter.render_into(&mut buffer, stride);
                    let (w, h) = (
                        region.bounding_box_size().width,
                        region.bounding_box_size().height,
                    );
                    // Keep the max-area region across iterations: the
                    // lane's contract is "one changed row per render",
                    // so the max is the honest worst case and immune
                    // to last-iteration sampling. The last iteration is
                    // reported alongside it: max ≫ last means the big
                    // region came from warm-up (e.g. the first render
                    // on a freshly switched ReusedBuffer), not from
                    // the steady-state per-change repaint.
                    let prev = report.get().unwrap_or((0, 0));
                    if u64::from(w) * u64::from(h) >= u64::from(prev.0) * u64::from(prev.1) {
                        report.set(Some((w, h)));
                    }
                    last.set(Some((w, h)));
                },
                BatchSize::PerIteration,
            );
        });
        let (w, h) = report.get().expect("at least one measured render ran");
        let area = u64::from(w) * u64::from(h);
        let share = area
            .checked_mul(100)
            .and_then(|v| v.checked_div(full_area))
            .unwrap_or(0);
        eprintln!(
            "[refresh_draw] draw-partial/{n}: region {w}×{h} ({area} px = {share}% of the \
             full frame; 100% = the whole-VecModel-replacement dirt, the D6 recorded finding)"
        );
        let (lw, lh) = last.get().expect("at least one measured render ran");
        let larea = u64::from(lw) * u64::from(lh);
        let lshare = larea
            .checked_mul(100)
            .and_then(|v| v.checked_div(full_area))
            .unwrap_or(0);
        eprintln!(
            "[refresh_draw] draw-partial/{n}: last-iteration region {lw}×{lh} ({larea} px = \
             {lshare}% of the full frame)"
        );
    }
    group.finish();
}

criterion_group!(benches, refresh_lanes, draw_full_lanes, draw_partial_lanes);
criterion_main!(benches);
